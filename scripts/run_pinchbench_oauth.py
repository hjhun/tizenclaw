#!/usr/bin/env python3
"""Run PinchBench on TizenClaw using the active OpenAI OAuth config."""

from __future__ import annotations

import argparse
import json
import logging
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_PINCHBENCH_SKILL_ROOT = Path("/home/hjhun/samba/github/pinchbench/skill")
DEFAULT_SCRATCH_ROOT = REPO_ROOT / ".tmp" / "pinchbench_oauth"
DEFAULT_RESULTS_DIR = DEFAULT_SCRATCH_ROOT / "results"
TARGET_PASS_RATE = 95.0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run PinchBench with TizenClaw's active OpenAI OAuth backend.",
    )
    parser.add_argument(
        "--skill-root",
        type=Path,
        default=DEFAULT_PINCHBENCH_SKILL_ROOT,
        help="Path to the PinchBench skill repository.",
    )
    parser.add_argument(
        "--suite",
        default="all",
        help='Tasks to run: "all", "automated-only", or comma-separated task IDs.',
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=DEFAULT_RESULTS_DIR,
        help="Directory for the aggregate results JSON.",
    )
    parser.add_argument(
        "--scratch-root",
        type=Path,
        default=DEFAULT_SCRATCH_ROOT,
        help="Scratch directory for temporary task workspaces.",
    )
    parser.add_argument(
        "--runs",
        type=int,
        default=1,
        help="Number of runs per task for averaging.",
    )
    parser.add_argument(
        "--timeout-multiplier",
        type=float,
        default=1.0,
        help="Scale all task timeouts.",
    )
    parser.add_argument(
        "--judge-timeout-seconds",
        type=float,
        default=180.0,
        help="Timeout for the LLM judge prompt.",
    )
    parser.add_argument(
        "--verbose",
        "-v",
        action="store_true",
        help="Enable verbose logging.",
    )
    parser.add_argument(
        "--stream-runtime-io",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="Echo captured runtime stdout/stderr after each prompt.",
    )
    return parser.parse_args()


def configure_logging() -> logging.Logger:
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s - %(levelname)s - %(message)s",
        handlers=[logging.StreamHandler(sys.stdout)],
    )
    return logging.getLogger("pinchbench_oauth")


def cleanup_benchmark_artifacts(
    scratch_root: Path,
    output_dir: Path,
    logger: logging.Logger,
) -> dict[str, Any]:
    removed_paths: list[str] = []

    if scratch_root.exists():
        for child in sorted(scratch_root.iterdir()):
            if child == output_dir:
                continue
            if child.is_dir():
                shutil.rmtree(child)
                removed_paths.append(str(child))
            elif child.is_file() and child.suffix in {".json", ".log"}:
                child.unlink()
                removed_paths.append(str(child))

    if output_dir.exists():
        for child in sorted(output_dir.iterdir()):
            if child.is_file() and child.suffix == ".json":
                child.unlink()
                removed_paths.append(str(child))

    logger.info(
        "Cleanup removed %d stale PinchBench artifact(s) from %s and %s",
        len(removed_paths),
        scratch_root,
        output_dir,
    )
    for path in removed_paths:
        logger.info("Cleanup removed: %s", path)

    return {
        "scratch_root": str(scratch_root),
        "output_dir": str(output_dir),
        "removed_paths": removed_paths,
        "removed_count": len(removed_paths),
    }


def load_pinchbench_modules(skill_root: Path):
    scripts_dir = skill_root / "scripts"
    if not scripts_dir.exists():
        raise FileNotFoundError(f"PinchBench scripts directory not found: {scripts_dir}")
    sys.path.insert(0, str(scripts_dir))
    import lib_agent  # type: ignore
    import lib_grading  # type: ignore
    from lib_tasks import Task, TaskLoader  # type: ignore

    return lib_agent, lib_grading, Task, TaskLoader


def run_json_command(cmd: list[str]) -> dict[str, Any]:
    result = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        check=False,
        cwd=str(REPO_ROOT),
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"Command failed: {' '.join(cmd)}: {result.stderr.strip() or result.stdout.strip()}"
        )
    payload = json.loads(result.stdout or "{}")
    if not isinstance(payload, dict):
        raise RuntimeError(f"Unexpected JSON payload from {' '.join(cmd)}")
    return payload


def read_config_value(path: str) -> Any:
    payload = run_json_command(["tizenclaw-cli", "config", "get", path])
    if payload.get("status") != "ok":
        raise RuntimeError(f"Failed to read config path {path}: {payload}")
    return payload.get("value")


