"""
Anthropic (Claude) LLM Backend for TizenClaw.

Uses the Anthropic Messages API (api.anthropic.com).
Handles Anthropic's unique header requirements and message format.
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


class AnthropicBackend(LlmBackend):
    """Anthropic Claude API backend."""

    def __init__(self):
        self._api_key = ""
        self._model = "claude-sonnet-4-20250514"
        self._endpoint = "https://api.anthropic.com/v1/messages"
        self._ssl_ctx: Optional[ssl.SSLContext] = None
        self._backend_name = "anthropic"
        self._api_version = "2023-06-01"

    def get_name(self) -> str:
        return f"anthropic/{self._model}"

    async def initialize(self, config: Dict[str, Any]) -> bool:
        self._api_key = config.get("api_key", os.environ.get("ANTHROPIC_API_KEY", ""))
        self._model = config.get("model", self._model)
        ep = config.get("endpoint", "")
        if ep:
            self._endpoint = ep.rstrip("/")

        if not self._api_key:
            logger.error("Anthropic: no API key provided")
            return False

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
        """Convert to Anthropic message format."""
        result = []
        for m in messages:
            if m.role == "system":
                continue  # System prompt handled separately

            if m.role == "assistant":
                content = []
                if m.content:
                    content.append({"type": "text", "text": m.content})
                if m.tool_calls:
                    for tc in m.tool_calls:
                        content.append({
                            "type": "tool_use",
                            "id": tc.id or tc.name,
                            "name": tc.name,
                            "input": tc.args if isinstance(tc.args, dict) else {}
                        })
                result.append({"role": "assistant", "content": content})

            elif m.role == "tool":
                result.append({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": m.tool_call_id or "",
                        "content": m.content or ""
                    }]
                })

            else:  # user
                result.append({"role": "user", "content": m.content or ""})

        return result

    def _convert_tools(self, tools: List[LlmToolDecl]) -> List[Dict]:
        """Convert to Anthropic tool format."""
        result = []
        for t in tools:
            schema = t.parameters or {
                "type": "object",
                "properties": {
                    "arguments": {
                        "type": "string",
                        "description": "Arguments for the tool"
                    }
                }
            }
            result.append({
                "name": t.name,
                "description": t.description or "",
                "input_schema": schema
            })
        return result

    def _make_request(self, payload: Dict) -> Dict:
        body = json.dumps(payload).encode("utf-8")
        req = urllib.request.Request(
            self._endpoint, data=body, method="POST",
            headers={
                "Content-Type": "application/json",
                "x-api-key": self._api_key,
                "anthropic-version": self._api_version,
            }
        )

        try:
            resp = urllib.request.urlopen(req, context=self._ssl_ctx, timeout=120)
            return json.loads(resp.read().decode("utf-8"))
        except urllib.error.HTTPError as e:
            error_body = e.read().decode("utf-8", errors="replace")
            logger.error(f"Anthropic API error {e.code}: {error_body[:200]}")
            return {"error": f"HTTP {e.code}: {error_body[:200]}", "http_status": e.code}
        except Exception as e:
            logger.error(f"Anthropic request failed: {e}")
            return {"error": str(e)}

    async def chat(
        self,
        messages: List[LlmMessage],
        tools: List[LlmToolDecl],
        on_chunk: Optional[Callable[[str], None]] = None,
        system_prompt: str = ""
    ) -> LlmResponse:
        api_messages = self._convert_messages(messages)

        payload: Dict[str, Any] = {
            "model": self._model,
            "messages": api_messages,
            "max_tokens": 4096,
        }

        if system_prompt:
            payload["system"] = system_prompt

        converted_tools = self._convert_tools(tools)
        if converted_tools:
            payload["tools"] = converted_tools

        logger.info(f"Anthropic API call: {self._model}, messages={len(api_messages)}, tools={len(converted_tools)}")

        result = await asyncio.to_thread(self._make_request, payload)

        if "error" in result:
            err_msg = result["error"]
            if isinstance(err_msg, dict):
                err_msg = err_msg.get("message", str(err_msg))
            return LlmResponse(
                success=False,
                error_message=str(err_msg),
                http_status=result.get("http_status", 0)
            )

        # Parse response
        content_blocks = result.get("content", [])
        text = ""
        tcalls = []

        for block in content_blocks:
            if block.get("type") == "text":
                text += block.get("text", "")
            elif block.get("type") == "tool_use":
                tcalls.append(LlmToolCall(
                    id=block.get("id", ""),
                    name=block.get("name", ""),
                    args=block.get("input", {})
                ))

        usage = result.get("usage", {})
        logger.info(f"Anthropic response: text={len(text)} chars, tool_calls={len(tcalls)}, tokens={usage.get('output_tokens', 0)}")

        return LlmResponse(
            success=True,
            text=text,
            tool_calls=tcalls,
            prompt_tokens=usage.get("input_tokens", 0),
            completion_tokens=usage.get("output_tokens", 0),
            total_tokens=usage.get("input_tokens", 0) + usage.get("output_tokens", 0)
        )

    async def generate_stream(self, prompt, history, tools):
        resp = await self.chat(
            history + [LlmMessage(role="user", content=prompt)],
            tools
        )
        return resp
