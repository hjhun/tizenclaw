#!/usr/bin/env python3
"""Overwrite .dev/SCORE.md from a PinchBench result JSON file."""

from __future__ import annotations

import argparse
import json
import re
from datetime import datetime, timezone
from pathlib import Path


TARGET_PASS_RATE = 95.0
STAGE_ORDER = [
    ("Planning", "1. Planning"),
    ("Design", "2. Design"),
    ("Development", "3. Development"),
    ("Build/Deploy", "4. Build/Deploy"),
    ("Test/Review", "5. Test/Review"),
    ("Commit", "6. Commit"),
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("result_json", type=Path)
    parser.add_argument(
        "--output",
        type=Path,
        default=Path(".dev/SCORE.md"),
    )
    parser.add_argument(
        "--dashboard",
        type=Path,
        default=Path(".dev/DASHBOARD.md"),
        help="Dashboard Markdown file that carries stage verdict evidence.",
    )
    parser.add_argument(
        "--commit-sha",
        default="",
        help="Commit SHA to record for Stage 6.",
    )
    return parser.parse_args()


def task_score(task: dict) -> float:
    grading = task.get("grading") or {}
    mean = grading.get("mean")
    if isinstance(mean, (int, float)):
        return float(mean)
    runs = grading.get("runs") or []
    if runs:
        score = runs[0].get("score")
        if isinstance(score, (int, float)):
            return float(score)
    return 0.0


def parse_stage_verdicts(dashboard_path: Path, commit_sha: str) -> list[tuple[str, str]]:
    verdicts = {name: "NOT RECORDED" for name, _ in STAGE_ORDER}
    commit_default = "PASS" if commit_sha else "NOT STARTED"
    verdicts["Commit"] = commit_default

    if not dashboard_path.exists():
        return [(numbered_name, verdicts[name]) for name, numbered_name in STAGE_ORDER]

    text = dashboard_path.read_text(encoding="utf-8")
    match = re.search(
        r"^## Stage Verdicts\s*$\n(?P<body>.*?)(?=^## |\Z)",
        text,
        flags=re.MULTILINE | re.DOTALL,
    )
    if not match:
        return [(numbered_name, verdicts[name]) for name, numbered_name in STAGE_ORDER]

    for raw_line in match.group("body").splitlines():
        line = raw_line.strip()
        if not line.startswith("- "):
            continue
        label, separator, verdict = line[2:].partition(":")
        if not separator:
            continue
        stage_name = label.strip()
        if stage_name not in verdicts:
            continue
        parsed_verdict = verdict.strip()
        if parsed_verdict:
            verdicts[stage_name] = parsed_verdict

    if commit_sha:
        verdicts["Commit"] = "PASS"

    return [(numbered_name, verdicts[name]) for name, numbered_name in STAGE_ORDER]


def build_ledger(result_path: Path, payload: dict, commit_sha: str, dashboard_path: Path) -> str:
    tasks = payload.get("tasks") or []
    summary = payload.get("summary") or {}
    total_score = float(summary.get("total_score") or sum(task_score(task) for task in tasks))
    total_possible = float(summary.get("max_score") or float(len(tasks) or 1))
    pass_rate = float(summary.get("pass_rate") or ((total_score / total_possible) * 100.0))
    efficiency = payload.get("efficiency") or {}
    token_usage = int(
        efficiency.get("total_tokens")
        or sum(int((task.get("usage") or {}).get("total_tokens") or 0) for task in tasks)
    )
    api_requests = int(
        efficiency.get("total_requests")
        or sum(int((task.get("usage") or {}).get("request_count") or 0) for task in tasks)
    )
    execution_time = float(
        efficiency.get("total_execution_time_seconds")
        or sum(float(task.get("execution_time") or 0.0) for task in tasks)
    )
    status = "MET" if pass_rate >= TARGET_PASS_RATE else "NOT MET"
    timestamp = payload.get("timestamp")
    if isinstance(timestamp, (int, float)):
        timestamp_text = datetime.fromtimestamp(timestamp, tz=timezone.utc).strftime(
            "%Y-%m-%d %H:%M:%S %z"
        )
    elif isinstance(timestamp, str) and timestamp:
        timestamp_text = timestamp
    else:
        timestamp_text = "unknown"

    run_id = payload.get("run_id", result_path.stem)
    runtime = payload.get("runtime", "unknown")
    model = payload.get("model", "unknown")
    suite = payload.get("suite", "unknown")
    execution_mode = payload.get("execution_mode") or {}
    stage_results = parse_stage_verdicts(dashboard_path, commit_sha)

    lines = [
        "# SCORE",
        "",
        f"- Run ID: `{run_id}`",
        f"- Runtime: `{runtime}`",
        f"- Model: `{model}`",
        f"- Timestamp: `{timestamp_text}`",
        f"- Suite: `{suite}`",
        f"- Final Score: `{pass_rate:.1f}%` (`{total_score:.2f} / {total_possible:.2f}`)",
        f"- Total Tokens: `{token_usage}`",
        f"- Total Requests: `{api_requests}`",
        f"- Status: `{status}`",
    ]

    if execution_mode:
        lines.extend(
            [
                f"- Auth Mode: `{execution_mode.get('auth_mode', 'unknown')}`",
                f"- OAuth Source: `{execution_mode.get('oauth_source', 'unknown')}`",
                f"- Model Injection: `{'disabled' if not execution_mode.get('model_injection', True) else 'enabled'}`",
                f"- Judge Mode: `{execution_mode.get('judge_mode', 'unknown')}`",
                f"- Config Unchanged During Run: `{execution_mode.get('config_unchanged', False)}`",
            ]
        )

    lines.extend(["", "## Stage Results", ""])
    for stage, verdict in stage_results:
        lines.append(f"{stage}: {verdict}")
    if commit_sha:
        lines.append(f"- Commit SHA: `{commit_sha}`")

    lines.extend(
        [
            "",
            "## Task Scores",
            "",
        ]
    )
    for task in tasks:
        score = task_score(task)
        task_id = task.get("task_id", "unknown_task")
        lines.append(f"- `{task_id}`: `{score:.4f}`")

    lines.extend(
        [
            "",
            "## Summary",
            "",
            f"- Total score: `{total_score:.4f} / {total_possible:.1f}`",
            f"- Pass rate: `{pass_rate:.1f}%`",
            f"- Token usage: `{token_usage}`",
            f"- API requests: `{api_requests}`",
            f"- Execution time: `{execution_time:.2f}s`",
            f"- Result JSON: `{result_path}`",
        ]
    )

    return "\n".join(lines) + "\n"


def main() -> int:
    args = parse_args()
    payload = json.loads(args.result_json.read_text(encoding="utf-8"))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        build_ledger(args.result_json, payload, args.commit_sha, args.dashboard),
        encoding="utf-8",
    )
    print(args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
