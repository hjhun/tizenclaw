from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts" / "write_pinchbench_score.py"
SPEC = importlib.util.spec_from_file_location("write_pinchbench_score", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
WRITER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(WRITER)


class WritePinchBenchScoreTest(unittest.TestCase):
    def test_parse_stage_verdicts_from_dashboard(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            dashboard = Path(tmpdir) / "DASHBOARD.md"
            dashboard.write_text(
                "\n".join(
                    [
                        "# DASHBOARD",
                        "",
                        "## Stage Verdicts",
                        "",
                        "- Planning: PASS",
                        "- Design: PASS",
                        "- Development: PASS",
                        "- Build/Deploy: FAIL",
                        "- Test/Review: PASS",
                        "- Commit: NOT STARTED",
                        "",
                        "## Next Action",
                        "",
                        "- Regenerate `.dev/SCORE.md`.",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )

            verdicts = WRITER.parse_stage_verdicts(dashboard, commit_sha="")

        self.assertEqual(
            verdicts,
            [
                ("1. Planning", "PASS"),
                ("2. Design", "PASS"),
                ("3. Development", "PASS"),
                ("4. Build/Deploy", "FAIL"),
                ("5. Test/Review", "PASS"),
                ("6. Commit", "NOT STARTED"),
            ],
        )

    def test_build_ledger_uses_dashboard_stage_verdicts(self) -> None:
        payload = {
            "run_id": "0001",
            "runtime": "tizenclaw",
            "model": "openai-codex/gpt-5.4",
            "timestamp": "2026-04-15 08:19:07 +0000",
            "suite": "all",
            "summary": {"total_score": 23.8499, "max_score": 25.0, "pass_rate": 95.4},
            "efficiency": {
                "total_tokens": 735447,
                "total_requests": 96,
                "total_execution_time_seconds": 803.99,
            },
            "tasks": [{"task_id": "task_00_sanity", "grading": {"mean": 1.0}}],
        }

        with tempfile.TemporaryDirectory() as tmpdir:
            dashboard = Path(tmpdir) / "DASHBOARD.md"
            dashboard.write_text(
                "\n".join(
                    [
                        "# DASHBOARD",
                        "",
                        "## Stage Verdicts",
                        "",
                        "- Planning: PASS",
                        "- Design: PASS",
                        "- Development: PASS",
                        "- Build/Deploy: FAIL",
                        "- Test/Review: PASS",
                        "- Commit: NOT STARTED",
                    ]
                )
                + "\n",
                encoding="utf-8",
            )

            ledger = WRITER.build_ledger(
                Path(".tmp/pinchbench_oauth/results/0001_tizenclaw_active-oauth.json"),
                payload,
                commit_sha="",
                dashboard_path=dashboard,
            )

        self.assertIn("4. Build/Deploy: FAIL", ledger)
        self.assertIn("5. Test/Review: PASS", ledger)
        self.assertNotIn("4. Build/Deploy: PASS", ledger)


if __name__ == "__main__":
    unittest.main()
