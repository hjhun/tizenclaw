# TizenClaw PinchBench Report

## Execution Context

- Cycle: `host-default`
- Benchmark date: `2026-04-14 05:56:03 KST`
- Runtime: `tizenclaw`
- Model: `openai-codex/gpt-5.4`
- Auth mode: `oauth` via `codex_cli`
- Suite: `all` (`25` tasks, `1` run per task)
- Benchmark command:
  `python3 scripts/run_pinchbench_oauth.py --suite all --runs 1 --no-stream-runtime-io`
- Build/deploy command: `./deploy_host.sh`
- Regression command: `./deploy_host.sh --test`

## Summary

- Final score: `22.8943 / 25.0`
- Pass rate: `91.58%`
- Target pass rate: `95.0%`
- Benchmark verdict: `NOT MET`
- Perfect-score tasks: `10`
- Tasks scoring `>= 0.95`: `13`
- Tasks scoring `< 0.90`: `4`
- Config unchanged during run: `true`

TizenClaw completed the full PinchBench suite without timeouts and kept the
configured `openai-codex` backend unchanged for the entire run. The aggregate
score stayed below the target mainly because `task_22_second_brain` collapsed
to `0.0250`, with additional drag from `task_24_polymarket_briefing`,
`task_03_blog`, and `task_16_email_triage`.

## Task Results

| Task | Score | Exec Time (s) |
| --- | ---: | ---: |
| `task_00_sanity` | `1.0000` | `2.28` |
| `task_01_calendar` | `1.0000` | `0.03` |
| `task_02_stock` | `1.0000` | `196.69` |
| `task_03_blog` | `0.8500` | `66.19` |
| `task_04_weather` | `1.0000` | `11.03` |
| `task_05_summary` | `0.9800` | `14.64` |
| `task_06_events` | `0.9300` | `0.11` |
| `task_07_email` | `0.9100` | `7.60` |
| `task_08_memory` | `1.0000` | `17.16` |
| `task_09_files` | `1.0000` | `7.40` |
| `task_10_workflow` | `0.9375` | `28.43` |
| `task_11_clawdhub` | `1.0000` | `13.86` |
| `task_12_skill_search` | `1.0000` | `30.49` |
| `task_13_image_gen` | `0.9417` | `16.41` |
| `task_14_humanizer` | `0.9300` | `13.99` |
| `task_15_daily_summary` | `0.9500` | `0.08` |
| `task_16_email_triage` | `0.8976` | `0.17` |
| `task_17_email_search` | `0.9300` | `0.13` |
| `task_16_market_research` | `0.9150` | `148.94` |
| `task_18_spreadsheet_summary` | `0.9600` | `23.09` |
| `task_20_eli5_pdf_summary` | `0.9125` | `2.61` |
| `task_21_openclaw_comprehension` | `1.0000` | `19.22` |
| `task_22_second_brain` | `0.0250` | `46.43` |
| `task_24_polymarket_briefing` | `0.8250` | `24.63` |
| `task_25_access_log_anomaly` | `1.0000` | `35.29` |

## Efficiency

- Total tokens: `966,707`
- Input tokens: `889,654`
- Output tokens: `18,301`
- Requests: `87`
- Total execution time: `726.87s` (`12m 06.87s`)
- Score per 1k tokens: `0.023683`
- Median task score: `0.9500`

Longest tasks:

| Task | Score | Time (s) |
| --- | ---: | ---: |
| `task_02_stock` | `1.0000` | `196.69` |
| `task_16_market_research` | `0.9150` | `148.94` |
| `task_03_blog` | `0.8500` | `66.19` |
| `task_22_second_brain` | `0.0250` | `46.43` |
| `task_25_access_log_anomaly` | `1.0000` | `35.29` |

Observed category strengths:

- `file_ops`: `1.0000` average across `3` tasks
- `comprehension`: `0.9556` average across `4` tasks
- `writing`: `0.8800` average across `2` tasks
- `memory`: `0.0250` average across `1` task

## Failures And Notes

Primary score losses:

- `task_22_second_brain` (`0.0250`): memory save and cross-session recall
  both failed, so the persistence path is the clearest benchmark weakness.
- `task_24_polymarket_briefing` (`0.8250`): top-market selection quality and
  news relevance were weak, with at least one mismatched or fabricated item.
- `task_03_blog` (`0.8500`): content quality was strong, but the run lost
  points for unnecessary extra file writes and read-back steps.
- `task_16_email_triage` (`0.8976`): prioritization was mostly good, but
  completeness and ordering still left measurable grading loss.

Common deductions in the `0.91` to `0.94` band came from:

- unnecessary verification or read-back tool usage
- over-generation beyond the exact file/output asked for
- date specificity issues in research-style tasks
- weaker instruction compliance on narrow formatting constraints

Host validation evidence:

- `./deploy_host.sh` completed successfully and reported IPC readiness.
- Host logs repeatedly reached `Daemon ready` before shutdown during the
  scripted benchmark/test cycle.
- `./deploy_host.sh --test` finished with overall success for the canonical
  Rust workspace, parity harness, and documentation verification.

Regression warning from `./deploy_host.sh --test`:

- The initial workspace test pass emitted compile errors in
  `src/tizenclaw/src/core/agent_core.rs` for missing functions
  `recent_news_selection_score`, `format_prediction_market_related_news`, and
  `extract_specific_calendar_dates`.
- The script continued and the canonical Rust workspace tests passed, so this
  report records the warning but does not treat it as a benchmark-run blocker.

## Artifacts

- Aggregate JSON:
  [0001_tizenclaw_active-oauth.json](/home/hjhun/samba/github/tizenclaw/.tmp/pinchbench_oauth/results/0001_tizenclaw_active-oauth.json)
- Scratch root:
  [pinchbench_oauth](/home/hjhun/samba/github/tizenclaw/.tmp/pinchbench_oauth)
- Host log:
  [tizenclaw.log](/home/hjhun/.tizenclaw/logs/tizenclaw.log)
