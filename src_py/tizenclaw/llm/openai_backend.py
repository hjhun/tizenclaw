import os
import json
import logging
import asyncio
import urllib.request
import urllib.error
from typing import List, Dict, Any, Optional, Callable, AsyncGenerator
from tizenclaw.llm.llm_backend import LlmBackend, LlmMessage, LlmToolDecl, LlmResponse, LlmToolCall

logger = logging.getLogger(__name__)

class OpenAiBackend(LlmBackend):
    """
    Implements LlmBackend for OpenAI's Chat Completions REST API.
    Aims for zero external dependencies using asyncio.to_thread and urllib.
    """
    def __init__(self, api_key: str = None, model: str = "gpt-4o"):
        self.api_key = api_key or os.environ.get("OPENAI_API_KEY", "")
        self.model = model
        self.endpoint = "https://api.openai.com/v1/chat/completions"

    def get_name(self) -> str:
        return f"openai/{self.model}"

    async def initialize(self, config: Dict[str, Any] = None) -> bool:
        if config:
            self.api_key = config.get("api_key", self.api_key)
            self.model = config.get("model", self.model)
            self.endpoint = config.get("endpoint", self.endpoint)
        if not self.api_key:
            logger.warning("OpenAI API key missing. LlmBackend may fail.")
            return False
        return True

    def _make_http_request(self, payload: dict) -> dict:
        req = urllib.request.Request(self.endpoint, method="POST")
        req.add_header("Authorization", f"Bearer {self.api_key}")
        req.add_header("Content-Type", "application/json")
        data = json.dumps(payload).encode('utf-8')
        try:
            with urllib.request.urlopen(req, data=data, timeout=30) as r:
                return json.loads(r.read().decode('utf-8'))
        except urllib.error.URLError as e:
            logger.error(f"OpenAI API Request failed: {e}")
            return {"error": str(e)}

    def _convert_messages(self, messages: List[LlmMessage]) -> list:
        result = []
        for m in messages:
            msg: Dict[str, Any] = {"role": m.role, "content": m.text or ""}
            if m.tool_calls:
                msg["tool_calls"] = []
                for tc in m.tool_calls:
                    msg["tool_calls"].append({
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.name,
                            "arguments": json.dumps(tc.args)
                        }
                    })
            if m.role == "tool" and m.tool_call_id:
                msg["tool_call_id"] = m.tool_call_id
            result.append(msg)
        return result

    def _convert_tools(self, tools: List[LlmToolDecl]) -> list:
        if not tools:
            return []
        openai_tools = []
        for t in tools:
            openai_tools.append({
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters
                }
            })
        return openai_tools

    async def chat(
        self,
        messages: List[LlmMessage],
        tools: List[LlmToolDecl],
        on_chunk: Optional[Callable[[str], None]] = None,
        system_prompt: str = ""
    ) -> LlmResponse:
        """Implements the abstract chat() method from LlmBackend."""
        api_messages = []
        if system_prompt:
            api_messages.append({"role": "system", "content": system_prompt})
        api_messages.extend(self._convert_messages(messages))

        payload: Dict[str, Any] = {
            "model": self.model,
            "messages": api_messages,
        }
        converted_tools = self._convert_tools(tools)
        if converted_tools:
            payload["tools"] = converted_tools
            payload["tool_choice"] = "auto"

        # Offload sync urllib networking to thread pool
        result = await asyncio.to_thread(self._make_http_request, payload)

        if "error" in result:
            logger.error(f"Chat failed: {result['error']}")
            return LlmResponse(success=False, text="", error_message=str(result['error']))

        choice = result.get("choices", [{}])[0].get("message", {})
        text = choice.get("content", "") or ""
        usage = result.get("usage", {})

        tcalls = []
        if "tool_calls" in choice:
            for tc in choice["tool_calls"]:
                name = tc.get("function", {}).get("name", "")
                args_str = tc.get("function", {}).get("arguments", "{}")
                try:
                    args = json.loads(args_str)
                except json.JSONDecodeError:
                    args = {}
                tcalls.append(LlmToolCall(id=tc.get("id", ""), name=name, args=args))

        return LlmResponse(
            success=True,
            text=text,
            tool_calls=tcalls,
            prompt_tokens=usage.get("prompt_tokens", 0),
            completion_tokens=usage.get("completion_tokens", 0),
            total_tokens=usage.get("total_tokens", 0)
        )

    async def generate_response(self, prompt: str, history: List[LlmMessage], tools: List[LlmToolDecl]) -> LlmResponse:
        """Convenience wrapper: appends user prompt to history and calls chat()."""
        messages = list(history)
        if prompt:
            messages.append(LlmMessage(role="user", text=prompt))
        return await self.chat(messages, tools)

    async def generate_stream(self, prompt: str, history: List[LlmMessage], tools: List[LlmToolDecl]) -> AsyncGenerator[str, None]:
        # Wrap single response for zero-dependency streaming stub
        response = await self.generate_response(prompt, history, tools)
        yield response.text