def read_active_runtime_snapshot() -> dict[str, Any]:
    active_backend = read_config_value("active_backend")
    if not isinstance(active_backend, str) or not active_backend:
        raise RuntimeError("TizenClaw active_backend is not configured")
    if active_backend != "openai-codex":
        raise RuntimeError(
            f"Active backend must be openai-codex for this run, found {active_backend}"
        )

    model_name = read_config_value(f"backends.{active_backend}.model")
    fallback_backends = read_config_value("fallback_backends")
    auth_status = run_json_command(["tizenclaw-cli", "auth", "openai-codex", "status", "--json"])
    if auth_status.get("status") != "ok" or not auth_status.get("linked"):
        raise RuntimeError(
            "OpenAI Codex OAuth is not linked; run `tizenclaw-cli auth openai-codex login` first"
        )

    return {
        "active_backend": active_backend,
        "configured_model": model_name,
        "fallback_backends": fallback_backends,
        "auth_mode": "oauth",
        "oauth_source": auth_status.get("oauth_source", ""),
        "account_id": auth_status.get("account_id", ""),
    }


def select_task_ids(tasks: list[Any], suite: str) -> list[str] | None:
    if suite == "all":
        return None
    if suite == "automated-only":
        return [task.task_id for task in tasks if task.grading_type == "automated"]
    return [task_id.strip() for task_id in suite.split(",") if task_id.strip()]


def next_run_id(run_root: Path) -> str:
    run_root.mkdir(parents=True, exist_ok=True)
    existing_ids: list[int] = []
    for entry in run_root.iterdir():
        if entry.is_dir() and entry.name.isdigit():
            existing_ids.append(int(entry.name))
    return f"{(max(existing_ids) + 1) if existing_ids else 1:04d}"


def git_short_rev(path: Path) -> str:
    result = subprocess.run(
        ["git", "rev-parse", "--short", "HEAD"],
        capture_output=True,
        text=True,
        check=False,
        cwd=str(path),
    )
    if result.returncode != 0:
        return ""
    return result.stdout.strip()


def copy_runtime_skills(lib_agent: Any, runtime: str, dest_workspace: Path, logger: logging.Logger) -> None:
    skills_dir = lib_agent._get_runtime_skills_dir(runtime)
    if not skills_dir.exists():
        return
    dest_skills_dir = dest_workspace / "skills"
    dest_skills_dir.mkdir(parents=True, exist_ok=True)
    for skill_dir_src in skills_dir.iterdir():
        if not skill_dir_src.is_dir():
            continue
        dest_skill_dir = dest_skills_dir / skill_dir_src.name
        if dest_skill_dir.exists():
            shutil.rmtree(dest_skill_dir)
        shutil.copytree(skill_dir_src, dest_skill_dir)
        logger.info("Copied skill to benchmark workspace: %s", skill_dir_src.name)


def prepare_task_workspace_local(
    *,
    lib_agent: Any,
    skill_root: Path,
    scratch_root: Path,
    run_id: str,
    task: Any,
    runtime: str,
    logger: logging.Logger,
) -> Path:
    workspace = scratch_root / run_id / task.task_id
    if workspace.exists():
        shutil.rmtree(workspace)
    workspace.mkdir(parents=True, exist_ok=True)

    for file_spec in task.workspace_files:
        if "content" in file_spec:
            dest = workspace / file_spec["path"]
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_text(file_spec["content"], encoding="utf-8")
            continue

        source = skill_root / "assets" / file_spec["source"]
        dest = workspace / file_spec["dest"]
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(source.read_bytes())

    for bootstrap_file in ("BOOTSTRAP.md", "SOUL.md", "USER.md", "IDENTITY.md"):
        bootstrap_path = workspace / bootstrap_file
        if bootstrap_path.exists():
            bootstrap_path.unlink()
            logger.info("Removed bootstrap file: %s", bootstrap_file)

    copy_runtime_skills(lib_agent, runtime, workspace, logger)
    return workspace


def stream_runtime_io(stdout: str, stderr: str) -> None:
    if stdout:
        print(stdout, end="" if stdout.endswith("\n") else "\n")
    if stderr:
        print(stderr, end="" if stderr.endswith("\n") else "\n", file=sys.stderr)


