"""Assembly helpers for parity tool pools."""

from __future__ import annotations

from pathlib import Path

from .tools import collect_tool_inventory


def build_tool_pool(root: Path) -> dict[str, object]:
    """Assemble a grouped tool pool for inventory and audit flows."""

    tools = collect_tool_inventory(root)
    grouped = {
        "embedded": [tool for tool in tools if tool["category"] == "embedded"],
        "cli": [tool for tool in tools if tool["category"] == "cli"],
    }
    return {
        "tool_count": len(tools),
        "groups": grouped,
        "tools": tools,
    }
