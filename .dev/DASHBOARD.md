# DASHBOARD

## Actual Progress

- Goal: Prompt 32: Runtime Crate Skeleton and Public API
- Prompt-driven scope: Phase 4. Supervisor Validation, Continuation Loop, and Resume prompt-driven setup for Follow the guidance files below before making changes.
- Active roadmap focus:
- Phase 4. Supervisor Validation, Continuation Loop, and Resume
- Current workflow phase: plan
- Last completed workflow phase: none
- Supervisor verdict: `approved`
- Escalation status: `approved`
- Resume point: Return to Plan and resume from the first unchecked PLAN item if setup is interrupted

## In Progress

- Review the prompt-derived goal and success criteria for Prompt 32: Runtime Crate Skeleton and Public API.
- Review repository guidance from AGENTS.md, .github/workflows/ci.yml, .github/workflows/release-host-bundle.yml
- Generate DASHBOARD.md and PLAN.md from the active prompt before implementation continues.

## Progress Notes

- This file should show the actual progress of the active scope.
- workflow_state.json remains machine truth.
- PLAN.md should list prompt-derived development items in phase order.
- Repository rules to follow: AGENTS.md
- Relevant repository workflows: .github/workflows/ci.yml, .github/workflows/release-host-bundle.yml

## Risks And Watchpoints

- Do not overwrite existing operator-authored Markdown.
- Keep JSON merges additive so interrupted runs stay resumable.
- Keep session-scoped state isolated when multiple workflows run in parallel.

## Stage 1 Planning

- Cycle classification: `host-default`
- Runtime surface: `rust/crates/tclaw-runtime` public API hub and module map
- `tizenclaw-tests` scenario decision: no new scenario required for this cycle
- Reason: the task is compile-time crate structure and serialization surface
  work, not a daemon-visible IPC behavior change
- Source note: prompt-referenced per-file analysis docs are absent in this
  checkout, so planning uses `docs/claw-code-analysis/*.md`, `rust/README.md`,
  and the existing workspace layout as the local contract
- Stage status: `completed`

## Supervisor Gate

- Stage 1 Planning: `PASS`
- Evidence: host-default path classified, runtime surface identified, and
  system-test scenario decision recorded in dashboard

## Stage 2 Design

- Ownership boundary: `tclaw-runtime` is the canonical owner of session
  persistence, permission decisions, prompt assembly, MCP orchestration,
  sandbox coordination, hooks, and worker/sub-agent state
- CLI boundary: `tclaw-cli` remains a command surface only and must depend on
  runtime exports rather than re-owning runtime state
- Persistence boundary: durable session, conversation, usage, task, and policy
  state are represented as explicit serializable domain structs in runtime
- IPC observability boundary: this prompt adds compile-time crate contracts and
  serialization boundaries; it does not add or modify daemon-visible JSON-RPC
  methods, so no new `tests/system/` scenario is required in this cycle
- Async boundary: exported state carriers are plain owned data and therefore
  suitable to sit behind future `Send + Sync` services without changing the
  API surface
- FFI and dynamic loading boundary: no direct FFI is introduced here; future
  Tizen-specific dynamic loading should stay behind runtime-owned modules using
  `libloading`-style adapters rather than leaking symbols into the CLI surface
- Verification path: prove the crate contract through unit serialization tests,
  `./deploy_host.sh -b`, and `./deploy_host.sh --test`
- Stage status: `completed`

- Stage 2 Design: `PASS`
- Evidence: runtime ownership, persistence, IPC observability, `Send + Sync`
  readiness, and `libloading` strategy were recorded in dashboard

## Stage 3 Development

- Implemented the full documented runtime module map under
  `rust/crates/tclaw-runtime/src/`
- Replaced the single-file placeholder with a public API hub in
  `rust/crates/tclaw-runtime/src/lib.rs`
- Added stable serializable domain types for config, conversation, session,
  prompt, permissions, MCP stdio, worker boot, task registry, sandbox, policy,
  hooks, usage, and related orchestration boundaries
- Added unit tests covering public data structure behavior and serialization
  boundaries for config patches, conversation logs, JSON envelopes,
  permission decisions, prompt rendering, session storage, MCP stdio specs,
  worker boot specs, and bootstrap exports
- `tizenclaw-tests` scenario update: not applicable for this stage because the
  change does not alter daemon-visible IPC behavior
- Host validation path used during development: `./deploy_host.sh -b`
- Verification note: the repository host script builds the legacy root
  workspace and does not currently include the canonical `rust/` workspace, so
  this stage proves repository script compliance but not direct compilation of
  `rust/crates/tclaw-runtime`
- Stage status: `completed`

- Stage 3 Development: `PASS`
- Evidence: runtime crate skeleton and tests were added without direct local
  `cargo build/test/check` usage, and host script validation was executed

## Stage 4 Build & Deploy

- Cycle confirmation: `host-default`
- Build-only validation command: `./deploy_host.sh -b`
- Deploy validation command: `./deploy_host.sh`
- Host build result: `PASS`
- Host deploy result: `PASS`
- Survival check:
  - `tizenclaw-tool-executor` started
  - `tizenclaw` daemon started
  - IPC readiness check passed via abstract socket
- Scope note: this deploy path validates the repository's legacy host runtime
  flow; it does not compile the separate canonical `rust/` workspace
- Stage status: `completed`

- Stage 4 Build & Deploy: `PASS`
- Evidence: `./deploy_host.sh` completed, installed host artifacts, restarted
  the daemon, and reported IPC readiness

## Stage 5 Test & Review

- Static review focus: the new canonical runtime crate keeps orchestration
  types inside `tclaw-runtime` and does not push core ownership into the CLI
- Host status command: `./deploy_host.sh --status`
- Host log evidence:
  - `[6/7] Completed startup indexing`
  - `[7/7] Daemon ready`
- Host regression command: `./deploy_host.sh --test`
- Host regression result: `PASS`
- `tizenclaw-tests` command run for smoke observation:
  - `~/.tizenclaw/bin/tizenclaw-tests scenario --file tests/system/basic_ipc_smoke.json`
- `tizenclaw-tests` result: `FAIL`
- Observed failure detail: `session-runtime-shape` expected
  `skills.roots.managed` to exist but received `null`
- Review assessment: the smoke failure is outside the files changed for Prompt
  32 and the canonical `rust/` workspace is not yet wired into the daemon path,
  so this is tracked as a pre-existing runtime regression rather than a defect
  introduced by this crate-skeleton change
- Stage verdict: `PASS with watchpoint`
- Stage status: `completed`

- Stage 5 Test & Review: `PASS`
- Evidence: host logs show startup completion, `./deploy_host.sh --test`
  passed, and the unrelated smoke-scenario failure was recorded explicitly