def execute_tizenclaw_task_active_config(
    *,
    lib_agent: Any,
    task: Any,
    agent_id: str,
    run_id: str,
    timeout_multiplier: float,
    skill_root: Path,
    scratch_root: Path,
    verbose: bool,
    stream_io: bool,
    logger: logging.Logger,
) -> dict[str, Any]:
    logger.info("🤖 Agent [%s] starting task: %s", agent_id, task.task_id)
    logger.info("   Task: %s", task.name)
    logger.info("   Category: %s", task.category)
    if verbose:
        prompt_preview = task.prompt[:500] + "..." if len(task.prompt) > 500 else task.prompt
        logger.info("   Prompt: %s", prompt_preview)

    start_time = time.time()
    timeout_seconds = task.timeout_seconds * timeout_multiplier
    stdout = ""
    stderr = ""
    exit_code = -1
    timed_out = False
    transcript: list[dict[str, Any]] = []
    usage = {
        "input_tokens": 0,
        "output_tokens": 0,
        "cache_read_tokens": 0,
        "cache_write_tokens": 0,
        "total_tokens": 0,
        "cost_usd": 0.0,
        "request_count": 0,
    }

    sessions = task.frontmatter.get("sessions", [])
    session_entries = sessions if sessions else [task.prompt]
    current_session_id: str | None = None
    current_workspace: Path | None = None
    seed_workspace: Path | None = None

    for index, session_entry in enumerate(session_entries, 1):
        if isinstance(session_entry, str):
            session_prompt = session_entry
            new_session = False
        elif isinstance(session_entry, dict):
            session_prompt = session_entry.get("prompt") or session_entry.get("message", "")
            new_session = bool(session_entry.get("new_session"))
        else:
            logger.warning("Skipping invalid session entry for %s: %r", task.task_id, session_entry)
            continue

        if not session_prompt:
            continue

        if current_session_id is None or new_session:
            next_session_id = f"{task.task_id}_{int(time.time() * 1000)}_{index}"
            lib_agent._cleanup_tizenclaw_session(next_session_id)
            next_workspace = lib_agent._tizenclaw_workdir(next_session_id)

            if seed_workspace is None:
                seed_workspace = prepare_task_workspace_local(
                    lib_agent=lib_agent,
                    skill_root=skill_root,
                    scratch_root=scratch_root,
                    run_id=run_id,
                    task=task,
                    runtime="tizenclaw",
                    logger=logger,
                )
                source_workspace = seed_workspace
            else:
                source_workspace = current_workspace or seed_workspace

            if next_workspace.exists():
                shutil.rmtree(next_workspace)
            shutil.copytree(source_workspace, next_workspace)
            current_session_id = next_session_id
            current_workspace = next_workspace

        logger.info("   Session %d/%d", index, len(session_entries))
        elapsed = time.time() - start_time
        remaining = timeout_seconds - elapsed
        if remaining <= 0:
            timed_out = True
            break

        assert current_session_id is not None
        assert current_workspace is not None

        try:
            baseline = lib_agent._read_tizenclaw_usage(current_session_id)
            transcript_start_index = len(lib_agent._load_tizenclaw_transcript(current_session_id))
            result = lib_agent._run_tizenclaw_message(
                session_id=current_session_id,
                prompt=session_prompt,
                workspace=current_workspace,
                timeout_seconds=remaining,
            )
            if stream_io:
                stream_runtime_io(result.stdout, result.stderr)
            stdout += result.stdout
            stderr += result.stderr
            exit_code = result.returncode

            transcript.extend(
                lib_agent._wait_for_tizenclaw_transcript_slice(
                    current_session_id,
                    transcript_start_index,
                )
            )
            delta_usage = lib_agent._read_tizenclaw_usage(current_session_id, baseline)
            for key in usage:
                usage[key] += delta_usage.get(key, 0)

            if result.returncode != 0:
                break
        except subprocess.TimeoutExpired as exc:
            timed_out = True
            stdout_chunk = lib_agent._coerce_subprocess_output(exc.stdout)
            stderr_chunk = lib_agent._coerce_subprocess_output(exc.stderr)
            if stream_io:
                stream_runtime_io(stdout_chunk, stderr_chunk)
            stdout += stdout_chunk
            stderr += stderr_chunk
            break
        except (FileNotFoundError, RuntimeError, ValueError, json.JSONDecodeError) as exc:
            stderr += f"tizenclaw runtime error: {exc}"
            break

    execution_time = time.time() - start_time
    workspace_str = str(current_workspace) if current_workspace is not None else ""

    status = "success"
    if timed_out:
        status = "timeout"
    if exit_code != 0 and not timed_out:
        status = "error"
    if not lib_agent._transcript_has_agent_activity(transcript):
        status = "error"
    if "tizenclaw runtime error:" in stderr:
        status = "error"

    return {
        "agent_id": agent_id,
        "task_id": task.task_id,
        "status": status,
        "transcript": transcript,
        "usage": usage,
        "workspace": workspace_str,
        "exit_code": exit_code,
        "timed_out": timed_out,
        "execution_time": execution_time,
        "stdout": stdout,
        "stderr": stderr,
    }


