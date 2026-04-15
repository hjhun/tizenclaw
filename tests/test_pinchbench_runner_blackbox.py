from __future__ import annotations

import json
import os
import stat
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
RUNNER = ROOT / "scripts" / "run_pinchbench_oauth.py"


class PinchBenchRunnerBlackBoxTest(unittest.TestCase):
    def test_runner_emits_retry_inclusive_result_json(self) -> None:
        test_temp_root = ROOT / ".tmp" / "test_pinchbench_runner_blackbox"
        test_temp_root.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=test_temp_root) as tmpdir:
            temp_root = Path(tmpdir)
            skill_root = temp_root / "skill"
            scripts_dir = skill_root / "scripts"
            tasks_dir = skill_root / "tasks"
            runtime_skills = temp_root / "runtime-skills"
            bin_dir = temp_root / "bin"
            output_dir = temp_root / "results"
            scratch_root = temp_root / "scratch"

            scripts_dir.mkdir(parents=True)
            tasks_dir.mkdir(parents=True)
            runtime_skills.mkdir(parents=True)
            bin_dir.mkdir(parents=True)

            (scripts_dir / "lib_tasks.py").write_text(
                textwrap.dedent(
                    """
                    from __future__ import annotations

                    from dataclasses import dataclass, field


                    @dataclass
                    class Task:
                        task_id: str
                        name: str
                        category: str
                        prompt: str
                        timeout_seconds: float
                        frontmatter: dict = field(default_factory=dict)
                        workspace_files: list = field(default_factory=list)
                        grading_type: str = "llm_judge"
                        llm_judge_rubric: str = "Return a score."


                    class TaskLoader:
                        def __init__(self, tasks_dir):
                            self.tasks_dir = tasks_dir

                        def load_all_tasks(self):
                            return [
                                Task(
                                    task_id="retry_task",
                                    name="Retry Task",
                                    category="blackbox",
                                    prompt="runtime prompt",
                                    timeout_seconds=5.0,
                                )
                            ]
                    """
                ),
                encoding="utf-8",
            )
            (scripts_dir / "lib_grading.py").write_text(
                textwrap.dedent(
                    """
                    from __future__ import annotations

                    import json
                    from dataclasses import dataclass


                    @dataclass
                    class GradeResult:
                        task_id: str
                        score: float
                        max_score: float
                        grading_type: str
                        breakdown: dict
                        notes: str = ""

                        def to_dict(self):
                            return {
                                "task_id": self.task_id,
                                "score": self.score,
                                "max_score": self.max_score,
                                "grading_type": self.grading_type,
                                "breakdown": self.breakdown,
                                "notes": self.notes,
                            }


                    def _summarize_transcript(transcript):
                        return "summary"


                    def _format_grading_criteria(task):
                        return "criteria"


                    def _build_judge_prompt(task, transcript_summary, rubric):
                        return "judge prompt"


                    def _parse_judge_response(transcript):
                        if not transcript:
                            return {}
                        content = transcript[-1].get("content", "")
                        try:
                            return json.loads(content)
                        except json.JSONDecodeError:
                            return {}


                    def _normalize_judge_response(raw):
                        return raw or {}


                    def _normalize_score_dict(scores):
                        return dict(scores or {})


                    def _grade_automated(task, execution_result, verbose=False):
                        return GradeResult(
                            task_id=task.task_id,
                            score=1.0,
                            max_score=1.0,
                            grading_type="automated",
                            breakdown={},
                            notes="",
                        )


                    def _combine_grades(task, auto_result, llm_result):
                        return llm_result
                    """
                ),
                encoding="utf-8",
            )
            (scripts_dir / "lib_agent.py").write_text(
                textwrap.dedent(
                    f"""
                    from __future__ import annotations

                    import subprocess
                    import time
                    from pathlib import Path
                    from types import SimpleNamespace


                    _ROOT = Path({str(temp_root)!r})
                    _RUNTIME_SKILLS = Path({str(runtime_skills)!r})
                    _USAGE: dict[str, dict] = {{}}
                    _TRANSCRIPTS: dict[str, list] = {{}}
                    _EXECUTION_ATTEMPTS = 0
                    _JUDGE_ATTEMPTS = 0


                    def _get_runtime_skills_dir(runtime):
                        return _RUNTIME_SKILLS


                    def _cleanup_tizenclaw_session(session_id):
                        _USAGE.pop(session_id, None)
                        _TRANSCRIPTS.pop(session_id, None)


                    def _tizenclaw_workdir(session_id):
                        return _ROOT / "sessions" / session_id


                    def _coerce_subprocess_output(value):
                        if value is None:
                            return ""
                        if isinstance(value, bytes):
                            return value.decode("utf-8", errors="replace")
                        return str(value)


                    def _load_tizenclaw_transcript(session_id):
                        return list(_TRANSCRIPTS.get(session_id, []))


                    def _wait_for_tizenclaw_transcript_slice(session_id, start_index):
                        return _load_tizenclaw_transcript(session_id)[start_index:]


                    def _transcript_has_agent_activity(transcript):
                        return bool(transcript)


                    def _read_tizenclaw_usage(session_id, baseline=None):
                        current = dict(_USAGE.get(session_id, {{
                            "input_tokens": 0,
                            "output_tokens": 0,
                            "cache_read_tokens": 0,
                            "cache_write_tokens": 0,
                            "total_tokens": 0,
                            "cost_usd": 0.0,
                            "request_count": 0,
                        }}))
                        if baseline is None:
                            return dict(current)
                        delta = {{}}
                        for key, value in current.items():
                            delta[key] = value - baseline.get(key, 0)
                        return delta


                    def _run_tizenclaw_message(session_id, prompt, workspace, timeout_seconds):
                        global _EXECUTION_ATTEMPTS, _JUDGE_ATTEMPTS

                        if session_id.startswith("judge_"):
                            _JUDGE_ATTEMPTS += 1
                            if _JUDGE_ATTEMPTS == 1:
                                time.sleep(0.18)
                                _USAGE[session_id] = {{
                                    "input_tokens": 7,
                                    "output_tokens": 3,
                                    "cache_read_tokens": 0,
                                    "cache_write_tokens": 0,
                                    "total_tokens": 10,
                                    "cost_usd": 0.0,
                                    "request_count": 1,
                                }}
                                _TRANSCRIPTS[session_id] = [
                                    {{"role": "assistant", "content": "judge backend retry"}}
                                ]
                                raise subprocess.TimeoutExpired(
                                    cmd=["tizenclaw-cli"],
                                    timeout=timeout_seconds,
                                    output="HTTP 503 temporary outage\\n",
                                    stderr="",
                                )

                            time.sleep(0.06)
                            _USAGE[session_id] = {{
                                "input_tokens": 8,
                                "output_tokens": 2,
                                "cache_read_tokens": 0,
                                "cache_write_tokens": 0,
                                "total_tokens": 10,
                                "cost_usd": 0.0,
                                "request_count": 1,
                            }}
                            _TRANSCRIPTS[session_id] = [
                                {{
                                    "role": "assistant",
                                    "content": "{{\\"total\\": 1.0, \\"scores\\": {{}}, \\"notes\\": \\"ok\\"}}",
                                }}
                            ]
                            return SimpleNamespace(stdout="judge ok\\n", stderr="", returncode=0)

                        _EXECUTION_ATTEMPTS += 1
                        if _EXECUTION_ATTEMPTS == 1:
                            time.sleep(0.22)
                            _USAGE[session_id] = {{
                                "input_tokens": 10,
                                "output_tokens": 5,
                                "cache_read_tokens": 0,
                                "cache_write_tokens": 0,
                                "total_tokens": 15,
                                "cost_usd": 0.0,
                                "request_count": 1,
                            }}
                            _TRANSCRIPTS[session_id] = [
                                {{"role": "assistant", "content": "transient runtime failure"}}
                            ]
                            raise subprocess.TimeoutExpired(
                                cmd=["tizenclaw-cli"],
                                timeout=timeout_seconds,
                                output="HTTP 429 retry me\\n",
                                stderr="",
                            )

                        time.sleep(0.07)
                        _USAGE[session_id] = {{
                            "input_tokens": 20,
                            "output_tokens": 10,
                            "cache_read_tokens": 0,
                            "cache_write_tokens": 0,
                            "total_tokens": 30,
                            "cost_usd": 0.0,
                            "request_count": 1,
                        }}
                        _TRANSCRIPTS[session_id] = [
                            {{"role": "assistant", "content": "task completed"}}
                        ]
                        return SimpleNamespace(stdout="task ok\\n", stderr="", returncode=0)
                    """
                ),
                encoding="utf-8",
            )

            cli_path = bin_dir / "tizenclaw-cli"
            cli_path.write_text(
                "\n".join(
                    [
                        "#!/usr/bin/env python3",
                        "import json",
                        "import sys",
                        "",
                        "argv = sys.argv[1:]",
                        'if argv[:3] == ["config", "get", "active_backend"]:',
                        '    print(json.dumps({"status": "ok", "value": "openai-codex"}))',
                        'elif argv[:3] == ["config", "get", "backends.openai-codex.model"]:',
                        '    print(json.dumps({"status": "ok", "value": "gpt-5"}))',
                        'elif argv[:3] == ["config", "get", "fallback_backends"]:',
                        '    print(json.dumps({"status": "ok", "value": []}))',
                        'elif argv[:4] == ["auth", "openai-codex", "status", "--json"]:',
                        '    print(json.dumps({"status": "ok", "linked": True, "oauth_source": "test-harness", "account_id": "acct-test"}))',
                        "else:",
                        '    print(json.dumps({"status": "error", "argv": argv}))',
                        "    sys.exit(1)",
                        "",
                    ]
                ),
                encoding="utf-8",
            )
            cli_path.chmod(cli_path.stat().st_mode | stat.S_IXUSR)

            env = dict(os.environ)
            env["PATH"] = f"{bin_dir}{os.pathsep}{env.get('PATH', '')}"
            env["TIZENCLAW_CLI"] = str(cli_path)

            result = subprocess.run(
                [
                    sys.executable,
                    str(RUNNER),
                    "--skill-root",
                    str(skill_root),
                    "--output-dir",
                    str(output_dir),
                    "--scratch-root",
                    str(scratch_root),
                    "--suite",
                    "retry_task",
                    "--no-stream-runtime-io",
                ],
                cwd=ROOT,
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )
            if result.returncode != 0:
                self.fail(
                    "runner returned non-zero exit status\\n"
                    f"stdout:\\n{result.stdout}\\n"
                    f"stderr:\\n{result.stderr}"
                )

            output_files = sorted(output_dir.glob("*.json"))
            self.assertEqual(len(output_files), 1)

            payload = json.loads(output_files[0].read_text(encoding="utf-8"))
            task_entry = payload["tasks"][0]
            run_record = task_entry["runs"][0]
            retry_summary = task_entry["retry_summary"]
            efficiency = payload["efficiency"]

            self.assertEqual(retry_summary["execution_attempt_count"], 2)
            self.assertEqual(retry_summary["judge_attempt_count"], 2)
            self.assertEqual(retry_summary["failed_attempt_count"], 2)
            self.assertEqual(retry_summary["transient_retry_count"], 2)

            self.assertEqual(run_record["aggregate_usage"]["total_tokens"], 65)
            self.assertEqual(run_record["aggregate_usage"]["request_count"], 4)
            self.assertGreater(run_record["execution_time"], 0.45)
            self.assertTrue(run_record["execution_attempts"][0]["timed_out"])
            self.assertTrue(run_record["judge_attempts"][0]["timed_out"])
            self.assertTrue(run_record["usage_accounting"]["complete"])

            self.assertEqual(efficiency["total_tokens"], 65)
            self.assertEqual(efficiency["total_requests"], 4)
            self.assertGreater(efficiency["total_execution_time_seconds"], 0.45)
            self.assertEqual(efficiency["per_task"][0]["failed_attempt_count"], 2)
            self.assertTrue(efficiency["usage_accounting_complete"])
            self.assertEqual(efficiency["usage_accounting_incomplete_attempt_count"], 0)


if __name__ == "__main__":
    unittest.main()
