"""TizenClaw Telegram Bot Channel.

Pure-asyncio Telegram Bot API client using long-polling (getUpdates).
No external dependencies — uses only the standard-library ``urllib.request``.

Lifecycle:
  1. ``start(agent)`` — loads config, validates token, launches polling task
  2. Incoming messages are forwarded to ``AgentCore.process_prompt()``
  3. Responses are sent back via ``sendMessage``
  4. ``stop()`` — cancels polling task
"""
import asyncio
import json
import logging
import os
import urllib.request
import urllib.error
import urllib.parse
from typing import Optional, Dict, Any, List

logger = logging.getLogger(__name__)

CONFIG_DIR = "/opt/usr/share/tizenclaw/config"
DEFAULT_CONFIG_FILE = "telegram_config.json"

# Telegram Bot API base URL
TELEGRAM_API = "https://api.telegram.org/bot{token}/{method}"


class TelegramClient:
    """Async Telegram Bot that bridges chat messages to AgentCore."""

    def __init__(self):
        self.bot_token: str = ""
        self.allowed_chat_ids: List[int] = []
        self.bot_username: str = ""
        self._polling_task: Optional[asyncio.Task] = None
        self._running = False
        self._offset = 0  # getUpdates offset
        self._agent = None

    # ── public API ──────────────────────────────────────────────

    async def start(self, agent) -> bool:
        """Load config, validate bot token, and start the polling loop."""
        self._agent = agent

        config = self._load_config()
        if not config:
            logger.error("Telegram: No config found — channel disabled")
            return False

        self.bot_token = config.get("bot_token", "")
        self.allowed_chat_ids = config.get("allowed_chat_ids", [])

        if not self.bot_token or self.bot_token.startswith("YOUR_"):
            logger.error("Telegram: bot_token not configured — channel disabled")
            return False

        # Validate token with getMe
        me = await self._api_call("getMe")
        if not me or not me.get("ok"):
            logger.error(f"Telegram: getMe failed — invalid bot_token? response={me}")
            return False

        self.bot_username = me["result"].get("username", "unknown")
        logger.info(f"Telegram: Bot @{self.bot_username} connected successfully")

        self._running = True
        self._polling_task = asyncio.create_task(self._poll_loop())
        return True

    async def stop(self):
        """Stop the polling loop."""
        self._running = False
        if self._polling_task:
            self._polling_task.cancel()
            try:
                await self._polling_task
            except asyncio.CancelledError:
                pass
        logger.info("Telegram: Bot stopped")

    async def send_message(self, chat_id: int, text: str) -> bool:
        """Send a text message to a specific chat."""
        # Telegram message limit is 4096 chars; split if needed
        chunks = [text[i:i + 4000] for i in range(0, len(text), 4000)]
        for chunk in chunks:
            result = await self._api_call("sendMessage", {
                "chat_id": chat_id,
                "text": chunk,
                "parse_mode": "Markdown",
            })
            if not result or not result.get("ok"):
                # Retry without markdown if it fails (markdown parse errors)
                result = await self._api_call("sendMessage", {
                    "chat_id": chat_id,
                    "text": chunk,
                })
                if not result or not result.get("ok"):
                    logger.error(f"Telegram: sendMessage failed: {result}")
                    return False
        return True

    # ── polling loop ────────────────────────────────────────────

    async def _poll_loop(self):
        """Long-poll getUpdates and dispatch messages."""
        logger.info("Telegram: Polling loop started")

        while self._running:
            try:
                updates = await self._api_call("getUpdates", {
                    "offset": self._offset,
                    "timeout": 30,  # long poll timeout
                    "allowed_updates": json.dumps(["message"]),
                })

                if not updates or not updates.get("ok"):
                    logger.warning(f"Telegram: getUpdates failed: {updates}")
                    await asyncio.sleep(5)
                    continue

                for update in updates.get("result", []):
                    self._offset = update["update_id"] + 1
                    await self._handle_update(update)

            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"Telegram: Polling error: {e}")
                await asyncio.sleep(5)

        logger.info("Telegram: Polling loop stopped")

    async def _handle_update(self, update: Dict[str, Any]):
        """Process a single Telegram update."""
        message = update.get("message")
        if not message:
            return

        chat_id = message["chat"]["id"]
        text = message.get("text", "")
        user = message.get("from", {})
        username = user.get("username", user.get("first_name", "unknown"))

        if not text:
            return

        # Check allowed_chat_ids (empty = allow all)
        if self.allowed_chat_ids and chat_id not in self.allowed_chat_ids:
            logger.warning(f"Telegram: Ignoring message from unauthorized chat {chat_id}")
            await self.send_message(chat_id, "⚠️ This chat is not authorized to use TizenClaw.")
            return

        # Skip bot commands that aren't meant for us
        if text.startswith("/start"):
            await self.send_message(chat_id,
                f"👋 안녕하세요! TizenClaw AI Agent입니다.\n"
                f"디바이스에 대한 질문을 자유롭게 해주세요.\n\n"
                f"예시:\n"
                f"• 배터리 상태 알려줘\n"
                f"• 현재 실행중인 앱 목록\n"
                f"• Wi-Fi 정보 확인\n"
                f"• 화면 밝기 50으로 설정"
            )
            return

        if text.startswith("/help"):
            await self.send_message(chat_id,
                "🔧 *TizenClaw 도움말*\n\n"
                "자연어로 디바이스를 제어할 수 있습니다:\n"
                "• 디바이스 정보 조회 (배터리, 네트워크, 센서 등)\n"
                "• 앱 관리 (실행, 종료, 목록)\n"
                "• 설정 변경 (밝기, 볼륨 등)\n"
                "• 파일 관리\n"
                "• 알림 전송\n\n"
                "그냥 평소에 말하듯이 질문하세요!"
            )
            return

        logger.info(f"Telegram: [{username}] {text[:100]}")

        # Send typing indicator
        await self._api_call("sendChatAction", {
            "chat_id": chat_id,
            "action": "typing",
        })

        # Forward to AgentCore
        try:
            session_id = f"telegram_{chat_id}"
            response = await self._agent.process_prompt(session_id, text)

            if response:
                await self.send_message(chat_id, response)
            else:
                await self.send_message(chat_id, "⚠️ 응답을 생성하지 못했습니다.")

        except Exception as e:
            logger.error(f"Telegram: Error processing message: {e}")
            await self.send_message(chat_id, f"❌ 오류가 발생했습니다: {e}")

    # ── Telegram API helper ─────────────────────────────────────

    async def _api_call(
        self, method: str, params: Optional[Dict[str, Any]] = None
    ) -> Optional[Dict[str, Any]]:
        """Make an async HTTP request to the Telegram Bot API.

        Uses ``urllib.request`` in a thread executor to avoid blocking
        the event loop (no aiohttp dependency required).
        """
        url = TELEGRAM_API.format(token=self.bot_token, method=method)
        loop = asyncio.get_running_loop()

        try:
            result = await loop.run_in_executor(
                None, self._sync_api_call, url, params
            )
            return result
        except Exception as e:
            logger.error(f"Telegram API {method} error: {e}")
            return None

    @staticmethod
    def _sync_api_call(
        url: str, params: Optional[Dict[str, Any]]
    ) -> Optional[Dict[str, Any]]:
        """Synchronous HTTP POST to Telegram API."""
        try:
            if params:
                data = json.dumps(params).encode("utf-8")
                req = urllib.request.Request(
                    url,
                    data=data,
                    headers={"Content-Type": "application/json"},
                    method="POST",
                )
            else:
                req = urllib.request.Request(url)

            with urllib.request.urlopen(req, timeout=35) as resp:
                body = resp.read().decode("utf-8")
                return json.loads(body)

        except urllib.error.HTTPError as e:
            body = e.read().decode("utf-8", errors="ignore")
            logger.error(f"Telegram HTTP {e.code}: {body[:200]}")
            return {"ok": False, "error_code": e.code, "description": body[:200]}
        except Exception as e:
            logger.error(f"Telegram request error: {e}")
            return None

    # ── config ──────────────────────────────────────────────────

    @staticmethod
    def _load_config() -> Optional[Dict[str, Any]]:
        """Load telegram_config.json from the config directory."""
        config_path = os.path.join(CONFIG_DIR, DEFAULT_CONFIG_FILE)

        # Also check devel config (higher priority for development)
        devel_path = os.path.join(CONFIG_DIR, "devel", DEFAULT_CONFIG_FILE)

        for path in [devel_path, config_path]:
            if os.path.isfile(path):
                try:
                    with open(path, "r", encoding="utf-8") as f:
                        config = json.load(f)
                    logger.info(f"Telegram: Loaded config from {path}")
                    return config
                except Exception as e:
                    logger.error(f"Telegram: Failed to load {path}: {e}")

        return None
