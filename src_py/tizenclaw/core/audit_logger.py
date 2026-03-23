"""
TizenClaw AuditLogger — records all tool calls, LLM requests, and security events.
"""
import json
import logging
import os
import time
from typing import Dict, Any, Optional

logger = logging.getLogger(__name__)

AUDIT_DIR = "/opt/usr/share/tizenclaw/work/audit"


class AuditLogger:
    """File-based audit logger for all TizenClaw operations."""

    def __init__(self, audit_dir: str = AUDIT_DIR):
        self._dir = audit_dir
        os.makedirs(self._dir, exist_ok=True)
        self._current_file = None
        self._current_date = ""

    def _get_file(self):
        today = time.strftime("%Y-%m-%d")
        if today != self._current_date:
            if self._current_file:
                self._current_file.close()
            self._current_date = today
            path = os.path.join(self._dir, f"{today}.log")
            self._current_file = open(path, "a", encoding="utf-8")
        return self._current_file

    def _write(self, entry: Dict[str, Any]):
        entry["timestamp"] = time.strftime("%Y-%m-%dT%H:%M:%S")
        try:
            f = self._get_file()
            f.write(json.dumps(entry, ensure_ascii=False) + "\n")
            f.flush()
        except Exception as e:
            logger.error(f"AuditLogger write error: {e}")

    def log_tool_call(self, tool_name: str, arguments: str, result: str,
                      session_id: str = "", duration_ms: int = 0):
        self._write({
            "type": "tool_call",
            "tool": tool_name,
            "args": arguments[:500],
            "result": result[:500],
            "session": session_id,
            "duration_ms": duration_ms,
        })

    def log_llm_request(self, backend: str, model: str, prompt_preview: str,
                        tokens_used: int = 0, duration_ms: int = 0,
                        success: bool = True, error: str = ""):
        self._write({
            "type": "llm_request",
            "backend": backend,
            "model": model,
            "prompt": prompt_preview[:200],
            "tokens": tokens_used,
            "duration_ms": duration_ms,
            "success": success,
            "error": error[:200],
        })

    def log_auth_event(self, event_type: str, ip: str = "", success: bool = True):
        self._write({
            "type": "auth",
            "event": event_type,
            "ip": ip,
            "success": success,
        })

    def log_security_event(self, event: str, details: str = ""):
        self._write({
            "type": "security",
            "event": event,
            "details": details[:500],
        })

    def log_agent_event(self, agent_id: str, event: str, details: str = ""):
        self._write({
            "type": "agent",
            "agent_id": agent_id,
            "event": event,
            "details": details[:500],
        })

    def close(self):
        if self._current_file:
            self._current_file.close()
            self._current_file = None


# Global singleton
_audit_logger: Optional[AuditLogger] = None

def get_audit_logger() -> AuditLogger:
    global _audit_logger
    if _audit_logger is None:
        _audit_logger = AuditLogger()
    return _audit_logger
