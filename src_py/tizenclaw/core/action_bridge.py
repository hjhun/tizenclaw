"""
TizenClaw Action Bridge — Tizen Action Framework integration via ctypes FFI.

Binds to libcapi-appfw-tizen-action.so.1 shared library.
Provides:
  - Action discovery: enumerate available system actions
  - Action execution: invoke actions by name with parameters
  - Schema retrieval: get action parameter schemas
  - Event handling: receive action execution results

The Tizen Action Framework allows applications to register
"actions" that can be invoked by other apps. TizenClaw uses
this to bridge LLM tool calls to native Tizen actions.
"""
import asyncio
import ctypes
import json
import logging
import os
import time
from typing import Dict, List, Any, Optional, Callable

logger = logging.getLogger(__name__)

# Error codes
ACTION_ERROR_NONE = 0
ACTION_ERROR_INVALID_PARAMETER = -22
ACTION_ERROR_OUT_OF_MEMORY = -12

# ── Callback prototypes ──

# action_client_foreach_action_cb: bool (*)(action_h, void*)
ACTION_FOREACH_CB = ctypes.CFUNCTYPE(
    ctypes.c_bool,
    ctypes.c_void_p,    # action_h
    ctypes.c_void_p,    # user_data
)

# action_client_execute result callback (if async)
ACTION_RESULT_CB = ctypes.CFUNCTYPE(
    None,
    ctypes.c_void_p,    # action_client_h
    ctypes.c_int,       # result code
    ctypes.c_void_p,    # result data
    ctypes.c_void_p,    # user_data
)


class ActionInfo:
    """Information about a discovered Tizen Action."""
    def __init__(self, name: str = "", schema: str = "",
                 native_handle: ctypes.c_void_p = None):
        self.name = name
        self.schema = schema
        self.native_handle = native_handle

    def to_dict(self) -> Dict[str, Any]:
        result = {"name": self.name}
        if self.schema:
            try:
                result["schema"] = json.loads(self.schema)
            except json.JSONDecodeError:
                result["schema_raw"] = self.schema
        return result


