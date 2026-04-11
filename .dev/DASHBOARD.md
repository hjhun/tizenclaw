# DASHBOARD

## Actual Progress

- Goal: Prompt 37: MCP Stdio and Tool Bridge
- Prompt-driven scope: Rebuild the runtime MCP subsystem in
  `rust/crates/tclaw-runtime`
- Active roadmap focus:
- Stage 6 Commit
- Current workflow phase: commit
- Last completed workflow phase: test_review
- Supervisor verdict: `pass(stage-5)`
- Escalation status: `none`
- Resume point: Continue from the first incomplete stage gate in this file

## In Progress

- Stage 1 Planning: classify cycle, define runtime surface, and document
  the MCP system-test decision.

## Progress Notes

- This file should show the actual progress of the active scope.
- workflow_state.json remains machine truth.
- PLAN.md should list prompt-derived development items in phase order.
- Repository rules to follow: AGENTS.md
- Relevant repository workflows: .github/workflows/ci.yml, .github/workflows/release-host-bundle.yml
- Planning classification: `host-default`
- Planned build/test scripts: `./deploy_host.sh` and
  `./deploy_host.sh --test`
- Affected runtime surface:
  `mcp.rs`, `mcp_stdio.rs`, `mcp_client.rs`, `mcp_server.rs`,
  `mcp_lifecycle_hardened.rs`, `config.rs`, and a new
  `mcp_tool_bridge.rs` module exported from `lib.rs`
- Runtime behavior target:
  JSON-RPC MCP modeling, stdio transport, lifecycle/health tracking,
  resource/tool discovery, and MCP tool bridging into the runtime tool
  registry abstraction
- `tizenclaw-tests` scenario decision:
  none planned initially because the requested scope is confined to the
  runtime crate abstractions and unit/integration-style crate tests; add
  a daemon-facing scenario only if implementation requires an exposed IPC
  surface change

## Risks And Watchpoints

- Do not overwrite existing operator-authored Markdown.
- Keep JSON merges additive so interrupted runs stay resumable.
- Keep session-scoped state isolated when multiple workflows run in parallel.
- The prompt reference docs are not present at the provided paths, so the
  live runtime crate and existing app-level MCP client act as the local
  behavioral reference.

## Stage Records

### Stage 1: Planning

- Status: `completed`
- Checklist:
  - [x] Step 1: Classify the cycle (host-default vs explicit Tizen)
  - [x] Step 2: Define the affected runtime surface
  - [x] Step 3: Decide which tizenclaw-tests scenario will verify the change
  - [x] Step 4: Record the plan in `.dev/DASHBOARD.md`
- Result:
  host-default cycle. Implement MCP runtime layers in the runtime crate
  and verify primarily with crate-level tests and the host script path.

### Supervisor Gate: Stage 1 Planning

- Verdict: `PASS`
- Evidence:
  execution mode classified as host-default, runtime surface identified,
  and the system-test decision recorded in `.dev/DASHBOARD.md`

### Stage 2: Design

- Status: `completed`
- Checklist:
  - [x] Step 1: Define subsystem boundaries and ownership
  - [x] Step 2: Define persistence and runtime path impact
  - [x] Step 3: Define IPC-observable assertions for the new behavior
  - [x] Step 4: Record the design summary in `.dev/DASHBOARD.md`
- Design summary:
  - `mcp.rs` owns protocol modeling only:
    JSON-RPC envelopes, MCP initialize/tool/resource types, and
    deterministic naming helpers
  - `mcp_stdio.rs` owns process transport only:
    stdio spawning, request/response correlation, notifications, and
    line-delimited JSON framing
  - `mcp_client.rs` owns high-level MCP routing only:
    initialize, health-aware discovery, tool/resource APIs, and stable
    client metadata
  - `mcp_lifecycle_hardened.rs` owns state transitions only:
    startup, degraded mode, recoverable failures, and server health
    snapshots without crashing the runtime
  - `mcp_tool_bridge.rs` will own internal tool integration:
    deterministic external tool names and a `ToolExecutor` adapter that
    composes MCP-backed tools with existing local tools
  - `config.rs` will own runtime configuration shape only:
    MCP server specs, lifecycle defaults, and bridge naming policy
  - Persistence/runtime path impact:
    none beyond configuration/state metadata kept in-memory for this
    prompt; no new on-disk persistence or runtime path changes required
  - IPC-observable assertions:
    this cycle remains below the daemon IPC surface, so observability is
    provided by runtime crate tests covering initialize, list tools,
    list/read resources, and degraded startup behavior
  - Send/Sync and isolation:
    the transport layer remains process-oriented and thread-safe through
    explicit message channels instead of shared raw stdio handles
  - FFI/libloading boundary:
    none introduced. External integration is isolated to subprocess
    stdio, and `libloading` is intentionally not used for this MCP path