def run_tizenclaw_judge_active_config(
    *,
    lib_agent: Any,
    prompt: str,
    workspace: Path,
    timeout_seconds: float,
    stream_io: bool,
) -> dict[str, Any]:
    start_time = time.time()
    session_id = f"judge_{int(time.time() * 1000)}"
    lib_agent._cleanup_tizenclaw_session(session_id)
    actual_workspace = lib_agent._tizenclaw_workdir(session_id)
    actual_workspace.mkdir(parents=True, exist_ok=True)

    if workspace.exists():
        for item in workspace.iterdir():
            target = actual_workspace / item.name
            if item.is_dir():
                shutil.copytree(item, target, dirs_exist_ok=True)
            else:
                shutil.copy2(item, target)

    stdout = ""
    stderr = ""
    exit_code = -1
    timed_out = False

    try:
        transcript_start_index = len(lib_agent._load_tizenclaw_transcript(session_id))
        result = lib_agent._run_tizenclaw_message(
            session_id=session_id,
            prompt=prompt,
            workspace=actual_workspace,
            timeout_seconds=timeout_seconds,
        )
        if stream_io:
            stream_runtime_io(result.stdout, result.stderr)
        stdout = result.stdout
        stderr = result.stderr
        exit_code = result.returncode
        transcript = lib_agent._wait_for_tizenclaw_transcript_slice(
            session_id,
            transcript_start_index,
        )
    except subprocess.TimeoutExpired as exc:
        timed_out = True
        stdout = lib_agent._coerce_subprocess_output(exc.stdout)
        stderr = lib_agent._coerce_subprocess_output(exc.stderr)
        if stream_io:
            stream_runtime_io(stdout, stderr)
        transcript = lib_agent._load_tizenclaw_transcript(session_id)
    except (FileNotFoundError, RuntimeError, ValueError, json.JSONDecodeError) as exc:
        stderr = f"tizenclaw runtime error: {exc}"
        transcript = lib_agent._load_tizenclaw_transcript(session_id)

    execution_time = time.time() - start_time
    status = "success"
    if timed_out:
        status = "timeout"
    if exit_code != 0 and not timed_out:
        status = "error"
    if not lib_agent._transcript_has_agent_activity(transcript):
        status = "error"
    if "tizenclaw runtime error:" in stderr:
        status = "error"

    return {
        "agent_id": "tizenclaw-judge",
        "status": status,
        "transcript": transcript,
        "workspace": str(actual_workspace),
        "exit_code": exit_code,
        "timed_out": timed_out,
        "execution_time": execution_time,
        "stdout": stdout,
        "stderr": stderr,
    }


def grade_task_active_config(
    *,
    lib_grading: Any,
    lib_agent: Any,
    task: Any,
    execution_result: dict[str, Any],
    judge_timeout_seconds: float,
    scratch_root: Path,
    stream_io: bool,
    verbose: bool,
) -> Any:
    if task.grading_type == "automated":
        return lib_grading._grade_automated(task, execution_result, verbose=verbose)

    def llm_grade() -> Any:
        transcript_summary = lib_grading._summarize_transcript(execution_result.get("transcript", []))
        rubric = task.llm_judge_rubric or lib_grading._format_grading_criteria(task)
        prompt = lib_grading._build_judge_prompt(task, transcript_summary, rubric)
        judge_workspace = scratch_root / "judge" / task.task_id
        judge_workspace.mkdir(parents=True, exist_ok=True)
        judge_result = run_tizenclaw_judge_active_config(
            lib_agent=lib_agent,
            prompt=prompt,
            workspace=judge_workspace,
            timeout_seconds=judge_timeout_seconds,
            stream_io=stream_io,
        )
        raw_parsed = lib_grading._parse_judge_response(judge_result.get("transcript", []))
        parsed = lib_grading._normalize_judge_response(raw_parsed)
        breakdown = parsed.get("scores", {})
        total = parsed.get("total")
        notes = parsed.get("notes", "")
        return lib_grading.GradeResult(
            task_id=task.task_id,
            score=float(total) if total is not None else 0.0,
            max_score=1.0,
            grading_type="llm_judge",
            breakdown=lib_grading._normalize_score_dict(breakdown),
            notes=str(notes) if notes is not None else "",
        )

    if task.grading_type == "llm_judge":
        return llm_grade()

    auto_result = lib_grading._grade_automated(task, execution_result, verbose=verbose)
    llm_result = llm_grade()
    return lib_grading._combine_grades(task, auto_result, llm_result)


