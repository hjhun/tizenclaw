"""Runtime and bootstrap summary support for the parity workspace."""

from __future__ import annotations

from pathlib import Path


PYTHON_MODULES = [
    "src.__init__",
    "src.main",
    "src.commands",
    "src.tools",
    "src.tool_pool",
    "src.query_engine",
    "src.parity_audit",
    "src.port_manifest",
    "src.runtime",
    "src.session_store",
]


def _cargo_packages(paths: list[Path], root: Path) -> list[str]:
    return sorted(str(path.parent.relative_to(root)) for path in paths)


def build_runtime_summary(root: Path) -> dict[str, object]:
    """Summarize the mirrored runtime topology."""

    rust_workspace = _cargo_packages(
        list((root / "rust" / "crates").glob("*/Cargo.toml")),
        root,
    )
    legacy_workspace = _cargo_packages(
        list((root / "src").glob("*/Cargo.toml")),
        root,
    )
    python_tests = sorted(
        str(path.relative_to(root))
        for path in (root / "tests").rglob("test_*.py")
    )

    return {
        "mode": "parity-analysis",
        "canonical_runtime": "rust",
        "python_role": "audit-port",
        "python_modules": PYTHON_MODULES,
        "rust_workspace_crates": rust_workspace,
        "legacy_workspace_crates": legacy_workspace,
        "python_test_files": python_tests,
    }


def build_bootstrap_graph(root: Path) -> dict[str, object]:
    """Describe how the parity workspace is wired together."""

    runtime = build_runtime_summary(root)
    nodes = [
        {"id": "repo", "kind": "root", "label": "repository"},
        {"id": "python", "kind": "workspace", "label": "src package"},
        {"id": "rust", "kind": "workspace", "label": "rust/crates"},
        {"id": "legacy", "kind": "workspace", "label": "legacy src/* crates"},
        {"id": "tests", "kind": "workspace", "label": "tests"},
    ]
    nodes.extend(
        {"id": module, "kind": "python_module", "label": module}
        for module in runtime["python_modules"]
    )

    edges = [
        {"source": "repo", "target": "python", "label": "contains"},
        {"source": "repo", "target": "rust", "label": "contains"},
        {"source": "repo", "target": "legacy", "label": "contains"},
        {"source": "repo", "target": "tests", "label": "contains"},
    ]
    edges.extend(
        {"source": "python", "target": module, "label": "exports"}
        for module in runtime["python_modules"]
    )

    return {
        "nodes": nodes,
        "edges": edges,
        "rust_crate_count": len(runtime["rust_workspace_crates"]),
        "legacy_crate_count": len(runtime["legacy_workspace_crates"]),
    }
