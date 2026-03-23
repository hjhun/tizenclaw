"""
TizenClaw Tool Policy — Security policy for tool execution.

Controls which tools can be executed, rate limiting,
and loop detection to prevent infinite tool call chains.
"""
import json
import logging
import os
import time
from typing import Dict, Any, Optional, List, Set

logger = logging.getLogger(__name__)

POLICY_PATH = "/opt/usr/share/tizenclaw/config/tool_policy.json"


class ToolPolicy:
    """Enforces security policies on tool execution."""

    def __init__(self):
        self._allowed_tools: Optional[Set[str]] = None  # None = all allowed
        self._blocked_tools: Set[str] = set()
        self._rate_limits: Dict[str, int] = {}  # tool -> max calls per minute
        self._call_history: Dict[str, List[float]] = {}
        self._loop_window = 60  # seconds
        self._max_same_calls = 5  # max calls to same tool in window
        self._enabled = True

    def load_policy(self, path: str = POLICY_PATH) -> bool:
        if not os.path.isfile(path):
            logger.info("No tool_policy.json found, using defaults")
            return False
        try:
            with open(path, "r") as f:
                policy = json.load(f)
            if "allowed_tools" in policy:
                self._allowed_tools = set(policy["allowed_tools"])
            if "blocked_tools" in policy:
                self._blocked_tools = set(policy["blocked_tools"])
            if "rate_limits" in policy:
                self._rate_limits = policy["rate_limits"]
            self._loop_window = policy.get("loop_window_seconds", 60)
            self._max_same_calls = policy.get("max_same_calls_in_window", 5)
            self._enabled = policy.get("enabled", True)
            logger.info(f"Loaded tool policy: blocked={len(self._blocked_tools)}, rate_limits={len(self._rate_limits)}")
            return True
        except Exception as e:
            logger.error(f"Failed to load tool policy: {e}")
            return False

    def check(self, tool_name: str) -> tuple:
        """Check if a tool execution is allowed. Returns (allowed: bool, reason: str)."""
        if not self._enabled:
            return True, "policy disabled"

        # Check blocklist
        if tool_name in self._blocked_tools:
            return False, f"tool '{tool_name}' is blocked by policy"

        # Check allowlist
        if self._allowed_tools is not None and tool_name not in self._allowed_tools:
            return False, f"tool '{tool_name}' is not in allowed list"

        # Check rate limit
        if tool_name in self._rate_limits:
            max_calls = self._rate_limits[tool_name]
            now = time.time()
            history = self._call_history.get(tool_name, [])
            recent = [t for t in history if now - t < 60]
            if len(recent) >= max_calls:
                return False, f"tool '{tool_name}' rate limit exceeded ({max_calls}/min)"

        # Check loop detection
        now = time.time()
        history = self._call_history.get(tool_name, [])
        recent = [t for t in history if now - t < self._loop_window]
        if len(recent) >= self._max_same_calls:
            return False, f"loop detected: '{tool_name}' called {len(recent)} times in {self._loop_window}s"

        return True, "allowed"

    def record_call(self, tool_name: str):
        """Record a tool call for rate limiting and loop detection."""
        if tool_name not in self._call_history:
            self._call_history[tool_name] = []
        self._call_history[tool_name].append(time.time())
        # Cleanup old entries
        cutoff = time.time() - max(self._loop_window, 60) * 2
        self._call_history[tool_name] = [t for t in self._call_history[tool_name] if t > cutoff]

    def get_status(self) -> Dict[str, Any]:
        return {
            "enabled": self._enabled,
            "blocked_tools": list(self._blocked_tools),
            "allowed_tools": list(self._allowed_tools) if self._allowed_tools else "all",
            "rate_limits": self._rate_limits,
            "loop_window_seconds": self._loop_window,
            "max_same_calls": self._max_same_calls,
        }
