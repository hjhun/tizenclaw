"""
TizenClaw Slack Channel — Slack Bot integration using Web API.

Uses Slack socket mode (or polling conversations.history) with
only stdlib urllib. Sends messages via chat.postMessage.
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

SLACK_CONFIG_PATH = "/opt/usr/share/tizenclaw/config/slack_config.json"


class SlackChannel:
    """Slack Bot channel for TizenClaw."""

    def __init__(self):
        self._bot_token = ""
        self._app_token = ""
        self._channel_id = ""
        self._allowed_channels: List[str] = []
        self._enabled = False
        self._running = False
        self._agent = None
        self._ssl_ctx: Optional[ssl.SSLContext] = None
        self._last_ts = ""
        self._poll_interval = 3  # seconds
        self._task: Optional[asyncio.Task] = None

        try:
            self._ssl_ctx = ssl.create_default_context()
        except Exception:
            self._ssl_ctx = ssl._create_unverified_context()

    def _load_config(self) -> Optional[Dict]:
        if not os.path.isfile(SLACK_CONFIG_PATH):
            return None
        try:
            with open(SLACK_CONFIG_PATH, "r", encoding="utf-8") as f:
                return json.load(f)
        except Exception:
            return None

    async def start(self, agent_core) -> bool:
        """Start Slack channel."""
        self._agent = agent_core
        config = self._load_config()
        if not config:
            logger.info("SlackChannel: No config, disabled")
            return False

        self._bot_token = config.get("bot_token", "")
        self._channel_id = config.get("default_channel", "")
        self._allowed_channels = config.get("allowed_channels", [])
        self._poll_interval = config.get("poll_interval_seconds", 3)

        if not self._bot_token:
            logger.warning("SlackChannel: No bot_token configured")
            return False

        # Test auth
        ok = await self._test_auth()
        if not ok:
            return False

        self._enabled = True
        self._running = True
        self._task = asyncio.create_task(self._poll_loop())
        logger.info("SlackChannel: Started")
        return True

    async def stop(self):
        self._running = False
        if self._task:
            self._task.cancel()
            try:
                await self._task
            except asyncio.CancelledError:
                pass
        logger.info("SlackChannel: Stopped")

    async def _test_auth(self) -> bool:
        """Test bot token via auth.test."""
        try:
            data = await self._api_call("auth.test")
            if data.get("ok"):
                bot_name = data.get("user", "unknown")
                logger.info(f"SlackChannel: Authenticated as @{bot_name}")
                return True
            logger.error(f"SlackChannel: Auth failed: {data.get('error')}")
        except Exception as e:
            logger.error(f"SlackChannel: Auth error: {e}")
        return False

    async def _api_call(self, method: str, params: Dict = None) -> Dict:
        """Call Slack Web API."""
        url = f"https://slack.com/api/{method}"
        headers = {
            "Authorization": f"Bearer {self._bot_token}",
            "Content-Type": "application/json; charset=utf-8",
        }
        body = json.dumps(params or {}).encode("utf-8")
        req = urllib.request.Request(url, data=body, headers=headers, method="POST")

        loop = asyncio.get_running_loop()
        resp = await loop.run_in_executor(None, lambda: urllib.request.urlopen(
            req, context=self._ssl_ctx, timeout=15
        ))
        return json.loads(resp.read().decode("utf-8"))

    async def send_message(self, channel: str, text: str):
        """Send a message to a Slack channel."""
        try:
            await self._api_call("chat.postMessage", {
                "channel": channel,
                "text": text,
            })
        except Exception as e:
            logger.error(f"SlackChannel: Send failed: {e}")

    async def _poll_loop(self):
        """Poll for new messages using conversations.history."""
        # Get initial timestamp
        self._last_ts = str(time.time())

        while self._running:
            try:
                await asyncio.sleep(self._poll_interval)
                channels = self._allowed_channels or ([self._channel_id] if self._channel_id else [])

                for ch in channels:
                    await self._check_channel(ch)
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"SlackChannel: Poll error: {e}")
                await asyncio.sleep(10)

    async def _check_channel(self, channel: str):
        """Check for new messages in a channel."""
        try:
            data = await self._api_call("conversations.history", {
                "channel": channel,
                "oldest": self._last_ts,
                "limit": 10,
            })
            if not data.get("ok"):
                return

            messages = data.get("messages", [])
            for msg in reversed(messages):
                # Skip bot messages
                if msg.get("bot_id") or msg.get("subtype"):
                    continue

                text = msg.get("text", "").strip()
                ts = msg.get("ts", "")
                user = msg.get("user", "")

                if not text or float(ts) <= float(self._last_ts):
                    continue

                self._last_ts = ts
                logger.info(f"SlackChannel: Message from {user}: {text[:80]}")

                if self._agent:
                    response = await self._agent.process_prompt(f"slack_{user}", text)
                    await self.send_message(channel, response)

        except Exception as e:
            logger.error(f"SlackChannel: Check error: {e}")

    def get_status(self) -> Dict[str, Any]:
        return {
            "enabled": self._enabled,
            "running": self._running,
            "channel": self._channel_id,
            "allowed_channels": self._allowed_channels,
        }