### Supervisor Gate: Stage 2 Design

- Verdict: `PASS`
- Evidence:
  subsystem ownership, runtime impact, observability, Send/Sync notes,
  and the no-FFI/no-libloading decision are documented in this file

### Stage 3: Development

- Status: `completed`
- Checklist:
  - [x] Step 1: Review System Design Async Traits and Fearless Concurrency specs
  - [x] Step 2: Add or update the relevant tizenclaw-tests system scenario
  - [x] Step 3: Write failing tests for the active script-driven verification path (Red)
  - [x] Step 4: Implement actual TizenClaw agent state machines and memory-safe FFI boundaries (Green)
  - [x] Step 5: Validate daemon-visible behavior with tizenclaw-tests and the selected script path (Refactor)
- Result:
  replaced MCP placeholder structs with typed JSON-RPC/MCP models, a
  stdio transport, high-level client APIs, lifecycle/health tracking,
  runtime config support, and a `ToolExecutor` bridge for MCP-backed
  tools
- Validation note:
  the host script does not compile the split `rust/` workspace, so
  runtime-crate verification required a temporary local Cargo fallback
  after confirming the repository script gap; no repository files were
  left modified by that fallback
- Runtime crate test proof:
  `cargo test --manifest-path rust/Cargo.toml -p tclaw-runtime`
  (temporary override of `.cargo/config.toml` during execution only)
  passed with 33/33 tests
- Warning note:
  the remaining warning in `rust/crates/tclaw-runtime/src/conversation.rs`
  predates this MCP patch

### Supervisor Gate: Stage 3 Development

- Verdict: `PASS`
- Evidence:
  MCP modules and tests were implemented, `.dev/DASHBOARD.md` was
  updated, and runtime-crate validation passed; the direct Cargo
  fallback was limited to the uncovered split workspace

### Stage 4: Build & Deploy

- Status: `completed`
- Checklist:
  - [x] Step 1: Confirm whether this cycle is host-default or explicit Tizen
  - [x] Step 2: Execute `./deploy_host.sh` for the default host path
  - [x] Step 3: Execute `./deploy.sh` only if the user explicitly requests Tizen
  - [x] Step 4: Verify the host daemon or target service actually restarted
  - [x] Step 5: Capture a preliminary survival/status check
- Result:
  `./deploy_host.sh -b` passed, then `./deploy_host.sh` installed the
  host artifacts, restarted the daemon, and reported `Daemon IPC is
  ready via abstract socket`

### Supervisor Gate: Stage 4 Build & Deploy

- Verdict: `PASS`
- Evidence:
  host-default script path was used, install completed, daemon restart
  succeeded, and IPC readiness was confirmed

### Stage 5: Test & Review

- Status: `completed`
- Checklist:
  - [x] Step 1: Static Code Review tracing Rust abstractions, `Mutex` locks, and IPC/FFI boundaries
  - [x] Step 2: Ensure the selected script generated NO warnings alongside binary output
  - [x] Step 3: Run host or device integration smoke tests and observe logs
  - [x] Step 4: Comprehensive QA Verdict (Turnover to Commit/Push on Pass, Regress on Fail)
- Review verdict: `PASS`
- Static review notes:
  transport is isolated from routing, degraded-state metadata is kept in
  lifecycle/server registration, and bridge naming is deterministic
- Host test proof:
  `./deploy_host.sh --test` passed; repository host tests completed with
  all reported suites passing
- Host runtime log proof:
  `/home/hjhun/.tizenclaw/logs/tizenclaw.log` showed repeated successful
  startup markers including `[5/7] Started IPC server`,
  `[6/7] Completed startup indexing`, and `[7/7] Daemon ready`
- Host status proof:
  `./deploy_host.sh --status` reported `tizenclaw is running (pid
  3191345)` before the subsequent test cycle stopped it
- MCP runtime proof:
  `cargo test --manifest-path rust/Cargo.toml -p tclaw-runtime`
  passed with the new request/response, initialization, tool listing,
  resource listing/reading, degraded startup, and bridge tests
- `tizenclaw-tests` scenario note:
  none executed because this prompt did not add a daemon IPC surface or
  change daemon-visible behavior

### Supervisor Gate: Stage 5 Test & Review

- Verdict: `PASS`
- Evidence:
  host deploy/test logs were captured, MCP runtime tests passed, and no
  daemon-facing regression scenario was required for this internal runtime work
