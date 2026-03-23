"""
OpenAI-compatible LLM Backend for TizenClaw Python port.

Supports any OpenAI Chat Completions API compatible endpoint:
  - OpenAI (api.openai.com)
  - xAI / Grok (api.x.ai)
  - Ollama local (localhost:11434)
  - Any other /v1/chat/completions compatible server

SSL: Automatically discovers Tizen CA bundle paths to avoid
'unable to get local issuer certificate' errors.
"""
import os
import ssl
import json
import logging
import asyncio
import urllib.request
import urllib.error
from typing import List, Dict, Any, Optional, Callable, AsyncGenerator

from tizenclaw.llm.llm_backend import (
    LlmBackend, LlmMessage, LlmToolDecl, LlmResponse, LlmToolCall
)

logger = logging.getLogger(__name__)

# ── SSL Context Factory ───────────────────────────────────────────────────

# Tizen CA bundle search paths (same as libcurl's CA auto-discovery)
_CA_BUNDLE_PATHS = [
    "/etc/ssl/ca-bundle.pem",              # Tizen default
    "/etc/ssl/certs/ca-certificates.crt",  # Debian/Ubuntu
    "/etc/pki/tls/certs/ca-bundle.crt",    # RHEL/CentOS
    "/usr/share/ca-certificates",           # Alternative
]

def _create_ssl_context() -> ssl.SSLContext:
    """
    Create an SSL context with proper CA certificate discovery.
    On Tizen, the default Python ssl module often can't find the CA bundle
    because the compiled-in paths don't match Tizen's /etc/ssl/ca-bundle.pem.
    """
    ctx = ssl.create_default_context()

    # Try each known CA bundle path
    for ca_path in _CA_BUNDLE_PATHS:
        try:
            if os.path.isfile(ca_path):
                ctx.load_verify_locations(cafile=ca_path)
                logger.debug(f"SSL: Loaded CA bundle from {ca_path}")
                return ctx
            elif os.path.isdir(ca_path):
                ctx.load_verify_locations(capath=ca_path)
                logger.debug(f"SSL: Loaded CA directory from {ca_path}")
                return ctx
        except Exception:
            continue

    # Fallback: try system default (works on standard Linux)
    logger.debug("SSL: Using system default CA paths")
    return ctx


# ── OpenAI-Compatible Backend ─────────────────────────────────────────────

# Default endpoints per backend name
_DEFAULT_ENDPOINTS = {
    "openai": "https://api.openai.com/v1/chat/completions",
    "xai": "https://api.x.ai/v1/chat/completions",
    "ollama": "http://localhost:11434/v1/chat/completions",
}

_DEFAULT_MODELS = {
    "openai": "gpt-4o",
    "xai": "grok-3",
    "ollama": "llama3",
}


