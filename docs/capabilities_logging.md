# Capability: TizenClaw PID Logging
- **Goal:** Format and persist `tracing` logs with `YYYYMMDD.HHMMSS.sssUTC|PID|` on rotating `tizenclaw.log`.
- **Inputs:** Raw tracing events.
- **Outputs:** Formatted appended line in `.log`.
- **Resource Impact:** Extremely low. Output runs on dedicated blocking thread via `tracing-appender` to avoid async blocking.