class ActionBridge:
    """
    Bridge between TizenClaw and Tizen Action Framework.
    Uses ctypes FFI to call libcapi-appfw-tizen-action.so.1.
    """

    def __init__(self):
        self._lib = None
        self._client_handle = ctypes.c_void_p(0)
        self._actions: Dict[str, ActionInfo] = {}
        self._initialized = False
        # Must keep callback references alive
        self._foreach_cb_ref = None
        self._result_cb_ref = None

    def _load_lib(self) -> bool:
        """Load the Tizen Action Framework shared library."""
        try:
            self._lib = ctypes.CDLL("libcapi-appfw-tizen-action.so.1")
            logger.info("ActionBridge: libcapi-appfw-tizen-action.so.1 loaded")
            return True
        except OSError as e:
            logger.error(f"ActionBridge: Failed to load library: {e}")
            return False

    def initialize(self) -> bool:
        """Create action client and discover available actions."""
        if not self._load_lib():
            return False

        # action_client_create(action_client_h *handle) -> int
        handle = ctypes.c_void_p(0)
        ret = self._lib.action_client_create(ctypes.byref(handle))
        if ret != ACTION_ERROR_NONE:
            logger.error(f"ActionBridge: action_client_create failed: {ret}")
            return False

        self._client_handle = handle
        self._initialized = True
        logger.info("ActionBridge: Client created")

        # Discover available actions
        self._discover_actions()

        return True

    def shutdown(self):
        """Destroy action client."""
        if self._lib and self._client_handle:
            # Destroy any cloned action handles
            for action_info in self._actions.values():
                if action_info.native_handle:
                    try:
                        self._lib.action_destroy(action_info.native_handle)
                    except Exception:
                        pass

            self._lib.action_client_destroy(self._client_handle)
            self._client_handle = ctypes.c_void_p(0)
            self._actions.clear()
            logger.info("ActionBridge: Destroyed")
        self._initialized = False

    def _discover_actions(self):
        """Enumerate all available actions via action_client_foreach_action."""
        if not self._initialized:
            return

        discovered = []

        def _on_action(action_h, user_data):
            """Callback for each discovered action."""
            try:
                # action_get_name(action_h, char **name) -> int
                name_ptr = ctypes.c_char_p()
                ret = self._lib.action_get_name(action_h, ctypes.byref(name_ptr))
                if ret != ACTION_ERROR_NONE or not name_ptr.value:
                    return True  # continue enumeration

                name = name_ptr.value.decode("utf-8", errors="replace")

                # action_get_schema(action_h, char **schema) -> int
                schema_str = ""
                try:
                    schema_ptr = ctypes.c_char_p()
                    ret2 = self._lib.action_get_schema(action_h, ctypes.byref(schema_ptr))
                    if ret2 == ACTION_ERROR_NONE and schema_ptr.value:
                        schema_str = schema_ptr.value.decode("utf-8", errors="replace")
                except Exception:
                    pass

                # Clone the action handle for later use
                cloned = ctypes.c_void_p(0)
                try:
                    ret3 = self._lib.action_clone(action_h, ctypes.byref(cloned))
                    if ret3 != ACTION_ERROR_NONE:
                        cloned = None
                except Exception:
                    cloned = None

                discovered.append(ActionInfo(
                    name=name,
                    schema=schema_str,
                    native_handle=cloned,
                ))
            except Exception as e:
                logger.error(f"ActionBridge: Enumeration error: {e}")

            return True  # continue

        self._foreach_cb_ref = ACTION_FOREACH_CB(_on_action)
        self._lib.action_client_foreach_action(
            self._client_handle,
            self._foreach_cb_ref,
            None,  # user_data
        )

        for info in discovered:
            self._actions[info.name] = info

        logger.info(f"ActionBridge: Discovered {len(self._actions)} actions")

    def list_actions(self) -> List[Dict[str, Any]]:
        """List all discovered actions."""
        return [info.to_dict() for info in self._actions.values()]

    def get_action_schema(self, action_name: str) -> Optional[Dict[str, Any]]:
        """Get the parameter schema for a specific action."""
        info = self._actions.get(action_name)
        if not info:
            return None
        return info.to_dict()

    def execute_action(self, action_name: str,
                       parameters: Dict[str, Any] = None) -> str:
        """Execute an action by name with optional parameters.

        Returns JSON result string.
        """
        if not self._initialized:
            return json.dumps({"error": "ActionBridge not initialized"})

        info = self._actions.get(action_name)
        if not info:
            return json.dumps({"error": f"Action '{action_name}' not found"})

        # Build parameters as bundle (JSON string for simplicity)
        params_json = json.dumps(parameters or {}).encode("utf-8")

        try:
            # action_client_execute(client_h, action_name, params, ...) -> int
            ret = self._lib.action_client_execute(
                self._client_handle,
                action_name.encode("utf-8"),
                params_json,
            )

            if ret != ACTION_ERROR_NONE:
                return json.dumps({
                    "error": f"action_client_execute failed: {ret}",
                    "action": action_name,
                })

            return json.dumps({
                "status": "executed",
                "action": action_name,
                "parameters": parameters,
            })
        except Exception as e:
            return json.dumps({"error": f"Execution error: {e}"})

    async def execute_action_async(self, action_name: str,
                                   parameters: Dict[str, Any] = None) -> str:
        """Async wrapper for execute_action."""
        loop = asyncio.get_running_loop()
        return await loop.run_in_executor(
            None, self.execute_action, action_name, parameters
        )

    def get_tool_declarations(self) -> List[Dict[str, Any]]:
        """Convert discovered actions to LLM tool declarations.

        This allows the LLM to call Tizen actions as tools.
        """
        tools = []
        for name, info in self._actions.items():
            tool_decl = {
                "name": f"tizen_action_{name}",
                "description": f"Tizen Action: {name}",
                "parameters": {"type": "object", "properties": {}},
            }

            # Parse schema if available
            if info.schema:
                try:
                    schema = json.loads(info.schema)
                    if isinstance(schema, dict):
                        tool_decl["parameters"] = schema
                        if "description" in schema:
                            tool_decl["description"] = schema["description"]
                except json.JSONDecodeError:
                    pass

            tools.append(tool_decl)

        return tools

    def get_status(self) -> Dict[str, Any]:
        return {
            "initialized": self._initialized,
            "actions_count": len(self._actions),
            "action_names": list(self._actions.keys()),
        }


# ── Singleton ──

_bridge: Optional[ActionBridge] = None

def get_action_bridge() -> ActionBridge:
    global _bridge
    if _bridge is None:
        _bridge = ActionBridge()
    return _bridge
