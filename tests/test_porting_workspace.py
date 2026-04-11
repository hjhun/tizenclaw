from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from src.commands import build_command_graph, build_command_inventory
from src.parity_audit import run_parity_audit
from src.port_manifest import build_port_manifest
from src.query_engine import QueryEngine
from src.runtime import build_bootstrap_graph, build_runtime_summary
from src.session_store import load_session
from src.tool_pool import build_tool_pool


class PortingWorkspaceTest(unittest.TestCase):
    def test_command_inventory_and_graph_are_wired(self) -> None:
        inventory = build_command_inventory()
        graph = build_command_graph()
        self.assertGreaterEqual(len(inventory), 8)
        self.assertEqual(graph["command_count"], len(inventory))
        self.assertEqual(graph["root"], "main")

    def test_tool_pool_collects_embedded_and_cli_tools(self) -> None:
        pool = build_tool_pool(ROOT)
        self.assertGreater(pool["tool_count"], 0)
        self.assertGreater(len(pool["groups"]["embedded"]), 0)
        self.assertGreater(len(pool["groups"]["cli"]), 0)

    def test_manifest_tracks_document_reference_gaps(self) -> None:
        manifest = build_port_manifest(ROOT)
        missing_docs = [
            item["path"]
            for item in manifest["document_references"]
            if not item["exists"]
        ]
        self.assertIn(
            "docs/claw-code-analysis/files/src/main.py.md",
            missing_docs,
        )

    def test_runtime_summary_and_bootstrap_graph_are_meaningful(self) -> None:
        summary = build_runtime_summary(ROOT)
        bootstrap = build_bootstrap_graph(ROOT)
        self.assertIn("src.main", summary["python_modules"])
        self.assertGreater(len(summary["rust_workspace_crates"]), 0)
        self.assertGreater(len(bootstrap["edges"]), 0)

    def test_query_engine_finds_python_workspace_records(self) -> None:
        engine = QueryEngine(ROOT)
        result = engine.search("manifest", domain="commands")
        self.assertEqual(result["match_count"], 1)
        self.assertEqual(result["matches"][0]["name"], "manifest")

    def test_session_loader_summarizes_json_files(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "session.json"
            path.write_text(
                json.dumps({"messages": [{"role": "user"}, {"role": "assistant"}]}),
                encoding="utf-8",
            )
            summary = load_session(path)
        self.assertEqual(summary["message_count"], 2)
        self.assertEqual(summary["type"], "dict")

    def test_parity_audit_warns_on_missing_reference_docs_only(self) -> None:
        report = run_parity_audit(ROOT)
        self.assertEqual(report["status"], "warn")
        self.assertEqual(report["missing_files"], [])
        self.assertIn(
            "docs/claw-code-analysis/files/src/main.py.md",
            report["warnings"]["missing_document_references"],
        )

    def test_cli_manifest_command_is_runnable(self) -> None:
        command = [sys.executable, "-m", "src.main", "manifest", "--format", "json"]
        completed = subprocess.run(
            command,
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        )
        payload = json.loads(completed.stdout)
        self.assertEqual(payload["workspace"], "python-parity")

    def test_cli_query_command_supports_json_output(self) -> None:
        command = [
            sys.executable,
            "-m",
            "src.main",
            "query",
            "tool",
            "--domain",
            "tools",
            "--format",
            "json",
        ]
        completed = subprocess.run(
            command,
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        )
        payload = json.loads(completed.stdout)
        self.assertGreater(payload["match_count"], 0)
