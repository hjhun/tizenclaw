"""Port manifest generation for the parity workspace."""

from __future__ import annotations

from pathlib import Path

from .commands import build_command_inventory
from .runtime import build_runtime_summary
from .tool_pool import build_tool_pool


DOCUMENT_REFERENCES = [
    "docs/claw-code-analysis/overview-python.md",
    "docs/claw-code-analysis/files/src/main.py.md",
    "docs/claw-code-analysis/files/src/query_engine.py.md",
    "docs/claw-code-analysis/files/src/runtime.py.md",
    "docs/claw-code-analysis/files/src/commands.py.md",
    "docs/claw-code-analysis/files/src/tools.py.md",
    "docs/claw-code-analysis/files/src/parity_audit.py.md",
    "docs/claw-code-analysis/files/src/port_manifest.py.md",
    "docs/claw-code-analysis/files/src/tool_pool.py.md",
    "docs/claw-code-analysis/files/src/session_store.py.md",
    "docs/claw-code-analysis/files/tests/test_porting_workspace.py.md",
]


def build_port_manifest(root: Path) -> dict[str, object]:
    """Generate a manifest describing the current parity workspace."""

    command_inventory = build_command_inventory()
    tool_pool = build_tool_pool(root)
    runtime_summary = build_runtime_summary(root)
    doc_status = [
        {"path": path, "exists": (root / path).exists()}
        for path in DOCUMENT_REFERENCES
    ]

    return {
        "workspace": "python-parity",
        "purpose": [
            "command and tool inventory",
            "port manifest generation",
            "parity audit flows",
            "query and summary views",
            "lightweight CLI access",
        ],
        "command_inventory": command_inventory,
        "tool_pool": tool_pool,
        "runtime_summary": runtime_summary,
        "document_references": doc_status,
    }
