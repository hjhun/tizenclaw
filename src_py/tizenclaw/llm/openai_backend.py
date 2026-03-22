import os
import json
import logging
import asyncio
import urllib.request
import urllib.error
from typing import List, AsyncGenerator
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

    async def initialize(self) -> bool:
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
        return [{"role": m.role.value, "content": m.text} for m in messages]

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
                    "parameters": t.parameters_schema
                }
            })
        return openai_tools

    async def generate_response(self, prompt: str, history: List[LlmMessage], tools: List[LlmToolDecl]) -> LlmResponse:
        messages = self._convert_messages(history)
        messages.append({"role": "user", "content": prompt})
        
        payload = {
            "model": self.model,
            "messages": messages,
        }
        converted_tools = self._convert_tools(tools)
        if converted_tools:
            payload["tools"] = converted_tools
            payload["tool_choice"] = "auto"

        # Offload sync urllib networking to thread pool
        result = await asyncio.to_thread(self._make_http_request, payload)
        
        if "error" in result:
            logger.error(f"Generate response failed: {result['error']}")
            return LlmResponse(text=f"Error: {result['error']}", tool_calls=[])

        choice = result.get("choices", [{}])[0].get("message", {})
        text = choice.get("content", "") or ""
        
        tcalls = []
        if "tool_calls" in choice:
            for tc in choice["tool_calls"]:
                name = tc.get("function", {}).get("name")
                args_str = tc.get("function", {}).get("arguments", "{}")
                try:
                    args = json.loads(args_str)
                except json.JSONDecodeError:
                    args = {}
                tcalls.append(LlmToolCall(id=tc.get("id"), name=name, arguments=args))

        return LlmResponse(text=text, tool_calls=tcalls)

    async def generate_stream(self, prompt: str, history: List[LlmMessage], tools: List[LlmToolDecl]) -> AsyncGenerator[str, None]:
        # Implementation of streaming with urllib is complex without dependencies
        # So we wrap the single response for simplicity in zero-dependency python
        response = await self.generate_response(prompt, history, tools)
        yield response.text
