import logging
import json
import time
import os
import asyncio
from typing import Dict, Any, Optional
from tizenclaw.core.tool_indexer import ToolIndexer
from tizenclaw.infra.container_engine import ContainerEngine

logger = logging.getLogger(__name__)

# Default timeout for CLI tool execution (seconds)
DEFAULT_CLI_TIMEOUT = 30


class ToolDispatcher:
    """
    Validates LLM tool calls against the ToolIndexer and executes them
    by dispatching to the secure ContainerEngine via abstract namespace IPC.

    Integrated with:
      - ToolPolicy: blocklist, rate-limit, loop detection
      - AuditLogger: persistent audit trail for all tool calls
      - EventBus: publishes tool_call events for other subsystems
      - HealthMonitor: increments tool call counters
    """
    def __init__(self, indexer: ToolIndexer, container_engine: ContainerEngine,
                 tool_policy=None, audit_logger=None, event_bus=None,
                 health_monitor=None):
        self.indexer = indexer
        self.container = container_engine
        self.policy = tool_policy
        self.audit = audit_logger
        self.event_bus = event_bus
        self.health = health_monitor

    def _resolve_cli_command(self, metadata: Dict[str, Any]) -> str:
        """Resolve the CLI executable for a tool.

        Priority:
          1. ``binary`` — native ELF path
          2. ``command`` — explicit command string from .tool.md frontmatter
          3. tool ``name`` — fallback (assumes it's in $PATH)
        """
        if metadata.get("binary"):
            return metadata["binary"]
        if metadata.get("command"):
            return metadata["command"]
        return metadata.get("name", "")

    async def execute_tool(self, name: str, args: Dict[str, Any],
                           session_id: str = "") -> str:
        metadata = self.indexer.get_tool_metadata(name)
        if not metadata:
            return f"Error: Tool '{name}' not found or not registered."

        # ── Policy check ──
        if self.policy:
            allowed, reason = self.policy.check(name)
            if not allowed:
                logger.warning(f"ToolPolicy blocked '{name}': {reason}")
                if self.audit:
                    self.audit.log_security_event(
                        f"tool_blocked:{name}", reason
                    )
                return f"Error: {reason}"

        tool_type = metadata.get("type", "cli")
        args_str = args.get("arguments", "")
        if isinstance(args_str, dict):
            args_str = json.dumps(args_str)

        logger.info(f"Dispatching tool '{name}' (Type: {tool_type})")
        t0 = time.time()

        try:
            if tool_type == "cli":
                command = self._resolve_cli_command(metadata)
                logger.info(f"CLI command resolved: {command}")
                result = await self.container.execute_cli_tool(
                    command, args_str, DEFAULT_CLI_TIMEOUT
                )
            elif tool_type == "skill":
                path = metadata.get("path", "")
                result = await self.container.execute_skill(path, args_str)
            elif tool_type == "mcp":
                result = await self.container.execute_mcp_tool(name, args_str)
            elif tool_type == "embedded":
                if name == "generate_web_app":
                    result = await self._handle_generate_web_app(args)
                else:
                    result = f"Error: Embedded tool {name} is not implemented in Python port yet."
            else:
                result = f"Error: Unknown tool type '{tool_type}'"
        except Exception as e:
            logger.error(f"Tool execution failed for '{name}': {e}")
            result = f"Internal Execution Error: {e}"

        elapsed_ms = int((time.time() - t0) * 1000)

        # ── Record call for rate-limit / loop detection ──
        if self.policy:
            self.policy.record_call(name)

        # ── Audit logging ──
        if self.audit:
            self.audit.log_tool_call(
                tool_name=name,
                arguments=args_str,
                result=result,
                session_id=session_id,
                duration_ms=elapsed_ms,
            )

        # ── Health counter ──
        if self.health:
            self.health.increment_tool_call()

        # ── Publish event ──
        if self.event_bus:
            from tizenclaw.core.event_bus import Event
            await self.event_bus.publish_fire_and_forget(Event(
                topic="tool.executed",
                data={"tool": name, "args": args_str[:200],
                      "duration_ms": elapsed_ms,
                      "success": not result.startswith("Error")},
                source="tool_dispatcher",
            ))

        return result

    async def _handle_generate_web_app(self, args: Dict[str, Any]) -> str:
        """
        Implements the generate_web_app embedded tool.
        Saves HTML/CSS/JS to the web server directory and launches the tizenclaw-webview app.
        """
        app_id = args.get("app_id")
        title = args.get("title", "Web App")
        html = args.get("html")
        css = args.get("css", "")
        js = args.get("js", "")
        allowed_tools = args.get("allowed_tools", [])

        if not app_id or not html:
            return "Error: app_id and html are required to generate web app."

        # Sanitize app_id
        app_id = "".join(c for c in app_id if c.isalnum() or c == "_").lower()

        # Web App directory
        app_dir = os.path.join("/opt/usr/share/tizenclaw/web/apps", app_id)
        os.makedirs(app_dir, exist_ok=True)

        try:
            with open(os.path.join(app_dir, "index.html"), "w", encoding="utf-8") as f:
                f.write(html)
            if css:
                with open(os.path.join(app_dir, "style.css"), "w", encoding="utf-8") as f:
                    f.write(css)
            if js:
                with open(os.path.join(app_dir, "app.js"), "w", encoding="utf-8") as f:
                    f.write(js)
            if allowed_tools:
                with open(os.path.join(app_dir, "permissions.json"), "w", encoding="utf-8") as f:
                    json.dump({"allowed_tools": allowed_tools}, f)

            url = f"http://127.0.0.1:8080/apps/{app_id}/index.html"
            logger.info(f"Generated web app: {app_id} -> {url}")

            # Send app control to launch tizenclaw-webview
            args_str = f"launch --app-id org.tizenclaw.webview --operation http://tizen.org/appcontrol/operation/view --uri {url}"
            result = await self.execute_tool("tizen-app-manager-cli", {"arguments": args_str})
            
            # Simple check if execution encountered error or command failure
            if "Error" in result or "error" in result.lower():
                logger.warning(f"Failed to launch webview (is org.tizenclaw.webview installed?): {result}")
                return f"Success: App generated and accessible at {url}, but failed to auto-launch on device: {result}"
                
            return f"Success: Web app '{title}' generated at {url} and successfully launched on the device screen."
        except Exception as e:
            logger.error(f"Generate Web App failed: {e}")
            return f"Error creating web app: {e}"
