import asyncio
import logging
import json
from typing import Dict, List, Optional, Callable, Any

logger = logging.getLogger(__name__)

class AgentCore:
    """
    Python implementation of TizenClaw AgentCore.
    Handles task scheduling, execution, and LLM orchestration.
    """
    def __init__(self):
        self._running = False
        self._initialized = False
        self.system_prompt: str = (
            "You are TizenClaw, an AI agent running on a Tizen device. "
            "You can control the device using the available tools. "
            "When the user asks about device information, use the appropriate tool."
        )
        self.web_api_catalog: str = ""
        
        # Session state management
        self.sessions: Dict[str, List] = {}
        self.session_prompts: Dict[str, str] = {}
        self.session_lock = asyncio.Lock()
        
        # Dispatch maps equivalent to std::unordered_map
        self.tool_dispatch: Dict[str, Callable] = {}
        
        # Additional Component References (Placeholders)
        self.container_engine = None
        self.backend = None
        self.backend_lock = asyncio.Lock()
        self.scheduler = None

    async def initialize(self) -> bool:
        """Initialize backend, routing, and system contexts."""
        from tizenclaw.core.tool_indexer import ToolIndexer
        from tizenclaw.core.tool_dispatcher import ToolDispatcher
        from tizenclaw.infra.container_engine import ContainerEngine
        from tizenclaw.llm.openai_backend import OpenAiBackend
        from tizenclaw.llm.llm_backend import LlmMessage

        logger.info("Initializing AgentCore Python port...")
        
        import os
        os.makedirs("/opt/usr/share/tizenclaw/work/sessions", exist_ok=True)
        
        self.indexer = ToolIndexer()
        self.indexer.load_all_tools()
        
        self.container_engine = ContainerEngine()
        self.dispatcher = ToolDispatcher(self.indexer, self.container_engine)
        self.backend = OpenAiBackend()
        
        await self.backend.initialize()

        self._initialized = True
        logger.info("AgentCore initialized successfully.")
        return True

    def shutdown(self):
        """Clean shutdown of all async resources and backends."""
        self._running = False
        logger.info("AgentCore shutdown initiated.")

    async def process_prompt(self, session_id: str, prompt: str, on_chunk: Optional[Callable[[str], None]] = None) -> str:
        """
        Process a prompt through the LLM pipeline, looping over tool calls automatically.
        """
        from tizenclaw.llm.llm_backend import LlmMessage, LlmToolDecl

        async with self.session_lock:
            if session_id not in self.sessions:
                self.sessions[session_id] = []
            
            self.sessions[session_id].append(LlmMessage(role="user", text=prompt))

        schemas_raw = self.indexer.get_tool_schemas()
        tools = [
            LlmToolDecl(name=s["name"], description=s["description"], parameters=s.get("parameters", {}))
            for s in schemas_raw
        ]

        # AutoSkillAgent Intercept (Direct tool execution without LLM overhead)
        lower_prompt = prompt.lower()
        if "get_device_info" in lower_prompt or "device info" in lower_prompt:
            logger.info("AutoSkill intercept: get_device_info")
            tool_output = await self.dispatcher.execute_tool("get_device_info", {})
            return f"{tool_output}"

        # LLM execution loop (resolving tool calls)
        final_text = ""
        for i in range(10): # Max 10 tool iterations
            async with self.session_lock:
                current_history = list(self.sessions[session_id])
            
            # Send latest context to LLM
            response = await self.backend.chat(
                messages=current_history,
                tools=tools,
                on_chunk=on_chunk,
                system_prompt=self.system_prompt
            )

            if not response.success:
                error_msg = response.error_message or "LLM request failed"
                logger.error(f"LLM error: {error_msg}")
                return f"Error: {error_msg}"
            
            # Process returned text
            if response.text:
                final_text += response.text + "\n"
                if on_chunk:
                    on_chunk(response.text)
                
            # If no tools called, we're done
            if not response.has_tool_calls():
                async with self.session_lock:
                    self.sessions[session_id].append(LlmMessage(role="assistant", text=response.text))
                break

            # Handle tools sequentially
            async with self.session_lock:
                self.sessions[session_id].append(LlmMessage(role="assistant", text=response.text, tool_calls=response.tool_calls))
                
            for tc in response.tool_calls:
                logger.info(f"LLM tool call: {tc.name}({tc.args})")
                tool_output = await self.dispatcher.execute_tool(tc.name, tc.args)
                async with self.session_lock:
                    self.sessions[session_id].append(LlmMessage(role="tool", text=tool_output, tool_call_id=tc.id))

        return final_text.strip()

    def clear_session(self, session_id: str):
        """Clear session from memory (and eventually storage)."""
        if session_id in self.sessions:
            del self.sessions[session_id]

    async def execute_skill(self, skill_name: str, args: Dict[str, Any]) -> str:
        """Execute a skill explicitly inside the secure python container engine."""
        logger.debug(f"Executing skill: {skill_name} with args {args}")
        return json.dumps({"status": "success", "output": "mock_skill_execution"})

    async def start(self):
        self._running = True
        logger.info("AgentCore main loop started.")
        while self._running:
            await asyncio.sleep(1.0)
            
    def stop(self):
        self.shutdown()
