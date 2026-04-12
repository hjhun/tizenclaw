"""CLI shim for repository support tooling."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any

from .commands import build_command_graph, build_command_inventory
from .parity_audit import run_parity_audit
from .port_manifest import build_port_manifest
from .query_engine import QueryEngine
from .runtime import build_bootstrap_graph, build_runtime_summary
from .session_store import load_session
from .tool_pool import build_tool_pool


def repo_root() -> Path:
    return Path(__file__).resolve().parents[1]


def _emit(payload: Any, output_format: str) -> str:
    if output_format == "json":
        return json.dumps(payload, indent=2, sort_keys=True)
    if isinstance(payload, str):
        return payload
    return json.dumps(payload, indent=2, sort_keys=True)


def _format_inventory(root: Path, section: str) -> dict[str, object]:
    if section == "commands":
        return {"section": section, "items": build_command_inventory()}
    if section == "tools":
        pool = build_tool_pool(root)
        return {"section": section, "items": pool["tools"], "tool_count": pool["tool_count"]}
    if section == "runtime":
        return {"section": section, "items": build_runtime_summary(root)}
    manifest = build_port_manifest(root)
    return {
        "section": "all",
        "commands": manifest["command_inventory"],
        "tools": manifest["tool_pool"]["tools"],
        "runtime": manifest["runtime_summary"],
    }


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="python -m src.main",
        description="TizenClaw repository support CLI",
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    inventory = subparsers.add_parser("inventory", help="Show support-tool inventory")
    inventory.add_argument(
        "section",
        nargs="?",
        default="all",
        choices=("all", "commands", "tools", "runtime"),
    )
    inventory.add_argument("--format", choices=("json",), default="json")

    manifest = subparsers.add_parser("manifest", help="Generate port manifest")
    manifest.add_argument("--format", choices=("json",), default="json")

    audit = subparsers.add_parser("audit", help="Run repository audit")
    audit.add_argument("--format", choices=("json",), default="json")
    audit.add_argument("--strict", action="store_true")

    query = subparsers.add_parser("query", help="Search repository records")
    query.add_argument("term")
    query.add_argument(
        "--domain",
        choices=("all", "commands", "tools", "modules", "docs", "crates"),
        default="all",
    )
    query.add_argument("--format", choices=("json",), default="json")

    runtime = subparsers.add_parser("runtime", help="Show runtime summary")
    runtime.add_argument("--format", choices=("json",), default="json")

    session = subparsers.add_parser("session", help="Load a JSON session file")
    session.add_argument("path")
    session.add_argument("--format", choices=("json",), default="json")

    commands = subparsers.add_parser("commands", help="Show command graph")
    commands.add_argument("--format", choices=("json",), default="json")

    bootstrap = subparsers.add_parser("bootstrap", help="Show bootstrap graph")
    bootstrap.add_argument("--format", choices=("json",), default="json")

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    root = repo_root()

    if args.command == "inventory":
        print(_emit(_format_inventory(root, args.section), args.format))
        return 0
    if args.command == "manifest":
        print(_emit(build_port_manifest(root), args.format))
        return 0
    if args.command == "audit":
        report = run_parity_audit(root)
        print(_emit(report, args.format))
        if args.strict and report["status"] != "pass":
            return 1
        return 0
    if args.command == "query":
        engine = QueryEngine(root)
        print(_emit(engine.search(args.term, domain=args.domain), args.format))
        return 0
    if args.command == "runtime":
        print(_emit(build_runtime_summary(root), args.format))
        return 0
    if args.command == "session":
        print(_emit(load_session(Path(args.path)), args.format))
        return 0
    if args.command == "commands":
        print(_emit(build_command_graph(), args.format))
        return 0
    if args.command == "bootstrap":
        print(_emit(build_bootstrap_graph(root), args.format))
        return 0

    parser.error(f"unsupported command: {args.command}")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
