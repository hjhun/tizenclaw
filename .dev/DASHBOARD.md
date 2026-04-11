# DASHBOARD

## Actual Progress

- Goal: Prompt 12: Channels — Telegram, Discord, MCP, Voice
- Prompt-driven scope: Extend channel integrations under
  `src/tizenclaw/src/channel/` without breaking the existing daemon
  lifecycle and IPC model.
- Active roadmap focus: Prompt 12 channel registry, outbound channels,
  MCP stdio client, and config-driven factory loading.
- Current workflow phase: test_review
- Last completed workflow phase: test_review
- Supervisor verdict: `pass through stage 5`
- Escalation status: `none`
- Resume point: Continue from the current stage gate recorded below.

## In Progress

- Stage 1 Planning
- Stage 2 Design
- Stage 6 Commit & Push

## Progress Notes

- Repository rules to follow: AGENTS.md
- Build path classification: host-default cycle using `./deploy_host.sh`
- Existing channel API diverges from the prompt:
  `Channel` uses `start/stop/is_running/send_message`, registry is
  lifecycle-oriented, and Telegram is already a large bidirectional
  client. Changes must fit that architecture.
- Runtime surface to change:
  `channel/mod.rs`, `channel/channel_factory.rs`,
  `channel/telegram_client.rs`, `channel/discord_channel.rs`,
  `channel/slack_channel.rs`, `channel/webhook_channel.rs`,
  `channel/mcp_client.rs`, and supporting tests.
- Planned IPC-observable system scenario:
  `tests/system/channel_registry_runtime_contract.json`
  covering registry-managed channel lifecycle through existing IPC.
- External network integrations with real credentials will be validated
  through deterministic unit tests and script-driven host regression,
  not live third-party endpoints.

## Risks And Watchpoints

- Do not log channel secrets such as bot tokens or webhook URLs.
- Outbound sends must stay non-fatal even when HTTP calls fail.
- Keep message splitting on safe boundaries instead of truncating
  mid-word.
- MCP changes must preserve current stdio client-manager behavior while
  adding prompt-required connection and tool-call coverage.

## Stage Records

### Stage 1: Planning

- Status: PASS
- Cycle classification: host-default
- Affected runtime surface:
  registry-managed outbound channels plus MCP stdio tool discovery/call
- Required test contract:
  add `tests/system/channel_registry_runtime_contract.json` for
  registry-visible behavior and add unit coverage for outbound channel
  splitting, webhook method/headers, config loading, and MCP stdio
  discovery.

### Supervisor Gate: Stage 1 Planning

- Verdict: PASS
- Evidence:
  host-default cycle selected, runtime surface identified, dashboard
  updated, and system-test contract selected for daemon-visible changes.

### Stage 2: Design

- Status: PASS
- Subsystem boundaries and ownership:
  keep `ChannelRegistry` as the daemon-owned lifecycle container; extend
  it with status aggregation and config-oriented construction helpers
  instead of replacing it with a new shared-map API.
- Channel runtime boundaries:
  Telegram, Discord, Slack, and generic webhook remain outbound channel
  implementations behind `Channel::send_message`; `start()` for
  outbound-only channels becomes lightweight activation rather than
  background polling.
- Persistence and config boundaries:
  config parsing stays in `channel_factory` and `ChannelRegistry`; each
  channel gets a `from_value` or equivalent parsing path that validates
  required fields, applies safe defaults, and never logs secrets.
- HTTP boundary:
  outbound HTTP calls use `ureq` with a 10-second timeout and explicit
  headers; message splitting is shared by helper logic so Telegram uses
  4000 chars and Discord uses 2000 chars.
- MCP boundary:
  preserve existing `McpClientManager`, but extend `McpClient` with a
  config-driven connect path and a tool declaration type alias compatible
  with current LLM tool wiring. Stdio remains the primary implemented
  transport, with HTTP config parsing modeled without breaking current
  callers.
- IPC-observable verification:
  the new system scenario will validate registry-managed channel status
  through existing JSON-RPC `channel_status` / `start_channel` /
  `stop_channel`; outbound channel HTTP specifics and MCP stdio exchange
  will be covered by unit tests.

### Supervisor Gate: Stage 2 Design

- Verdict: PASS
- Evidence:
  ownership boundaries, runtime/config boundaries, dynamic stdio MCP
  loading strategy, and verification path were defined and recorded.

### Stage 3: Development

- Status: PASS
- System-test scenario added:
  `tests/system/channel_registry_runtime_contract.json`
- Unit coverage added for:
  channel config parsing, Discord splitting/webhook payloads,
  Slack webhook payloads, generic webhook method/headers,
  Telegram single-chat config parsing, and MCP stdio discovery/call.
- Implementation summary:
  normalized flat-vs-nested channel config parsing, added registry
  `status_all()`, hardened broadcast error handling, refit outbound
  Discord/Slack/Webhook semantics to the existing `Channel` lifecycle,
  added Telegram config parsing plus safe chunked sends, and refactored
  MCP client setup around config-driven stdio/http transport parsing.
- Script-driven validation used:
  `./deploy_host.sh -b`
- Development note:
  the prompt asked for `ureq`, but the workspace builds in offline
  vendored mode and does not vendor `ureq`. The implementation was
  adapted to use the repo's existing vendored HTTP stack so the required
  host build path remains green.

### Supervisor Gate: Stage 3 Development

- Verdict: PASS
- Evidence:
  no direct cargo commands were used outside the repository script,
  development artifacts and tests were added, and `./deploy_host.sh -b`
  completed successfully.

### Stage 4: Build & Deploy

- Status: PASS
- Script used:
  `./deploy_host.sh`
- Deployment evidence:
  host install completed, `tizenclaw-tool-executor` started, `tizenclaw`
  started, and IPC readiness passed on the host abstract socket.

### Supervisor Gate: Stage 4 Build & Deploy

- Verdict: PASS
- Evidence:
  default host script path was used, install completed, daemon restart
  succeeded, and IPC readiness was confirmed.

### Stage 5: Test & Review

- Status: PASS
- Runtime proof:
  `./deploy_host.sh --status` reported `tizenclaw` running with pid
  `3008841` and `tizenclaw-tool-executor` running with pid `3008839`.
- Runtime log proof:
  host log excerpts include `[5/7] Started IPC server` and
  `[7/7] Daemon ready`.
- System scenario command:
  `~/.tizenclaw/bin/tizenclaw-tests scenario --file tests/system/channel_registry_runtime_contract.json`
- System scenario result:
  4/4 steps passed for registry-managed channel lifecycle.
- Regression command:
  `./deploy_host.sh --test`
- Regression result:
  repository-wide host test cycle passed, including the new channel and
  MCP tests.
- QA verdict:
  PASS. No blocking defects remain in the modified channel paths.

### Supervisor Gate: Stage 5 Test & Review

- Verdict: PASS
- Evidence:
  runtime status and log proof were captured, the planned system
  scenario passed against the live daemon, and the script-driven host
  regression cycle passed cleanly.
