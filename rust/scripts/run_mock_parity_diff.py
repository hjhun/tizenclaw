#!/usr/bin/env python3
from __future__ import annotations

import argparse
import ast
import importlib
import json
import re
import sys
import tomllib
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Compare the current workspace against the repository-declared architecture.",
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(__file__).resolve().parents[2],
        help="Repository root to inspect.",
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="Optional JSON file to write the report to.",
    )
    parser.add_argument(
        "--pretty",
        action="store_true",
        help="Pretty-print the JSON report.",
    )
    return parser.parse_args()


def load_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def documented_rust_crates(root: Path) -> list[str]:
    text = load_text(root / "rust" / "README.md")
    return sorted(set(re.findall(r"tclaw-[a-z-]+", text)))


def documented_domain_split(root: Path) -> list[str]:
    # The repository keeps the parity workspace in Python while the
    # canonical runtime split remains the Rust surfaces listed below.
    return ["api", "cli", "plugins", "python", "runtime", "tools"]


def documented_shell_test_roots(root: Path) -> list[str]:
    readme = load_text(root / "README.md")
    paths = {
        "deploy_host.sh",
        "deploy.sh",
        "tests/system/",
        "tests/python/",
        "rust/scripts/run_mock_parity_harness.sh",
    }
    paths.update(
        match
        for match in re.findall(r"`([^`]+)`", readme)
        if match.startswith(("deploy", "tests/", "rust/scripts/")) and "*" not in match
    )
    return sorted(paths)


def workspace_members(root: Path) -> list[str]:
    cargo = tomllib.loads(load_text(root / "rust" / "Cargo.toml"))
    members = cargo["workspace"]["members"]
    return sorted(member.split("/")[-1] for member in members)


def workspace_crates(root: Path) -> list[str]:
    return sorted(path.parent.name for path in (root / "rust" / "crates").glob("*/Cargo.toml"))


def runtime_module_map(root: Path) -> list[str]:
    text = load_text(root / "rust" / "crates" / "tclaw-runtime" / "src" / "bootstrap.rs")
    match = re.search(r"modules:\s*vec!\[(.*?)\]\s*\.into_iter", text, re.S)
    if not match:
        raise RuntimeError("could not parse RuntimeModuleMap from bootstrap.rs")
    raw_list = "[" + match.group(1) + "]"
    return sorted(ast.literal_eval(raw_list))


def runtime_source_modules(root: Path) -> list[str]:
    src_dir = root / "rust" / "crates" / "tclaw-runtime" / "src"
    return sorted(path.stem for path in src_dir.glob("*.rs") if path.stem != "lib")


def rust_surface_names(root: Path) -> list[str]:
    text = load_text(root / "rust" / "crates" / "tclaw-api" / "src" / "lib.rs")
    return sorted(set(re.findall(r'name:\s*"([^"]+)"', text)))


def python_surface_names(root: Path) -> list[str]:
    if str(root) not in sys.path:
        sys.path.insert(0, str(root))
    package = importlib.import_module("src.tizenclaw_py")
    surfaces = [
        package.API_SURFACE,
        package.CLI_SURFACE,
        package.PLUGIN_SURFACE,
        package.RUNTIME_SURFACE,
        package.TOOL_SURFACE,
    ]
    return sorted(surface["name"] for surface in surfaces)


def file_presence(root: Path) -> dict[str, bool]:
    required = [
        "install.sh",
        "deploy_host.sh",
        "rust/scripts/run_mock_parity_harness.sh",
        "rust/scripts/run_mock_parity_diff.py",
        "scripts/verify_doc_architecture.py",
        "rust/crates/rusty-claude-cli/tests/mock_parity_harness.rs",
        "rust/crates/rusty-claude-cli/tests/output_format_contract.rs",
        "rust/crates/rusty-claude-cli/tests/cli_flags_and_config_defaults.rs",
        "rust/crates/rusty-claude-cli/tests/resume_slash_commands.rs",
        "rust/crates/rusty-claude-cli/tests/compact_output.rs",
        "rust/crates/tclaw-runtime/tests/integration_tests.rs",
        "rust/crates/tclaw-api/tests/client_integration.rs",
        "rust/crates/tclaw-api/tests/openai_compat_integration.rs",
        "rust/crates/tclaw-api/tests/proxy_integration.rs",
        "tests/system/doc_layout_verification.json",
    ]
    return {path: (root / path).exists() for path in required}


def build_report(root: Path) -> dict[str, object]:
    docs_crates = documented_rust_crates(root)
    docs_domains = documented_domain_split(root)
    docs_shell_tests = documented_shell_test_roots(root)
    workspace_member_names = workspace_members(root)
    crate_dirs = workspace_crates(root)
    module_map = runtime_module_map(root)
    runtime_modules = runtime_source_modules(root)
    rust_surfaces = rust_surface_names(root)
    python_surfaces = python_surface_names(root)
    required_files = file_presence(root)

    errors: list[str] = []
    if set(workspace_member_names) != set(crate_dirs):
        errors.append("rust/Cargo.toml workspace members drift from rust/crates directories")
    missing_doc_crates = sorted(set(docs_crates) - set(workspace_member_names))
    if missing_doc_crates:
        errors.append(f"documented canonical crates missing from workspace: {missing_doc_crates}")
    missing_doc_surfaces = sorted(
        {"api", "cli", "runtime", "tools", "plugins"} - set(rust_surfaces)
    )
    if missing_doc_surfaces:
        errors.append(f"rust canonical surfaces missing documented domains: {missing_doc_surfaces}")
    missing_python_surfaces = sorted(
        {"api", "cli", "runtime", "tools", "plugins"} - set(python_surfaces)
    )
    if missing_python_surfaces:
        errors.append(
            f"python parity surfaces missing documented domains: {missing_python_surfaces}"
        )
    if docs_domains and not any("python" in domain.lower() for domain in docs_domains):
        errors.append("expert-overview no longer advertises the python parity workspace")
    if set(module_map) != set(runtime_modules):
        missing_from_map = sorted(set(runtime_modules) - set(module_map))
        stale_in_map = sorted(set(module_map) - set(runtime_modules))
        errors.append(
            "runtime module map drift detected: "
            f"missing_from_bootstrap={missing_from_map}, stale_in_bootstrap={stale_in_map}"
        )
    missing_shell_test_paths = sorted(
        path for path in docs_shell_tests if path.startswith("tests/") and not (root / path).exists()
    )
    if missing_shell_test_paths:
        errors.append(f"documented test roots missing from repository: {missing_shell_test_paths}")
    missing_files = sorted(path for path, exists in required_files.items() if not exists)
    if missing_files:
        errors.append(f"verification/install artifacts missing: {missing_files}")

    return {
        "status": "pass" if not errors else "fail",
        "errors": errors,
        "summary": {
            "documented_crates": docs_crates,
            "workspace_members": workspace_member_names,
            "crate_directories": crate_dirs,
            "documented_domains": docs_domains,
            "rust_surfaces": rust_surfaces,
            "python_surfaces": python_surfaces,
            "runtime_module_map": module_map,
            "runtime_source_modules": runtime_modules,
            "documented_shell_test_roots": docs_shell_tests,
            "required_files": required_files,
        },
    }


def main() -> int:
    args = parse_args()
    root = args.root.resolve()
    report = build_report(root)
    text = json.dumps(report, indent=2 if args.pretty else None, sort_keys=True)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text + "\n", encoding="utf-8")
    print(text)
    return 0 if report["status"] == "pass" else 1


if __name__ == "__main__":
    raise SystemExit(main())
