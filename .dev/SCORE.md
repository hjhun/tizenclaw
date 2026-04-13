# PinchBench Score

- Timestamp: `2026-04-14 05:08:41`
- Run ID: `0155`
- Cycle: `host-default`
- Runtime: `tizenclaw`
- Result file:
  `/home/hjhun/samba/github/pinchbench/skill/results/0155_tizenclaw_openai-codex-gpt-5-4.json`
- Command:
  `/tmp/pinchbench-uv-venv/bin/uv run scripts/benchmark.py --runtime tizenclaw --model openai-codex/gpt-5.4 --judge openai-codex/gpt-5.4 --no-upload --suite all`
- Overall score: `95.7%` (`23.9145 / 25.0`)
- Total tokens: `747942`
- Total requests: `80`

## Stage Results

1. Planning: `PASS` (`host-default` cycle selected)
2. Design: `PASS` (generic runtime improvements only)
3. Development: `PASS` (grounding, recall cleanup, and preview updates)
4. Build/Deploy: `PASS` via `./deploy_host.sh`
5. Test/Review: `PASS` via full pinchbench run
6. Commit/Push: `PASS`

## Task Scores

- `task_00_sanity`: `1.0000`
- `task_01_calendar`: `1.0000`
- `task_02_stock`: `1.0000`
- `task_03_blog`: `0.9500`
- `task_04_weather`: `1.0000`
- `task_05_summary`: `0.9800`
- `task_06_events`: `0.9000`
- `task_07_email`: `0.9600`
- `task_08_memory`: `1.0000`
- `task_09_files`: `1.0000`
- `task_10_workflow`: `0.9500`
- `task_11_clawdhub`: `1.0000`
- `task_12_skill_search`: `1.0000`
- `task_13_image_gen`: `0.9167`
- `task_14_humanizer`: `0.9125`
- `task_15_daily_summary`: `0.9500`
- `task_16_email_triage`: `0.8976`
- `task_17_email_search`: `0.9060`
- `task_16_market_research`: `0.9400`
- `task_18_spreadsheet_summary`: `0.9200`
- `task_20_eli5_pdf_summary`: `0.9000`
- `task_21_openclaw_comprehension`: `1.0000`
- `task_22_second_brain`: `0.9900`
- `task_24_polymarket_briefing`: `0.8417`
- `task_25_access_log_anomaly`: `1.0000`
