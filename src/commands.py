"""Command inventory and graph helpers for the parity CLI."""

from __future__ import annotations

from dataclasses import asdict, dataclass, field


@dataclass(frozen=True)
class CommandSpec:
    """Describes one top-level parity CLI command."""

    name: str
    mode: str
    summary: str
    outputs: tuple[str, ...]
    arguments: tuple[str, ...] = ()
    tags: tuple[str, ...] = ()

    def to_dict(self) -> dict[str, object]:
        return asdict(self)


COMMAND_SPECS: tuple[CommandSpec, ...] = (
    CommandSpec(
        name="inventory",
        mode="inventory",
        summary="Show command, tool, and runtime inventory views.",
        outputs=("text", "json"),
        arguments=("section", "format"),
        tags=("inventory", "summary"),
    ),
    CommandSpec(
        name="manifest",
        mode="manifest",
        summary="Generate the Python parity port manifest.",
        outputs=("text", "json"),
        arguments=("format",),
        tags=("manifest", "audit"),
    ),
    CommandSpec(
        name="audit",
        mode="audit",
        summary="Run parity checks against the documented workspace contract.",
        outputs=("text", "json"),
        arguments=("format", "strict"),
        tags=("audit", "parity"),
    ),
    CommandSpec(
        name="query",
        mode="query",
        summary="Search inventory, docs, and module metadata.",
        outputs=("text", "json"),
        arguments=("term", "domain", "format"),
        tags=("query", "search"),
    ),
    CommandSpec(
        name="runtime",
        mode="runtime",
        summary="Summarize the mirrored runtime and bootstrap topology.",
        outputs=("text", "json"),
        arguments=("format",),
        tags=("runtime", "bootstrap"),
    ),
    CommandSpec(
        name="session",
        mode="session",
        summary="Load and summarize a JSON session or scenario file.",
        outputs=("text", "json"),
        arguments=("path", "format"),
        tags=("session", "analysis"),
    ),
    CommandSpec(
        name="commands",
        mode="commands",
        summary="Display the command graph for CLI mode routing.",
        outputs=("text", "json"),
        arguments=("format",),
        tags=("graph", "cli"),
    ),
    CommandSpec(
        name="bootstrap",
        mode="bootstrap",
        summary="Display the bootstrap graph for parity workspace wiring.",
        outputs=("text", "json"),
        arguments=("format",),
        tags=("graph", "bootstrap"),
    ),
)


def build_command_inventory() -> list[dict[str, object]]:
    """Return the documented command inventory."""

    return [spec.to_dict() for spec in COMMAND_SPECS]


def build_command_graph() -> dict[str, object]:
    """Return a lightweight graph of CLI routing relationships."""

    nodes = [
        {"id": "main", "kind": "entrypoint", "label": "python -m src.main"},
    ]
    nodes.extend(
        {"id": spec.name, "kind": "command", "label": spec.summary}
        for spec in COMMAND_SPECS
    )

    edges = [
        {
            "source": "main",
            "target": spec.name,
            "label": spec.mode,
        }
        for spec in COMMAND_SPECS
    ]

    return {
        "root": "main",
        "nodes": nodes,
        "edges": edges,
        "command_count": len(COMMAND_SPECS),
    }
