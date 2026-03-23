"""
TizenClaw Webhook Channel — sends notifications via HTTP POST webhooks.

Supports configurable webhook URLs for different event types.
Used by AutonomousTrigger, PerceptionEngine, and other subsystems
to push alerts to external services (Slack Incoming Webhooks,
Discord Webhooks, IFTTT, custom endpoints).
"""
import asyncio
import json
import logging
import os
import ssl
import time
import urllib.request
from typing import Dict, Any, Optional, List

logger = logging.getLogger(__name__)

WEBHOOK_CONFIG_PATH = "/opt/usr/share/tizenclaw/config/webhook_config.json"


class WebhookChannel:
    """HTTP POST webhook notification channel."""

    def __init__(self):
        self._webhooks: List[Dict[str, Any]] = []
        self._enabled = False
        self._ssl_ctx: Optional[ssl.SSLContext] = None
        self._retry_count = 2
        self._timeout = 10

        try:
            self._ssl_ctx = ssl.create_default_context()
        except Exception:
            self._ssl_ctx = ssl._create_unverified_context()

    def load_config(self, path: str = WEBHOOK_CONFIG_PATH) -> bool:
        if not os.path.isfile(path):
            logger.info("WebhookChannel: Config not found, disabled")
            return False
        try:
            with open(path, "r", encoding="utf-8") as f:
                cfg = json.load(f)
            self._enabled = cfg.get("enabled", False)
            self._webhooks = cfg.get("webhooks", [])
            self._retry_count = cfg.get("retry_count", 2)
            self._timeout = cfg.get("timeout_seconds", 10)
            logger.info(f"WebhookChannel: Loaded {len(self._webhooks)} webhooks, "
                        f"enabled={self._enabled}")
            return True
        except Exception as e:
            logger.error(f"WebhookChannel: Config error: {e}")
            return False

    def is_enabled(self) -> bool:
        return self._enabled

    async def send(self, event_type: str, message: str,
                   data: Dict[str, Any] = None) -> int:
        """Send notification to all matching webhooks. Returns success count."""
        if not self._enabled:
            return 0

        success = 0
        for wh in self._webhooks:
            # Filter by event type if specified
            types = wh.get("event_types", [])
            if types and event_type not in types and "*" not in types:
                continue

            url = wh.get("url", "")
            if not url:
                continue

            fmt = wh.get("format", "generic")
            payload = self._format_payload(fmt, event_type, message, data)

            if await self._post(url, payload, wh.get("headers", {})):
                success += 1

        return success

    def _format_payload(self, fmt: str, event_type: str,
                        message: str, data: Dict = None) -> Dict:
        """Format payload based on target format."""
        if fmt == "slack":
            return {
                "text": f"*[{event_type}]* {message}",
                "username": "TizenClaw",
                "icon_emoji": ":robot_face:",
            }
        elif fmt == "discord":
            return {
                "content": f"**[{event_type}]** {message}",
                "username": "TizenClaw",
            }
        else:
            return {
                "event_type": event_type,
                "message": message,
                "data": data or {},
                "timestamp": time.time(),
                "source": "tizenclaw",
            }

    async def _post(self, url: str, payload: Dict,
                    extra_headers: Dict = None) -> bool:
        """Post JSON payload with retry."""
        headers = {"Content-Type": "application/json"}
        if extra_headers:
            headers.update(extra_headers)

        body = json.dumps(payload, ensure_ascii=False).encode("utf-8")

        for attempt in range(self._retry_count + 1):
            try:
                req = urllib.request.Request(
                    url, data=body, method="POST", headers=headers
                )
                loop = asyncio.get_running_loop()
                await loop.run_in_executor(None, lambda: urllib.request.urlopen(
                    req, context=self._ssl_ctx, timeout=self._timeout
                ))
                return True
            except Exception as e:
                if attempt < self._retry_count:
                    await asyncio.sleep(1)
                else:
                    logger.warning(f"WebhookChannel: POST failed to {url}: {e}")
        return False

    def get_status(self) -> Dict[str, Any]:
        return {
            "enabled": self._enabled,
            "webhook_count": len(self._webhooks),
            "urls": [wh.get("url", "")[:50] + "..." for wh in self._webhooks],
        }
