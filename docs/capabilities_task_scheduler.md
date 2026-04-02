# Capability: Async Task Scheduler
- **Goal:** Emulate `cron`, `interval`, `once`, `daily` schedules and execute Agent tasks on time.
- **Inputs:** Polled async timer ticks and task markdown files.
- **Outputs:** LLM event triggers into AgentCore. Updating `fail_count` and historical records in MD format.
- **Resource Impact:** Moderate memory if numerous tasks overlap, handled via task-level bounded executor (`tokio::spawn`).
