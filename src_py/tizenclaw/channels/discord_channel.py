"""
TizenClaw Discord Channel — Discord Bot integration using REST API polling.

Uses Discord HTTP API (no external WebSocket library needed).
Polls for messages via GET /channels/{id}/messages.
Sends replies via POST /channels/{id}/messages.
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

DISCORD_CONFIG_PATH = "/opt/usr/share/tizenclaw/config/discord_config.json"
DISCORD_API = "https://discord.com/api/v10"


class DiscordChannel:
    """Discord Bot channel for TizenClaw."""

    def __init__(self):
        self._bot_token = ""
        self._channel_ids: List[str] = []
        self._bot_id = ""
        self._enabled = False
        self._running = False
        self._agent = None
        self._ssl_ctx: Optional[ssl.SSLContext] = None
        self._last_message_ids: Dict[str, str] = {}
        self._poll_interval = 3
        self._task: Optional[asyncio.Task] = None

        try:
            self._ssl_ctx = ssl.create_default_context()
        except Exception:
            self._ssl_ctx = ssl._create_unverified_context()

    def _load_config(self) -> Optional[Dict]:
        if not os.path.isfile(DISCORD_CONFIG_PATH):
            return None
        try:
            with open(DISCORD_CONFIG_PATH, "r", encoding="utf-8") as f:
                return json.load(f)
        except Exception:
            return None

    async def start(self, agent_core) -> bool:
        self._agent = agent_core
        config = self._load_config()
        if not config:
            logger.info("DiscordChannel: No config, disabled")
            return False

        self._bot_token = config.get("bot_token", "")
        self._channel_ids = config.get("channel_ids", [])
        self._poll_interval = config.get("poll_interval_seconds", 3)

        if not self._bot_token:
            logger.warning("DiscordChannel: No bot_token configured")
            return False

        # Get bot user ID
        ok = await self._get_bot_user()
        if not ok:
            return False

        self._enabled = True
        self._running = True
        self._task = asyncio.create_task(self._poll_loop())
        logger.info(f"DiscordChannel: Started, watching {len(self._channel_ids)} channels")
        return True

    async def stop(self):
        self._running = False
        if self._task:
            self._task.cancel()
            try:
                await self._task
            except asyncio.CancelledError:
                pass
        logger.info("DiscordChannel: Stopped")

    async def _api_get(self, endpoint: str) -> Dict:
        url = f"{DISCORD_API}{endpoint}"
        headers = {
            "Authorization": f"Bot {self._bot_token}",
        }
        req = urllib.request.Request(url, headers=headers, method="GET")
        loop = asyncio.get_running_loop()
        resp = await loop.run_in_executor(None, lambda: urllib.request.urlopen(
            req, context=self._ssl_ctx, timeout=15
        ))
        return json.loads(resp.read().decode("utf-8"))

    async def _api_post(self, endpoint: str, data: Dict) -> Dict:
        url = f"{DISCORD_API}{endpoint}"
        headers = {
            "Authorization": f"Bot {self._bot_token}",
            "Content-Type": "application/json",
        }
        body = json.dumps(data).encode("utf-8")
        req = urllib.request.Request(url, data=body, headers=headers, method="POST")
        loop = asyncio.get_running_loop()
        resp = await loop.run_in_executor(None, lambda: urllib.request.urlopen(
            req, context=self._ssl_ctx, timeout=15
        ))
        return json.loads(resp.read().decode("utf-8"))

    async def _get_bot_user(self) -> bool:
        try:
            data = await self._api_get("/users/@me")
            self._bot_id = data.get("id", "")
            name = data.get("username", "unknown")
            logger.info(f"DiscordChannel: Authenticated as {name} (ID: {self._bot_id})")
            return True
        except Exception as e:
            logger.error(f"DiscordChannel: Auth failed: {e}")
            return False

    async def send_message(self, channel_id: str, text: str):
        """Send a message to a Discord channel."""
        # Discord max message length is 2000
        chunks = [text[i:i+1900] for i in range(0, len(text), 1900)]
        for chunk in chunks:
            try:
                await self._api_post(f"/channels/{channel_id}/messages", {
                    "content": chunk,
                })
            except Exception as e:
                logger.error(f"DiscordChannel: Send failed: {e}")

    async def _poll_loop(self):
        """Poll channels for new messages."""
        # Initialize last message IDs
        for ch_id in self._channel_ids:
            try:
                data = await self._api_get(f"/channels/{ch_id}/messages?limit=1")
                if isinstance(data, list) and data:
                    self._last_message_ids[ch_id] = data[0].get("id", "0")
            except Exception:
                self._last_message_ids[ch_id] = "0"

        while self._running:
            try:
                await asyncio.sleep(self._poll_interval)
                for ch_id in self._channel_ids:
                    await self._check_channel(ch_id)
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"DiscordChannel: Poll error: {e}")
                await asyncio.sleep(10)

    async def _check_channel(self, channel_id: str):
        after = self._last_message_ids.get(channel_id, "0")
        try:
            data = await self._api_get(
                f"/channels/{channel_id}/messages?after={after}&limit=10"
            )
            if not isinstance(data, list):
                return

            for msg in reversed(data):
                author = msg.get("author", {})
                # Skip bot's own messages
                if author.get("id") == self._bot_id or author.get("bot"):
                    self._last_message_ids[channel_id] = msg["id"]
                    continue

                text = msg.get("content", "").strip()
                msg_id = msg.get("id", "0")
                user_id = author.get("id", "unknown")
                username = author.get("username", "user")

                self._last_message_ids[channel_id] = msg_id

                if not text:
                    continue

                logger.info(f"DiscordChannel: {username}: {text[:80]}")

                if self._agent:
                    response = await self._agent.process_prompt(
                        f"discord_{user_id}", text
                    )
                    await self.send_message(channel_id, response)

        except Exception as e:
            logger.error(f"DiscordChannel: Check error: {e}")

    def get_status(self) -> Dict[str, Any]:
        return {
            "enabled": self._enabled,
            "running": self._running,
            "bot_id": self._bot_id,
            "channels": self._channel_ids,
        }
