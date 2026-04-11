"""Session loading helpers for parity analysis workflows."""

from __future__ import annotations

import json
from pathlib import Path


def _message_count(payload: object) -> int:
    if isinstance(payload, dict):
        for key in ("messages", "events", "steps", "calls"):
            value = payload.get(key)
            if isinstance(value, list):
                return len(value)
    if isinstance(payload, list):
        return len(payload)
    return 0


def load_session(path: Path) -> dict[str, object]:
    """Load a JSON session-like file and compute a small summary."""

    payload = json.loads(path.read_text(encoding="utf-8"))
    if isinstance(payload, dict):
        keys = sorted(payload.keys())
    else:
        keys = []

    return {
        "path": str(path),
        "exists": path.exists(),
        "type": type(payload).__name__,
        "top_level_keys": keys,
        "message_count": _message_count(payload),
        "payload": payload,
    }
