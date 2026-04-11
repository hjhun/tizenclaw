"""Tool inventory helpers for parity and audit views."""

from __future__ import annotations

from dataclasses import asdict, dataclass
from pathlib import Path


@dataclass(frozen=True)
class ToolSpec:
    """Represents one tool surface found in the repository."""

    name: str
    category: str
    path: str
    summary: str
    source: str

    def to_dict(self) -> dict[str, str]:
        return asdict(self)


def _summary_from_markdown(path: Path) -> str:
    lines = [
        line.strip()
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    for line in lines:
        if not line.startswith("#"):
            return line
    return "No summary available."


def _embedded_tools(root: Path) -> list[ToolSpec]:
    embedded_dir = root / "tools" / "embedded"
    tools: list[ToolSpec] = []
    for path in sorted(embedded_dir.glob("*.md")):
        tools.append(
            ToolSpec(
                name=path.stem,
                category="embedded",
                path=str(path.relative_to(root)),
                summary=_summary_from_markdown(path),
                source="tools/embedded",
            )
        )
    return tools


def _cli_tools(root: Path) -> list[ToolSpec]:
    cli_dir = root / "tools" / "cli"
    tools: list[ToolSpec] = []
    for path in sorted(cli_dir.glob("*/tool.md")):
        tools.append(
            ToolSpec(
                name=path.parent.name,
                category="cli",
                path=str(path.relative_to(root)),
                summary=_summary_from_markdown(path),
                source="tools/cli",
            )
        )
    return tools


def collect_tool_inventory(root: Path) -> list[dict[str, str]]:
    """Return all known tool surfaces."""

    tool_specs = _embedded_tools(root) + _cli_tools(root)
    return [tool.to_dict() for tool in sorted(tool_specs, key=lambda item: item.name)]