def compute_efficiency_summary(task_entries: list[dict[str, Any]], grades_by_task_id: dict[str, Any]) -> dict[str, Any]:
    total_input_tokens = 0
    total_output_tokens = 0
    total_tokens = 0
    total_cost_usd = 0.0
    total_requests = 0
    total_execution_time = 0.0
    tasks_with_usage = 0
    per_task_efficiency: list[dict[str, Any]] = []

    for entry in task_entries:
        usage = entry.get("usage", {})
        task_id = entry["task_id"]
        grading = grades_by_task_id.get(task_id, {})
        score = float(grading.get("mean", 0.0))
        input_tokens = int(usage.get("input_tokens", 0))
        output_tokens = int(usage.get("output_tokens", 0))
        total = int(usage.get("total_tokens", 0))
        requests = int(usage.get("request_count", 0))
        exec_time = float(entry.get("execution_time", 0.0) or 0.0)

        total_input_tokens += input_tokens
        total_output_tokens += output_tokens
        total_tokens += total
        total_requests += requests
        total_execution_time += exec_time
        if total > 0:
            tasks_with_usage += 1

        per_task_efficiency.append(
            {
                "task_id": task_id,
                "score": round(score, 4),
                "total_tokens": total,
                "cost_usd": round(float(usage.get("cost_usd", 0.0) or 0.0), 6),
                "tokens_per_score_point": round(total / score, 1) if score > 0 else None,
            }
        )

    all_scores = [float(grading.get("mean", 0.0)) for grading in grades_by_task_id.values()]
    total_score = sum(all_scores)
    num_tasks = len(all_scores)
    return {
        "total_tokens": total_tokens,
        "total_input_tokens": total_input_tokens,
        "total_output_tokens": total_output_tokens,
        "total_cost_usd": round(total_cost_usd, 6),
        "total_requests": total_requests,
        "total_execution_time_seconds": round(total_execution_time, 2),
        "tasks_with_usage_data": tasks_with_usage,
        "tokens_per_task": round(total_tokens / num_tasks, 1) if num_tasks > 0 else 0,
        "score_per_1k_tokens": (
            round(total_score / (total_tokens / 1000), 6) if total_tokens > 0 else None
        ),
        "per_task": per_task_efficiency,
    }


