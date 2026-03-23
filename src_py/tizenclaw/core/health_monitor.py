"""
TizenClaw HealthMonitor — tracks runtime metrics for /api/metrics endpoint.

Counters: requests, errors, LLM calls, tool calls
System: uptime, memory (VmRSS), CPU load, PID, threads
"""
import json
import logging
import os
import threading
import time
from typing import Dict, Any, Optional

logger = logging.getLogger(__name__)


class HealthMonitor:
    """Thread-safe health metrics tracker."""

    def __init__(self):
        self._start_time = time.time()
        self._lock = threading.Lock()
        self._requests = 0
        self._errors = 0
        self._llm_calls = 0
        self._tool_calls = 0

    # ── Counter operations (thread-safe) ──

    def increment_request(self):
        with self._lock:
            self._requests += 1

    def increment_error(self):
        with self._lock:
            self._errors += 1

    def increment_llm_call(self):
        with self._lock:
            self._llm_calls += 1

    def increment_tool_call(self):
        with self._lock:
            self._tool_calls += 1

    # ── Getters ──

    def get_request_count(self) -> int:
        return self._requests

    def get_error_count(self) -> int:
        return self._errors

    def get_llm_call_count(self) -> int:
        return self._llm_calls

    def get_tool_call_count(self) -> int:
        return self._tool_calls

    def get_uptime_seconds(self) -> float:
        return time.time() - self._start_time

    # ── System metrics ──

    @staticmethod
    def _get_vm_rss_kb() -> int:
        """Read VmRSS from /proc/self/status."""
        try:
            with open("/proc/self/status", "r") as f:
                for line in f:
                    if line.startswith("VmRSS:"):
                        return int(line.split()[1])
        except Exception:
            pass
        return 0

    @staticmethod
    def _get_load_avg() -> tuple:
        """Read 1/5/15 min load averages."""
        try:
            with open("/proc/loadavg", "r") as f:
                parts = f.read().split()
                return float(parts[0]), float(parts[1]), float(parts[2])
        except Exception:
            return 0.0, 0.0, 0.0

    @staticmethod
    def _get_thread_count() -> int:
        try:
            with open("/proc/self/status", "r") as f:
                for line in f:
                    if line.startswith("Threads:"):
                        return int(line.split()[1])
        except Exception:
            pass
        return 1

    # ── JSON metrics export ──

    def get_metrics_json(self) -> str:
        """Return a JSON string with all health metrics."""
        uptime = self.get_uptime_seconds()
        hours = int(uptime // 3600)
        minutes = int((uptime % 3600) // 60)
        seconds = int(uptime % 60)

        load1, load5, load15 = self._get_load_avg()

        metrics = {
            "uptime": {
                "seconds": round(uptime, 1),
                "formatted": f"{hours}h {minutes}m {seconds}s",
            },
            "counters": {
                "requests": self._requests,
                "errors": self._errors,
                "llm_calls": self._llm_calls,
                "tool_calls": self._tool_calls,
            },
            "memory": {
                "vm_rss_kb": self._get_vm_rss_kb(),
            },
            "cpu": {
                "load_1m": load1,
                "load_5m": load5,
                "load_15m": load15,
            },
            "threads": self._get_thread_count(),
            "pid": os.getpid(),
        }
        return json.dumps(metrics)

    def get_metrics_dict(self) -> Dict[str, Any]:
        return json.loads(self.get_metrics_json())


# Global singleton
_health_monitor: Optional[HealthMonitor] = None


def get_health_monitor() -> HealthMonitor:
    global _health_monitor
    if _health_monitor is None:
        _health_monitor = HealthMonitor()
    return _health_monitor
