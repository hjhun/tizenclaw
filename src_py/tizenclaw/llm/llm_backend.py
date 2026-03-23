"""
LLM Backend abstraction layer for TizenClaw Python port.

Provides:
  - Data classes: LlmMessage, LlmResponse, LlmToolCall, LlmToolDecl
  - Abstract base: LlmBackend
  - Factory: LlmBackendFactory (creates backends from llm_config.json)
  - Manager: LlmBackendManager (active + fallback switching)
"""
import os
import json
import logging
from typing import List, Dict, Any, Callable, Optional
from dataclasses import dataclass, field
from abc import ABC, abstractmethod

logger = logging.getLogger(__name__)

# ── Data Classes ──────────────────────────────────────────────────────────

@dataclass
class LlmToolCall:
    id: str
    name: str
    args: Dict[str, Any]

@dataclass
class LlmMessage:
    role: str  # "user", "assistant", "tool", "system"
    text: str = ""
    tool_calls: List[LlmToolCall] = field(default_factory=list)
    tool_name: str = ""
    tool_call_id: str = ""
    tool_result: Dict[str, Any] = field(default_factory=dict)

@dataclass
class LlmResponse:
    success: bool = False
    text: str = ""
    error_message: str = ""
    tool_calls: List[LlmToolCall] = field(default_factory=list)
    prompt_tokens: int = 0
    completion_tokens: int = 0
    total_tokens: int = 0
    http_status: int = 0

    def has_tool_calls(self) -> bool:
        return len(self.tool_calls) > 0

@dataclass
class LlmToolDecl:
    name: str
    description: str
    parameters: Dict[str, Any]  # JSON Schema

# ── Abstract Base ─────────────────────────────────────────────────────────

class LlmBackend(ABC):
    """Abstract base class for all LLM providers in TizenClaw."""

    @abstractmethod
    async def initialize(self, config: Dict[str, Any]) -> bool:
        pass

    @abstractmethod
    async def chat(
        self,
        messages: List[LlmMessage],
        tools: List[LlmToolDecl],
        on_chunk: Optional[Callable[[str], None]] = None,
        system_prompt: str = ""
    ) -> LlmResponse:
        pass

    @abstractmethod
    def get_name(self) -> str:
        pass

    def shutdown(self):
        pass

# ── Factory ───────────────────────────────────────────────────────────────

class LlmBackendFactory:
    """Creates LLM backend instances from name + config."""

    @staticmethod
    def create(name: str, backend_config: Dict[str, Any] = None) -> Optional[LlmBackend]:
        from tizenclaw.llm.openai_backend import OpenAiCompatibleBackend

        # OpenAI-compatible backends (openai, xai, ollama local via /v1)
        if name in ("openai", "xai", "ollama"):
            return OpenAiCompatibleBackend(backend_name=name)
        elif name == "gemini":
            from tizenclaw.llm.gemini_backend import GeminiBackend
            return GeminiBackend()
        elif name == "anthropic":
            from tizenclaw.llm.anthropic_backend import AnthropicBackend
            return AnthropicBackend()
        else:
            logger.warning(f"Unknown backend: {name}")
            return None

# ── Manager ───────────────────────────────────────────────────────────────

CONFIG_PATHS = [
    "/opt/usr/share/tizenclaw/config/llm_config.json",
    "/opt/usr/share/tizenclaw/data/devel/llm_config.json",
]

class LlmBackendManager:
    """
    Manages active + fallback LLM backends, inspired by C++ SwitchToBestBackend.
    Loads config from llm_config.json and creates backend instances.
    """

    def __init__(self):
        self.active: Optional[LlmBackend] = None
        self.fallbacks: List[LlmBackend] = []
        self.config: Dict[str, Any] = {}
        self.system_prompt: str = ""

    def load_config(self, config_path: str = "") -> Dict[str, Any]:
        """Load llm_config.json from standard paths."""
        paths = [config_path] + CONFIG_PATHS if config_path else CONFIG_PATHS
        for p in paths:
            if os.path.isfile(p):
                try:
                    with open(p, "r", encoding="utf-8") as f:
                        self.config = json.load(f)
                    logger.info(f"Loaded LLM config from {p}")
                    return self.config
                except Exception as e:
                    logger.error(f"Failed to load config {p}: {e}")

        logger.warning("No llm_config.json found. Using defaults.")
        self.config = {
            "active_backend": "openai",
            "fallback_backends": [],
            "backends": {
                "openai": {"model": "gpt-4o"}
            }
        }
        return self.config

    async def initialize(self, config_path: str = "") -> bool:
        """Load config, create backends, initialize active."""
        self.load_config(config_path)

        # Load system prompt from file if configured
        prompt_file = self.config.get("system_prompt_file", "")
        if prompt_file and os.path.isfile(prompt_file):
            try:
                with open(prompt_file, "r", encoding="utf-8") as f:
                    self.system_prompt = f.read().strip()
                logger.info(f"Loaded system prompt from {prompt_file}")
            except Exception as e:
                logger.error(f"Failed to load system prompt: {e}")

        backends_conf = self.config.get("backends", {})
        active_name = self.config.get("active_backend", "openai")
        fallback_names = self.config.get("fallback_backends", [])

        # Create and initialize active backend
        if active_name in backends_conf:
            backend = LlmBackendFactory.create(active_name, backends_conf[active_name])
            if backend:
                ok = await backend.initialize(backends_conf[active_name])
                if ok:
                    self.active = backend
                    logger.info(f"Active LLM backend: {backend.get_name()}")
                else:
                    logger.warning(f"Active backend '{active_name}' init returned False (API key missing?). Still usable.")
                    self.active = backend

        # Create fallback backends
        for fb_name in fallback_names:
            if fb_name in backends_conf and fb_name != active_name:
                backend = LlmBackendFactory.create(fb_name, backends_conf[fb_name])
                if backend:
                    await backend.initialize(backends_conf[fb_name])
                    self.fallbacks.append(backend)
                    logger.info(f"Fallback LLM backend: {backend.get_name()}")

        if not self.active:
            logger.error("No active LLM backend available!")
            return False

        return True

    async def chat(
        self,
        messages: List[LlmMessage],
        tools: List[LlmToolDecl],
        on_chunk: Optional[Callable[[str], None]] = None,
        system_prompt: str = ""
    ) -> LlmResponse:
        """
        Chat with active backend. On failure, try fallbacks sequentially.
        """
        prompt = system_prompt or self.system_prompt
        all_backends = ([self.active] if self.active else []) + self.fallbacks

        for backend in all_backends:
            try:
                response = await backend.chat(messages, tools, on_chunk, prompt)
                if response.success:
                    return response
                logger.warning(f"Backend {backend.get_name()} returned error: {response.error_message}")
            except Exception as e:
                logger.error(f"Backend {backend.get_name()} raised exception: {e}")

        return LlmResponse(
            success=False,
            error_message="All LLM backends failed"
        )

    def get_active_name(self) -> str:
        return self.active.get_name() if self.active else "none"

    def shutdown(self):
        if self.active:
            self.active.shutdown()
        for fb in self.fallbacks:
            fb.shutdown()