class OpenAiCompatibleBackend(LlmBackend):
    """
    Implements LlmBackend for any OpenAI Chat Completions API compatible endpoint.
    Zero external dependencies: uses asyncio.to_thread + urllib.request.
    """

    def __init__(self, backend_name: str = "openai"):
        self._backend_name = backend_name
        self.api_key: str = ""
        self.model: str = _DEFAULT_MODELS.get(backend_name, "gpt-4o")
        self.endpoint: str = _DEFAULT_ENDPOINTS.get(backend_name, _DEFAULT_ENDPOINTS["openai"])
        self._ssl_ctx: Optional[ssl.SSLContext] = None
        self._timeout: int = 60

    def get_name(self) -> str:
        return f"{self._backend_name}/{self.model}"

    async def initialize(self, config: Dict[str, Any] = None) -> bool:
        config = config or {}
        self.api_key = config.get("api_key", os.environ.get("OPENAI_API_KEY", ""))
        self.model = config.get("model", self.model)

        # Endpoint: config may specify base URL (e.g. "https://api.openai.com/v1")
        # or full endpoint. Normalize to /chat/completions.
        endpoint = config.get("endpoint", "")
        if endpoint:
            if not endpoint.endswith("/chat/completions"):
                endpoint = endpoint.rstrip("/") + "/chat/completions"
            self.endpoint = endpoint

        # Build SSL context once during init
        if self.endpoint.startswith("https://"):
            self._ssl_ctx = _create_ssl_context()

        if not self.api_key and self._backend_name not in ("ollama",):
            logger.warning(f"{self._backend_name}: API key missing. Requests will fail.")
            return False

        logger.info(f"Initialized {self.get_name()} → {self.endpoint}")
        return True

    def _make_http_request(self, payload: dict) -> dict:
        """Synchronous HTTP POST (called from asyncio.to_thread)."""
        req = urllib.request.Request(self.endpoint, method="POST")
        if self.api_key:
            req.add_header("Authorization", f"Bearer {self.api_key}")
        req.add_header("Content-Type", "application/json")
        data = json.dumps(payload).encode("utf-8")

        try:
            # Use our custom SSL context for HTTPS
            ctx = self._ssl_ctx if self.endpoint.startswith("https://") else None
            with urllib.request.urlopen(req, data=data, timeout=self._timeout, context=ctx) as r:
                body = r.read().decode("utf-8")
                return json.loads(body)
        except urllib.error.HTTPError as e:
            body = ""
            try:
                body = e.read().decode("utf-8", errors="replace")
            except Exception:
                pass
            logger.error(f"{self._backend_name} HTTP {e.code}: {body[:200]}")
            return {"error": f"HTTP {e.code}: {body[:200]}", "http_status": e.code}
        except urllib.error.URLError as e:
            logger.error(f"{self._backend_name} request failed: {e.reason}")
            return {"error": str(e.reason)}
        except Exception as e:
            logger.error(f"{self._backend_name} unexpected error: {e}")
            return {"error": str(e)}

    @staticmethod
    def _convert_messages(messages: List[LlmMessage]) -> list:
        """Convert LlmMessage list to OpenAI API format."""
        result = []
        for m in messages:
            msg: Dict[str, Any] = {"role": m.role, "content": m.text or ""}

            # Assistant messages with tool_calls
            if m.tool_calls and m.role == "assistant":
                msg["tool_calls"] = []
                for tc in m.tool_calls:
                    msg["tool_calls"].append({
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.name,
                            "arguments": json.dumps(tc.args) if isinstance(tc.args, dict) else str(tc.args)
                        }
                    })

            # Tool result messages
            if m.role == "tool" and m.tool_call_id:
                msg["tool_call_id"] = m.tool_call_id

            result.append(msg)
        return result

    @staticmethod
    def _convert_tools(tools: List[LlmToolDecl]) -> list:
        """Convert LlmToolDecl list to OpenAI function calling format."""
        if not tools:
            return []
        return [
            {
                "type": "function",
                "function": {
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.parameters
                }
            }
            for t in tools
        ]

    async def chat(
        self,
        messages: List[LlmMessage],
        tools: List[LlmToolDecl],
        on_chunk: Optional[Callable[[str], None]] = None,
        system_prompt: str = ""
    ) -> LlmResponse:
        """Send chat completion request to OpenAI-compatible API."""
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

        logger.info(
            f"LLM API call: {self._backend_name}/{self.model}, "
            f"messages={len(api_messages)}, tools={len(converted_tools)}"
        )

        # Offload synchronous urllib to thread pool
        result = await asyncio.to_thread(self._make_http_request, payload)

        if "error" in result:
            return LlmResponse(
                success=False,
                error_message=result["error"],
                http_status=result.get("http_status", 0)
            )

        # Parse response
        choice = result.get("choices", [{}])[0].get("message", {})
        text = choice.get("content", "") or ""
        usage = result.get("usage", {})

        # Parse tool calls
        tcalls = []
        for tc in choice.get("tool_calls", []):
            func = tc.get("function", {})
            name = func.get("name", "")
            args_str = func.get("arguments", "{}")
            try:
                args = json.loads(args_str)
            except (json.JSONDecodeError, TypeError):
                args = {"arguments": args_str}
            tcalls.append(LlmToolCall(id=tc.get("id", ""), name=name, args=args))

        logger.info(
            f"LLM API response: text={len(text)} chars, "
            f"tool_calls={len(tcalls)}, tokens={usage.get('total_tokens', 0)}"
        )

        return LlmResponse(
            success=True,
            text=text,
            tool_calls=tcalls,
            prompt_tokens=usage.get("prompt_tokens", 0),
            completion_tokens=usage.get("completion_tokens", 0),
            total_tokens=usage.get("total_tokens", 0)
        )

    async def generate_stream(
        self, prompt: str, history: List[LlmMessage], tools: List[LlmToolDecl]
    ) -> AsyncGenerator[str, None]:
        """Stub streaming: wraps single response."""
        messages = list(history)
        if prompt:
            messages.append(LlmMessage(role="user", text=prompt))
        response = await self.chat(messages, tools)
        yield response.text
