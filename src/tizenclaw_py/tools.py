from __future__ import annotations

from pathlib import Path

from src.tool_pool import build_tool_pool


TOOL_SURFACE = {
    "name": "tools",
    "role": "tool contract parity helpers",
    "tool_count": build_tool_pool(Path(__file__).resolve().parents[2])["tool_count"],
}
