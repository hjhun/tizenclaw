import os
import json
import logging
from datetime import datetime
from typing import List, Dict, Any

logger = logging.getLogger(__name__)

class SessionStore:
    """
    Python implementation of TizenClaw SessionStore.
    Manages conversational history serialization (Markdown/JSON) and usage logging.
    """
    def __init__(self):
        self.sessions_dir = "/opt/usr/share/tizenclaw/sessions"
        self.logs_dir = "/opt/usr/share/tizenclaw/logs"
        self._ensure_dir(self.sessions_dir)
        self._ensure_dir(self.logs_dir)

    def set_directory(self, directory: str):
        self.sessions_dir = directory
        self._ensure_dir(self.sessions_dir)

    def _ensure_dir(self, directory: str):
        os.makedirs(directory, exist_ok=True)

    def save_session(self, session_id: str, history: List[Dict[str, Any]]) -> bool:
        """Save chat history to disk in Markdown format."""
        path = os.path.join(self.sessions_dir, f"{session_id}.md")
        try:
            content = self._messages_to_markdown(history)
            # Atomic write simulation
            tmp_path = path + ".tmp"
            with open(tmp_path, "w", encoding="utf-8") as f:
                f.write(content)
            os.replace(tmp_path, path)
            return True
        except Exception as e:
            logger.error(f"Failed to save session {session_id}: {e}")
            return False

    def load_session(self, session_id: str) -> List[Dict[str, Any]]:
        """Load session history from Markdown file."""
        path = os.path.join(self.sessions_dir, f"{session_id}.md")
        if not os.path.exists(path):
            return []
        try:
            with open(path, "r", encoding="utf-8") as f:
                content = f.read()
            return self._markdown_to_messages(content)
        except Exception as e:
            logger.error(f"Failed to load session {session_id}: {e}")
            return []

    def delete_session(self, session_id: str):
        path = os.path.join(self.sessions_dir, f"{session_id}.md")
        if os.path.exists(path):
            os.remove(path)

    def log_skill_execution(self, session_id: str, skill_name: str, args: Dict[str, Any], result: str, duration_ms: int):
        log_entry = {
            "timestamp": datetime.utcnow().isoformat(),
            "session_id": session_id,
            "skill_name": skill_name,
            "args": args,
            "result": result,
            "duration_ms": duration_ms
        }
        # Appending to a daily log file
        log_file = os.path.join(self.logs_dir, f"skills_{datetime.utcnow().strftime('%Y-%m-%d')}.log")
        with open(log_file, "a", encoding="utf-8") as f:
            f.write(json.dumps(log_entry) + "\n")

    def _messages_to_markdown(self, history: List[Dict[str, Any]]) -> str:
        # Placeholder for Markdown serialization logic
        return json.dumps(history, indent=2)

    def _markdown_to_messages(self, content: str) -> List[Dict[str, Any]]:
        # Placeholder for Markdown deserialization logic
        return json.loads(content)
