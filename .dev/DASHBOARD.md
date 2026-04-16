# DASHBOARD

## Active Goal

Compare tizenclaw against openclaw and openclaude, identify gaps, author ROADMAP.md,
implement ClawHub-ready host path, and clean up Telegram coding-agent UX.

## Current Stage

**Stage 7. Evaluate** — DONE (tester fix applied)

## Stage Completion Status

| Stage | Status |
|---|---|
| 0. Refine | DONE |
| 1. Plan | DONE |
| 2. Design | DONE |
| 3. Develop | DONE |
| 4. Build/Deploy | DONE |
| 5. Test/Review | DONE |
| 6. Commit | DONE |
| 7. Evaluate | DONE |

## Scope

### ClawHub Integration
- New `clawhub_client.rs` module in tizenclaw daemon
- IPC methods: `clawhub_install`, `clawhub_search`, `clawhub_list`
- CLI commands: `tizenclaw-cli skill-hub install|search|list`
- Install target: `~/.tizenclaw/workspace/skill-hubs/clawhub/<slug>/`
- Lock file: `~/.tizenclaw/workspace/.clawhub/lock.json`
- Base URL: `https://clawhub.ai` (env override: `TIZENCLAW_CLAWHUB_URL`)
- `zip` crate (v2, deflate) vendored for archive extraction

### Telegram UX Cleanup
- Removed commands: `/coding_agent`, `/devel`, `/devel_result`, `/mode`, `/auto_approve`
- Removed `/select coding` from keyboard; only `/select chat` remains
- Removed from status/startup/connected messages: `CodingAgent:`, `CodingMode:`,
  `AutoApprove:`, `Binary:`, `Runs:`
- Removed from `TelegramPendingMenu`: `CodingAgent`, `ExecutionMode`, `AutoApprove`
- `TelegramInteractionMode::Coding` retained internally for persisted state compat
- Removed dead functions: `set_cli_backend`, `set_execution_mode`, `set_auto_approve`
- Updated 42 Telegram tests: all pass

### CLI Help and Setup Cleanup (c85cad34)
- `tizenclaw-cli --help` now lists ClawHub commands under "ClawHub commands:" section
- `tizenclaw-cli setup` no longer shows "Telegram coding mode" wording
- Setup prompt now reads: "Do you want to configure Telegram now?"

### Tester Rework — Vendor/Lock Mismatch Fix (22464562)
- Root cause: `rust/Cargo.toml` declared `thiserror = "1"`, but the vendor
  directory only contains `thiserror 2.0.18`. `rust/Cargo.lock` was locked to
  `thiserror 1.0.69`, causing `--offline --locked` to fail and fall back to
  network resolution on every build.
- Fix: bumped workspace dependency to `thiserror = "2"` and updated
  `rust/Cargo.lock` to `2.0.18` with correct checksums from vendor.
- Result: canonical rust workspace build now resolves fully offline with no
  WARN fallback.

## Test Results (Stage 5 + Rework)

| Suite | Pass | Fail |
|---|---|---|
| Telegram client | 42 | 0 |
| All tizenclaw tests | 561 | 6 (pre-existing, unrelated) |
| ClawHub live search | ✓ live response from clawhub.ai | — |
| ClawHub list | ✓ returns empty lock | — |
| Parity harness | PASS | — |
| Doc architecture | PASS | — |
| TC-06 CLI help (skill-hub visible) | PASS | — |
| TC-07 Setup wizard (no coding mode) | PASS | — |
| deploy_host.sh -b (clean offline) | PASS | — |

## Risks and Watchpoints

- ClawHub live download requires network access at runtime; offline or rate-limited
  hosts will need the lock file pre-populated.
- 6 pre-existing test failures in `agent_core::tests` (prediction market / news
  summarization) are unrelated to this sprint and were present before these changes.

## Last Updated

2026-04-16 — Tester rework complete. All verifications pass.
Commits cfa3c43d, c85cad34, c5c4c1af, 52ce8e7a, 22464562 on develRust.
