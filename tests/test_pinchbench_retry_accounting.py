from __future__ import annotations

import importlib.util
import logging
import subprocess
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace


ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = ROOT / "scripts" / "run_pinchbench_oauth.py"
SPEC = importlib.util.spec_from_file_location("run_pinchbench_oauth", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
PINCHBENCH = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PINCHBENCH)


class PinchBenchRetryAccountingTest(unittest.TestCase):
    def test_task_entry_and_efficiency_include_failed_and_judge_retries(self) -> None:
        task = SimpleNamespace(task_id="retry_task", frontmatter={})

        execution_attempts = [
            {
                "attempt_kind": "execution",
                "attempt_index": 1,
                "status": "error",
                "transient_failure": True,
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 5,
                    "cache_read_tokens": 0,
                    "cache_write_tokens": 0,
                    "total_tokens": 15,
                    "cost_usd": 0.0,
                    "request_count": 1,
                },
                "execution_time": 3.5,
            },
            {
                "attempt_kind": "execution",
                "attempt_index": 2,
                "status": "success",
                "transient_failure": False,
                "usage": {
                    "input_tokens": 20,
                    "output_tokens": 10,
                    "cache_read_tokens": 0,
                    "cache_write_tokens": 0,
                    "total_tokens": 30,
                    "cost_usd": 0.0,
                    "request_count": 1,
                },
                "execution_time": 5.0,
            },
        ]
        judge_attempts = [
            {
                "attempt_kind": "judge",
                "attempt_index": 1,
                "status": "error",
                "transient_failure": True,
                "usage": {
                    "input_tokens": 7,
                    "output_tokens": 3,
                    "cache_read_tokens": 0,
                    "cache_write_tokens": 0,
                    "total_tokens": 10,
                    "cost_usd": 0.0,
                    "request_count": 1,
                },
                "execution_time": 2.0,
            },
            {
                "attempt_kind": "judge",
                "attempt_index": 2,
                "status": "success",
                "transient_failure": False,
                "usage": {
                    "input_tokens": 8,
                    "output_tokens": 2,
                    "cache_read_tokens": 0,
                    "cache_write_tokens": 0,
                    "total_tokens": 10,
                    "cost_usd": 0.0,
                    "request_count": 1,
                },
                "execution_time": 1.5,
            },
        ]

        grade = SimpleNamespace(to_dict=lambda: {"score": 1.0}, score=1.0, max_score=1.0)
        terminal_result = {
            "status": "success",
            "timed_out": False,
            "workspace": "/tmp/retry-task",
            "transcript": [{"role": "assistant", "content": "done"}],
        }

        run_record = PINCHBENCH.build_run_record(
            run_index=1,
            terminal_result=terminal_result,
            execution_attempts=execution_attempts,
            judge_attempts=judge_attempts,
            grade=grade,
        )
        grades_summary = {
            "runs": [{"score": 1.0}],
            "mean": 1.0,
            "std": 0.0,
            "min": 1.0,
            "max": 1.0,
        }
        task_entry = PINCHBENCH.build_task_entry(
            task=task,
            grades_summary=grades_summary,
            run_records=[run_record],
        )
        efficiency = PINCHBENCH.compute_efficiency_summary(
            [task_entry],
            {"retry_task": grades_summary},
        )

        self.assertEqual(task_entry["usage"]["total_tokens"], 65)
        self.assertEqual(task_entry["usage"]["request_count"], 4)
        self.assertEqual(task_entry["execution_time"], 12.0)
        self.assertEqual(task_entry["retry_summary"]["execution_attempt_count"], 2)
        self.assertEqual(task_entry["retry_summary"]["judge_attempt_count"], 2)
        self.assertEqual(task_entry["retry_summary"]["failed_attempt_count"], 2)
        self.assertEqual(task_entry["retry_summary"]["transient_retry_count"], 2)
        self.assertEqual(task_entry["retry_summary"]["usage_accounting_incomplete_attempt_count"], 0)
        self.assertTrue(task_entry["usage_accounting"]["complete"])

        self.assertEqual(efficiency["total_tokens"], 65)
        self.assertEqual(efficiency["total_requests"], 4)
        self.assertEqual(efficiency["total_execution_time_seconds"], 12.0)
        self.assertEqual(efficiency["tasks_with_usage_data"], 1)
        self.assertTrue(efficiency["usage_accounting_complete"])
        self.assertEqual(efficiency["usage_accounting_incomplete_attempt_count"], 0)
        self.assertEqual(efficiency["per_task"][0]["execution_attempt_count"], 2)
        self.assertEqual(efficiency["per_task"][0]["judge_attempt_count"], 2)
        self.assertEqual(efficiency["per_task"][0]["failed_attempt_count"], 2)
        self.assertEqual(
            efficiency["per_task"][0]["usage_accounting_incomplete_attempt_count"],
            0,
        )

    def test_timeout_attempts_keep_usage_deltas(self) -> None:
        class TimeoutLibAgent:
            def __init__(self, root: Path) -> None:
                self.root = root
                self.usage: dict[str, dict[str, int | float]] = {}
                self.transcripts: dict[str, list[dict[str, str]]] = {}

            def _get_runtime_skills_dir(self, runtime: str) -> Path:
                path = self.root / "runtime-skills"
                path.mkdir(parents=True, exist_ok=True)
                return path

            def _cleanup_tizenclaw_session(self, session_id: str) -> None:
                self.usage.pop(session_id, None)
                self.transcripts.pop(session_id, None)

            def _tizenclaw_workdir(self, session_id: str) -> Path:
                return self.root / "sessions" / session_id

            def _read_tizenclaw_usage(
                self,
                session_id: str,
                baseline: dict[str, int | float] | None = None,
            ) -> dict[str, int | float]:
                current = dict(
                    self.usage.get(
                        session_id,
                        {
                            "input_tokens": 0,
                            "output_tokens": 0,
                            "cache_read_tokens": 0,
                            "cache_write_tokens": 0,
                            "total_tokens": 0,
                            "cost_usd": 0.0,
                            "request_count": 0,
                        },
                    )
                )
                if baseline is None:
                    return current
                return {key: current[key] - baseline.get(key, 0) for key in current}

            def _load_tizenclaw_transcript(self, session_id: str) -> list[dict[str, str]]:
                return list(self.transcripts.get(session_id, []))

            def _wait_for_tizenclaw_transcript_slice(
                self,
                session_id: str,
                start_index: int,
            ) -> list[dict[str, str]]:
                return self._load_tizenclaw_transcript(session_id)[start_index:]

            def _transcript_has_agent_activity(self, transcript: list[dict[str, str]]) -> bool:
                return bool(transcript)

            def _coerce_subprocess_output(self, value: object) -> str:
                if value is None:
                    return ""
                if isinstance(value, bytes):
                    return value.decode("utf-8", errors="replace")
                return str(value)

            def _run_tizenclaw_message(
                self,
                session_id: str,
                prompt: str,
                workspace: Path,
                timeout_seconds: float,
            ) -> SimpleNamespace:
                self.usage[session_id] = {
                    "input_tokens": 11,
                    "output_tokens": 4,
                    "cache_read_tokens": 0,
                    "cache_write_tokens": 0,
                    "total_tokens": 15,
                    "cost_usd": 0.0,
                    "request_count": 1,
                }
                self.transcripts[session_id] = [
                    {"role": "assistant", "content": "partial reply before timeout"}
                ]
                raise subprocess.TimeoutExpired(
                    cmd=["tizenclaw-cli"],
                    timeout=timeout_seconds,
                    output="HTTP 429 before timeout\n",
                    stderr="",
                )

        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            lib_agent = TimeoutLibAgent(root)
            task = SimpleNamespace(
                task_id="timeout_task",
                name="Timeout Task",
                category="tests",
                prompt="prompt",
                timeout_seconds=1.0,
                frontmatter={},
                workspace_files=[],
            )

            result = PINCHBENCH.execute_tizenclaw_task_active_config(
                lib_agent=lib_agent,
                task=task,
                agent_id="bench-test",
                run_id="run-1",
                timeout_multiplier=1.0,
                skill_root=root,
                scratch_root=root / "scratch",
                verbose=False,
                stream_io=False,
                logger=logging.getLogger("pinchbench-timeout-test"),
            )

            self.assertTrue(result["timed_out"])
            self.assertEqual(result["usage"]["total_tokens"], 15)
            self.assertEqual(result["usage"]["request_count"], 1)
            self.assertEqual(len(result["transcript"]), 1)
            self.assertEqual(result["usage_accounting_warnings"], [])

    def test_usage_read_failures_are_marked_in_attempt_and_efficiency_output(self) -> None:
        class UsageReadFailureLibAgent:
            def __init__(self, root: Path) -> None:
                self.root = root
                self.usage: dict[str, dict[str, int | float]] = {}
                self.transcripts: dict[str, list[dict[str, str]]] = {}

            def _get_runtime_skills_dir(self, runtime: str) -> Path:
                path = self.root / "runtime-skills"
                path.mkdir(parents=True, exist_ok=True)
                return path

            def _cleanup_tizenclaw_session(self, session_id: str) -> None:
                self.usage.pop(session_id, None)
                self.transcripts.pop(session_id, None)

            def _tizenclaw_workdir(self, session_id: str) -> Path:
                return self.root / "sessions" / session_id

            def _read_tizenclaw_usage(
                self,
                session_id: str,
                baseline: dict[str, int | float] | None = None,
            ) -> dict[str, int | float]:
                if baseline is not None:
                    raise RuntimeError("usage snapshot unavailable")
                return {
                    "input_tokens": 0,
                    "output_tokens": 0,
                    "cache_read_tokens": 0,
                    "cache_write_tokens": 0,
                    "total_tokens": 0,
                    "cost_usd": 0.0,
                    "request_count": 0,
                }

            def _load_tizenclaw_transcript(self, session_id: str) -> list[dict[str, str]]:
                return list(self.transcripts.get(session_id, []))

            def _wait_for_tizenclaw_transcript_slice(
                self,
                session_id: str,
                start_index: int,
            ) -> list[dict[str, str]]:
                return self._load_tizenclaw_transcript(session_id)[start_index:]

            def _transcript_has_agent_activity(self, transcript: list[dict[str, str]]) -> bool:
                return bool(transcript)

            def _coerce_subprocess_output(self, value: object) -> str:
                if value is None:
                    return ""
                if isinstance(value, bytes):
                    return value.decode("utf-8", errors="replace")
                return str(value)

            def _run_tizenclaw_message(
                self,
                session_id: str,
                prompt: str,
                workspace: Path,
                timeout_seconds: float,
            ) -> SimpleNamespace:
                self.transcripts[session_id] = [
                    {"role": "assistant", "content": "completed despite missing usage snapshot"}
                ]
                return SimpleNamespace(stdout="ok\n", stderr="", returncode=0)

        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            lib_agent = UsageReadFailureLibAgent(root)
            task = SimpleNamespace(
                task_id="usage_gap_task",
                name="Usage Gap Task",
                category="tests",
                prompt="prompt",
                timeout_seconds=1.0,
                frontmatter={},
                workspace_files=[],
            )

            result = PINCHBENCH.execute_tizenclaw_task_active_config(
                lib_agent=lib_agent,
                task=task,
                agent_id="bench-test",
                run_id="run-usage-gap",
                timeout_multiplier=1.0,
                skill_root=root,
                scratch_root=root / "scratch",
                verbose=False,
                stream_io=False,
                logger=logging.getLogger("pinchbench-usage-gap-test"),
            )
            attempt = PINCHBENCH.build_attempt_record(
                attempt_kind="execution",
                attempt_index=1,
                transient_failure=False,
                result=result,
            )
            grade = SimpleNamespace(to_dict=lambda: {"score": 1.0}, score=1.0, max_score=1.0)
            run_record = PINCHBENCH.build_run_record(
                run_index=1,
                terminal_result=result,
                execution_attempts=[attempt],
                judge_attempts=[],
                grade=grade,
            )
            grades_summary = {
                "runs": [{"score": 1.0}],
                "mean": 1.0,
                "std": 0.0,
                "min": 1.0,
                "max": 1.0,
            }
            task_entry = PINCHBENCH.build_task_entry(
                task=task,
                grades_summary=grades_summary,
                run_records=[run_record],
            )
            efficiency = PINCHBENCH.compute_efficiency_summary(
                [task_entry],
                {"usage_gap_task": grades_summary},
            )

            self.assertEqual(result["usage"]["total_tokens"], 0)
            self.assertEqual(
                result["usage_accounting_warnings"],
                ["usage_read_failed:RuntimeError"],
            )
            self.assertFalse(attempt["usage_accounting_complete"])
            self.assertEqual(
                attempt["usage_accounting_warnings"],
                ["usage_read_failed:RuntimeError"],
            )
            self.assertFalse(run_record["usage_accounting"]["complete"])
            self.assertEqual(run_record["usage_accounting"]["incomplete_attempt_count"], 1)
            self.assertFalse(task_entry["usage_accounting"]["complete"])
            self.assertEqual(
                task_entry["retry_summary"]["usage_accounting_incomplete_attempt_count"],
                1,
            )
            self.assertFalse(efficiency["usage_accounting_complete"])
            self.assertEqual(efficiency["usage_accounting_incomplete_task_count"], 1)
            self.assertEqual(efficiency["usage_accounting_incomplete_attempt_count"], 1)
            self.assertEqual(
                efficiency["usage_accounting_warning_codes"],
                ["usage_read_failed:RuntimeError"],
            )
            self.assertFalse(efficiency["per_task"][0]["usage_accounting_complete"])
            self.assertEqual(
                efficiency["per_task"][0]["usage_accounting_warning_codes"],
                ["usage_read_failed:RuntimeError"],
            )


if __name__ == "__main__":
    unittest.main()
