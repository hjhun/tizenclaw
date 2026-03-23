"""
Google Gemini LLM Backend for TizenClaw.

Uses the Gemini REST API (generativelanguage.googleapis.com).
Converts between TizenClaw's unified LlmMessage format and Gemini's native format.
"""
import asyncio
import json
import logging
import os
import ssl
import urllib.request
from typing import Dict, List, Any, Optional, Callable

from .llm_backend import (
    LlmBackend, LlmMessage, LlmToolDecl, LlmToolCall, LlmResponse
)

logger = logging.getLogger(__name__)


class GeminiBackend(LlmBackend):
    """Google Gemini API backend."""

    def __init__(self):
        self._api_key = ""
        self._model = "gemini-2.0-flash"
        self._endpoint = "https://generativelanguage.googleapis.com/v1beta"
        self._ssl_ctx: Optional[ssl.SSLContext] = None
        self._backend_name = "gemini"

    def get_name(self) -> str:
        return f"gemini/{self._model}"

    async def initialize(self, config: Dict[str, Any]) -> bool:
        self._api_key = config.get("api_key", os.environ.get("GEMINI_API_KEY", ""))
        self._model = config.get("model", self._model)
        self._endpoint = config.get("endpoint", self._endpoint).rstrip("/")

        if not self._api_key:
            logger.error("Gemini: no API key provided")
            return False

        # SSL context
        self._ssl_ctx = ssl.create_default_context()
        for ca in ["/etc/ssl/ca-bundle.pem", "/etc/ssl/certs/ca-certificates.crt",
                    "/usr/share/ca-certificates"]:
            if os.path.exists(ca):
                try:
                    if os.path.isfile(ca):
                        self._ssl_ctx.load_verify_locations(ca)
                    else:
                        self._ssl_ctx.load_verify_locations(capath=ca)
                    break
                except Exception:
                    pass

        logger.info(f"Initialized {self.get_name()} → {self._endpoint}")
        return True

    def _convert_messages(self, messages: List[LlmMessage]) -> List[Dict]:
        """Convert LlmMessage list to Gemini 'contents' format."""
        contents = []
        for m in messages:
            if m.role == "system":
                continue  # System prompt handled separately
            parts = []

            # Text content
            if m.content:
                parts.append({"text": m.content})

            # Tool call results
            if m.role == "tool" and m.tool_call_id:
                parts.append({
                    "functionResponse": {
                        "name": m.tool_call_id,
                        "response": {"result": m.content}
                    }
                })

            # Tool calls from assistant
            if m.tool_calls:
                for tc in m.tool_calls:
                    parts.append({
                        "functionCall": {
                            "name": tc.name,
                            "args": tc.args if isinstance(tc.args, dict) else {}
                        }
                    })

            role = "model" if m.role == "assistant" else "user"
            if parts:
                contents.append({"role": role, "parts": parts})

        return contents

    def _convert_tools(self, tools: List[LlmToolDecl]) -> List[Dict]:
        """Convert LlmToolDecl to Gemini function declarations."""
        if not tools:
            return []
        declarations = []
        for t in tools:
            decl = {
                "name": t.name,
                "description": t.description or "",
            }
            if t.parameters:
                decl["parameters"] = t.parameters
            else:
                decl["parameters"] = {
                    "type": "object",
                    "properties": {
                        "arguments": {
                            "type": "string",
                            "description": "Arguments for the tool"
                        }
                    }
                }
            declarations.append(decl)
        return [{"functionDeclarations": declarations}]

    def _make_request(self, payload: Dict) -> Dict:
        """Make HTTP request to Gemini API."""
        url = (f"{self._endpoint}/models/{self._model}:generateContent"
               f"?key={self._api_key}")

        body = json.dumps(payload).encode("utf-8")
        req = urllib.request.Request(
            url, data=body, method="POST",
            headers={"Content-Type": "application/json"}
        )

        try:
            resp = urllib.request.urlopen(req, context=self._ssl_ctx, timeout=120)
            return json.loads(resp.read().decode("utf-8"))
        except urllib.error.HTTPError as e:
            error_body = e.read().decode("utf-8", errors="replace")
            logger.error(f"Gemini API error {e.code}: {error_body[:200]}")
            return {"error": f"HTTP {e.code}: {error_body[:200]}", "http_status": e.code}
        except Exception as e:
            logger.error(f"Gemini request failed: {e}")
            return {"error": str(e)}

    async def chat(
        self,
        messages: List[LlmMessage],
        tools: List[LlmToolDecl],
        on_chunk: Optional[Callable[[str], None]] = None,
        system_prompt: str = ""
    ) -> LlmResponse:
        contents = self._convert_messages(messages)

        payload: Dict[str, Any] = {"contents": contents}

        if system_prompt:
            payload["systemInstruction"] = {
                "parts": [{"text": system_prompt}]
            }

        tool_decls = self._convert_tools(tools)
        if tool_decls:
            payload["tools"] = tool_decls

        # Generation config
        payload["generationConfig"] = {
            "temperature": 0.7,
            "maxOutputTokens": 4096,
        }

        logger.info(f"Gemini API call: {self._model}, messages={len(contents)}, tools={len(tools)}")

        result = await asyncio.to_thread(self._make_request, payload)

        if "error" in result:
            return LlmResponse(
                success=False,
                error_message=result["error"],
                http_status=result.get("http_status", 0)
            )

        # Parse response
        candidates = result.get("candidates", [])
        if not candidates:
            return LlmResponse(success=False, error_message="No candidates in response")

        content = candidates[0].get("content", {})
        parts = content.get("parts", [])

        text = ""
        tcalls = []

        for part in parts:
            if "text" in part:
                text += part["text"]
            if "functionCall" in part:
                fc = part["functionCall"]
                tcalls.append(LlmToolCall(
                    id=fc.get("name", ""),
                    name=fc.get("name", ""),
                    args=fc.get("args", {})
                ))

        # Token usage
        usage = result.get("usageMetadata", {})
        logger.info(f"Gemini response: text={len(text)} chars, tool_calls={len(tcalls)}")

        return LlmResponse(
            success=True,
            text=text,
            tool_calls=tcalls,
            prompt_tokens=usage.get("promptTokenCount", 0),
            completion_tokens=usage.get("candidatesTokenCount", 0),
            total_tokens=usage.get("totalTokenCount", 0)
        )

    async def generate_stream(self, prompt, history, tools):
        resp = await self.chat(
            history + [LlmMessage(role="user", content=prompt)],
            tools
        )
        return resp
