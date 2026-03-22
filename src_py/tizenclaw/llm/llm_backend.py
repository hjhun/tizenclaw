from typing import List, Dict, Any, Callable, Optional
from dataclasses import dataclass, field
from abc import ABC, abstractmethod

@dataclass
class LlmToolCall:
    id: str
    name: str
    args: Dict[str, Any]

@dataclass
class LlmMessage:
    role: str  # "user", "assistant", "tool"
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

class LlmBackend(ABC):
    """
    Abstract base class for all LLM providers in TizenClaw.
    """
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

class LlmBackendFactory:
    @staticmethod
    def create(name: str) -> Optional[LlmBackend]:
        # TODO: Implement dynamic instantiation of OpenAI, Gemini, etc.
        return None
