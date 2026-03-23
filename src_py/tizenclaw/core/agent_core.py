"""
AgentCore — Central orchestration engine for TizenClaw Python port.

Manages:
  - LLM backend (via LlmBackendManager with active + fallback)
  - Tool indexing and dispatch (ToolIndexer + ToolDispatcher)
  - Session state (per-session message history with asyncio.Lock)
  - Agentic loop (iterative tool calling, max 10 rounds)
  - Auto-skill intercept (direct execution for known queries)
"""
import asyncio
import logging
import json
import os
from typing import Dict, List, Optional, Callable, Any

logger = logging.getLogger(__name__)

# Data directory
DATA_DIR = "/opt/usr/share/tizenclaw"
CONFIG_DIR = os.path.join(DATA_DIR, "config")
WORK_DIR = os.path.join(DATA_DIR, "work")

# Default system prompt
DEFAULT_SYSTEM_PROMPT = (
    "You are TizenClaw, an AI agent running on a Tizen embedded device. "
    "You can control the device using the available tools. "
    "When the user asks about device information, use the appropriate tool. "
    "Always respond in the same language as the user's message."
)


class AgentCore:
    """Python implementation of TizenClaw AgentCore."""

    def __init__(self):
        self._running = False
        self._initialized = False
        self.system_prompt: str = DEFAULT_SYSTEM_PROMPT

        # Session state management
        self.sessions: Dict[str, List] = {}
        self.session_prompts: Dict[str, str] = {}
        self.session_lock = asyncio.Lock()

        # Component references (set during initialize)
        self.indexer = None
        self.dispatcher = None
        self.backend_manager = None
        self.scheduler = None
        self.container_engine = None

    async def initialize(self) -> bool:
        """Initialize all subsystems: config, backend, tools, container."""
        from tizenclaw.core.tool_indexer import ToolIndexer
        from tizenclaw.core.tool_dispatcher import ToolDispatcher
        from tizenclaw.infra.container_engine import ContainerEngine
        from tizenclaw.llm.llm_backend import LlmBackendManager

        logger.info("Initializing AgentCore...")

        # Ensure work directories exist
        os.makedirs(os.path.join(WORK_DIR, "sessions"), exist_ok=True)
        os.makedirs(CONFIG_DIR, exist_ok=True)

        # Initialize LLM backend manager (loads llm_config.json)
        self.backend_manager = LlmBackendManager()
        await self.backend_manager.initialize()

        # Use system prompt from config if loaded
        if self.backend_manager.system_prompt:
            self.system_prompt = self.backend_manager.system_prompt

        # Initialize tool indexer
        self.indexer = ToolIndexer()
        self.indexer.load_all_tools()

        # Initialize container engine and dispatcher
        self.container_engine = ContainerEngine()
        self.dispatcher = ToolDispatcher(self.indexer, self.container_engine)

        self._initialized = True
        logger.info(
            f"AgentCore initialized. Active LLM: {self.backend_manager.get_active_name()}, "
            f"Tools: {len(self.indexer.tools)}"
        )
        return True

    def shutdown(self):
        """Clean shutdown of all resources."""
        self._running = False
        if self.backend_manager:
            self.backend_manager.shutdown()
        logger.info("AgentCore shutdown.")

    async def process_prompt(
        self,
        session_id: str,
        prompt: str,
        on_chunk: Optional[Callable[[str], None]] = None
    ) -> str:
        """
        Process a user prompt through the agentic loop:
          1. Auto-skill intercept (bypass LLM for known queries)
          2. Send to LLM with tool schemas
          3. Execute tool calls, feed results back
          4. Repeat until no more tool calls (max 10 iterations)
        """
        from tizenclaw.llm.llm_backend import LlmMessage, LlmToolDecl

        # Add user message to session history
        async with self.session_lock:
            if session_id not in self.sessions:
                self.sessions[session_id] = []
            self.sessions[session_id].append(LlmMessage(role="user", text=prompt))

        # Build tool schemas for LLM
        schemas_raw = self.indexer.get_tool_schemas()
        tools = [
            LlmToolDecl(
                name=s["name"],
                description=s["description"],
                parameters=s.get("parameters", {})
            )
            for s in schemas_raw
        ]

        # ── Auto-skill intercept ──
        # For known device info queries, execute tool directly without LLM overhead
        lower_prompt = prompt.lower().strip()
        auto_skill = self._match_auto_skill(lower_prompt)
        if auto_skill:
            logger.info(f"AutoSkill intercept: {auto_skill}")
            try:
                tool_output = await self.dispatcher.execute_tool(auto_skill, {})
                async with self.session_lock:
                    self.sessions[session_id].append(
                        LlmMessage(role="assistant", text=tool_output)
                    )
                return tool_output
            except Exception as e:
                logger.error(f"AutoSkill {auto_skill} failed: {e}")
                # Fall through to LLM

        # ── Agentic loop ──
        final_text = ""
        for iteration in range(10):
            async with self.session_lock:
                current_history = list(self.sessions[session_id])

            # Send to LLM via backend manager (auto-fallback)
            response = await self.backend_manager.chat(
                messages=current_history,
                tools=tools,
                on_chunk=on_chunk,
                system_prompt=self.system_prompt
            )

            if not response.success:
                error_msg = response.error_message or "LLM request failed"
                logger.error(f"LLM error (iteration {iteration}): {error_msg}")
                return f"Error: {error_msg}"

            # Accumulate response text
            if response.text:
                final_text += response.text + "\n"
                if on_chunk:
                    on_chunk(response.text)

            # If no tool calls, we're done
            if not response.has_tool_calls():
                async with self.session_lock:
                    self.sessions[session_id].append(
                        LlmMessage(role="assistant", text=response.text)
                    )
                break

            # Record assistant message with tool calls
            async with self.session_lock:
                self.sessions[session_id].append(
                    LlmMessage(
                        role="assistant",
                        text=response.text or "",
                        tool_calls=response.tool_calls
                    )
                )

            # Execute each tool call sequentially
            for tc in response.tool_calls:
                logger.info(f"Tool call [{iteration}]: {tc.name}({tc.args})")
                try:
                    tool_output = await self.dispatcher.execute_tool(tc.name, tc.args)
                except Exception as e:
                    tool_output = f"Tool execution error: {e}"
                    logger.error(f"Tool {tc.name} failed: {e}")

                async with self.session_lock:
                    self.sessions[session_id].append(
                        LlmMessage(role="tool", text=tool_output, tool_call_id=tc.id)
                    )

        return final_text.strip()

    @staticmethod
    def _match_auto_skill(prompt: str) -> Optional[str]:
        """Match known prompts to tool names for direct execution."""
        auto_map = {
            "get_device_info": ["get_device_info", "device info", "디바이스 정보"],
            "get_battery_info": ["get_battery_info", "battery info", "배터리 정보", "배터리"],
            "get_wifi_info": ["get_wifi_info", "wifi info", "와이파이 정보"],
            "get_system_info": ["get_system_info", "system info", "시스템 정보"],
        }
        for tool_name, keywords in auto_map.items():
            for kw in keywords:
                if kw in prompt:
                    return tool_name
        return None

    def clear_session(self, session_id: str):
        """Clear session history."""
        if session_id in self.sessions:
            del self.sessions[session_id]

    async def start(self):
        self._running = True
        logger.info("AgentCore main loop started.")
        while self._running:
            await asyncio.sleep(1.0)

    def stop(self):
        self.shutdown()
