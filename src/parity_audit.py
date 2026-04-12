"""Repository audit helpers for the support-tool workspace."""

from __future__ import annotations

from pathlib import Path

from .port_manifest import build_port_manifest


EXPECTED_FILES = [
    "src/__init__.py",
    "src/main.py",
    "src/commands.py",
    "src/tools.py",
    "src/tool_pool.py",
    "src/query_engine.py",
    "src/parity_audit.py",
    "src/port_manifest.py",
    "src/runtime.py",
    "src/session_store.py",
    "tests/test_porting_workspace.py",
]


def run_parity_audit(root: Path) -> dict[str, object]:
    """Check whether the parity workspace contract is present and wired."""

    manifest = build_port_manifest(root)
    missing_files = [path for path in EXPECTED_FILES if not (root / path).exists()]
    missing_docs = [
        item["path"]
        for item in manifest["document_references"]
        if not item["exists"]
    ]

    if missing_files:
        status = "fail"
    elif missing_docs:
        status = "warn"
    else:
        status = "pass"

    return {
        "status": status,
        "missing_files": missing_files,
        "warnings": {
            "missing_document_references": missing_docs,
        },
        "summary": {
            "command_count": len(manifest["command_inventory"]),
            "tool_count": manifest["tool_pool"]["tool_count"],
            "python_module_count": len(manifest["runtime_summary"]["python_modules"]),
        },
    }