def main() -> int:
    args = parse_args()
    logger = configure_logging()

    skill_root = args.skill_root.resolve()
    if not skill_root.exists():
        raise FileNotFoundError(f"PinchBench skill root not found: {skill_root}")

    lib_agent, lib_grading, _, TaskLoader = load_pinchbench_modules(skill_root)

    runtime_snapshot_before = read_active_runtime_snapshot()
    model_label = (
        f"{runtime_snapshot_before['active_backend']}/"
        f"{runtime_snapshot_before['configured_model']}"
    )
    logger.info(
        "Running PinchBench with active config: backend=%s model=%s oauth_source=%s",
        runtime_snapshot_before["active_backend"],
        runtime_snapshot_before["configured_model"],
        runtime_snapshot_before["oauth_source"],
    )

    tasks_dir = skill_root / "tasks"
    task_loader = TaskLoader(tasks_dir)
    tasks = task_loader.load_all_tasks()
    selected_ids = select_task_ids(tasks, args.suite)
    tasks_to_run = tasks if selected_ids is None else [task for task in tasks if task.task_id in selected_ids]
    task_map = {task.task_id: task for task in tasks_to_run}

    scratch_root = args.scratch_root.resolve()
    output_dir = args.output_dir.resolve()
    cleanup_summary = cleanup_benchmark_artifacts(scratch_root, output_dir, logger)
    run_id = next_run_id(output_dir)
    agent_id = "bench-tizenclaw-active-oauth"
    runs_per_task = max(1, args.runs)

    task_results: list[dict[str, Any]] = []
    grades_by_task_id: dict[str, Any] = {}

    logger.info("Loaded %d task(s) for suite=%s", len(tasks_to_run), args.suite)

    for index, task in enumerate(tasks_to_run, 1):
        grades = []
        run_results = []
        for run_index in range(runs_per_task):
            logger.info("%s", "=" * 80)
            logger.info(
                "Task %d/%d (%s) run %d/%d",
                index,
                len(tasks_to_run),
                task.task_id,
                run_index + 1,
                runs_per_task,
            )
            logger.info("%s", "=" * 80)

            result = execute_tizenclaw_task_active_config(
                lib_agent=lib_agent,
                task=task,
                agent_id=agent_id,
                run_id=f"{run_id}-{run_index + 1}",
                timeout_multiplier=args.timeout_multiplier,
                skill_root=skill_root,
                scratch_root=scratch_root,
                verbose=args.verbose,
                stream_io=args.stream_runtime_io,
                logger=logger,
            )
            grade = grade_task_active_config(
                lib_grading=lib_grading,
                lib_agent=lib_agent,
                task=task,
                execution_result=result,
                judge_timeout_seconds=args.judge_timeout_seconds,
                scratch_root=scratch_root,
                stream_io=args.stream_runtime_io,
                verbose=args.verbose,
            )

            grades.append(grade)
            run_results.append(result)
            task_results.append(result)

            score_pct = grade.score / grade.max_score * 100 if grade.max_score > 0 else 0.0
            logger.info(
                "Task %s scored %.4f/%.1f (%.1f%%)",
                task.task_id,
                grade.score,
                grade.max_score,
                score_pct,
            )
            if grade.notes:
                logger.info("Notes: %s", grade.notes[:200])

        scores = [grade.score for grade in grades]
        grades_by_task_id[task.task_id] = {
            "runs": [grade.to_dict() for grade in grades],
            "mean": statistics.mean(scores),
            "std": statistics.stdev(scores) if len(scores) > 1 else 0.0,
            "min": min(scores),
            "max": max(scores),
        }

    task_entries = [
        {
            "task_id": result["task_id"],
            "status": result["status"],
            "timed_out": result["timed_out"],
            "execution_time": result["execution_time"],
            "transcript_length": len(result["transcript"]),
            "usage": result.get("usage", {}),
            "workspace": result["workspace"],
            "grading": grades_by_task_id[result["task_id"]],
            "frontmatter": task_map[result["task_id"]].frontmatter,
        }
        for result in task_results
    ]

    runtime_snapshot_after = read_active_runtime_snapshot()
    config_unchanged = runtime_snapshot_before == runtime_snapshot_after
    efficiency = compute_efficiency_summary(task_entries, grades_by_task_id)
    total_score = sum(grades_by_task_id[task_id]["mean"] for task_id in grades_by_task_id)
    max_score = float(len(grades_by_task_id) or 1)
    pass_rate = (total_score / max_score) * 100.0

    aggregate = {
        "runtime": "tizenclaw",
        "model": model_label,
        "benchmark_version": git_short_rev(skill_root),
        "run_id": run_id,
        "timestamp": time.time(),
        "suite": args.suite,
        "runs_per_task": runs_per_task,
        "execution_mode": {
            **runtime_snapshot_after,
            "model_injection": False,
            "judge_mode": "active_config",
            "config_unchanged": config_unchanged,
        },
        "cleanup": cleanup_summary,
        "tasks": task_entries,
        "efficiency": efficiency,
        "summary": {
            "total_score": total_score,
            "max_score": max_score,
            "pass_rate": pass_rate,
            "target_pass_rate": TARGET_PASS_RATE,
        },
    }

    output_dir.mkdir(parents=True, exist_ok=True)
    output_path = output_dir / f"{run_id}_tizenclaw_active-oauth.json"
    output_path.write_text(json.dumps(aggregate, indent=2), encoding="utf-8")

    logger.info("%s", "=" * 80)
    logger.info("Final score: %.2f / %.2f (%.1f%%)", total_score, max_score, pass_rate)
    logger.info("Token usage: %s", efficiency["total_tokens"])
    logger.info("Request count: %s", efficiency["total_requests"])
    logger.info("Config unchanged during run: %s", config_unchanged)
    logger.info("Results written to %s", output_path)
    logger.info("%s", "=" * 80)

    return 0 if pass_rate >= TARGET_PASS_RATE else 1


if __name__ == "__main__":
    raise SystemExit(main())
