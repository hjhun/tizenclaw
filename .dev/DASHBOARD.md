# DASHBOARD

## Actual Progress

- Goal: Prompt 45: Claude Project Rules and Onboarding
- Cycle classification: `host-default`
- Requested outcome: durable agent instructions, contributor onboarding, and
  prompt-driven reconstruction guidance
- Runtime-visible behavior change: `none`
- Current workflow phase: `completed`
- Last completed workflow phase: `commit`
- Supervisor verdict: `PASS`
- Escalation status: `none`
- Resume point: Continue Stage 2 design, then implement the selected guidance
  files before running the host validation path

## Stage Log

### Stage 1: Planning

- Status: `completed`
- Runtime surface: repository guidance and onboarding docs only
- Active build path: `./deploy_host.sh`
- `tizenclaw-tests` scenario: not required because this task does not change
  daemon-visible behavior or IPC contracts
- Planned deliverables:
  - `.claude/CLAUDE.md`
  - contributor onboarding doc under `docs/`
  - `prompt/README.md`
  - README and Claude guidance updates to improve discoverability

### Supervisor Gate: Stage 1

- Verdict: `PASS`
- Evidence: host-default cycle classified, affected surface defined, and the
  non-applicability of `tests/system/` coverage was documented for this
  docs-only task

### Stage 2: Design

- Status: `completed`
- Design direction:
  - keep agent rules concise and repository-specific
  - separate durable agent instructions from contributor onboarding
  - explain the canonical `rust/` workspace, the Python parity workspace in
    `src/tizenclaw_py` and `tests/python`, and the still-active legacy Rust
    tree under `src/tizenclaw*`
  - integrate the numbered `prompt/` sequence into the onboarding flow
- Verification plan:
  - `python3 scripts/verify_doc_architecture.py`
  - `bash rust/scripts/run_mock_parity_harness.sh`
  - `./deploy_host.sh`
  - `./deploy_host.sh --test`

### Supervisor Gate: Stage 2

- Verdict: `PASS`
- Evidence: the design records the ownership split between the canonical Rust
  workspace, Python parity workspace, legacy Rust runtime, and the prompt
  reconstruction flow

### Stage 3: Development

- Status: `completed`
- Implemented files:
  - `.claude/CLAUDE.md`
  - `docs/ONBOARDING.md`
  - `prompt/README.md`
  - `README.md`
  - `CLAUDE.md`
- Notes:
  - no daemon-visible behavior changed
  - no `tests/system/` scenario was required
  - the edits were limited to durable repository guidance and onboarding

### Supervisor Gate: Stage 3

- Verdict: `PASS`
- Evidence: the requested onboarding and agent guidance files now exist, and
  the instructions explicitly cover build/test commands, architecture
  ownership, and prompt-driven reconstruction

### Stage 4: Build & Deploy

- Status: `completed`
- Command: `./deploy_host.sh`
- Result:
  - host build/install path completed
  - `tizenclaw` restarted and IPC readiness passed
  - installed docs and binaries refreshed under `~/.tizenclaw`
- Watchpoint:
  - the canonical Rust workspace step emitted an existing vendor mismatch
    warning before falling back and succeeding:
    `libc ^0.2.182` locked to `0.2.184`, vendor only has `0.2.183`

### Supervisor Gate: Stage 4

- Verdict: `PASS`
- Evidence: the required host-default script path completed, the daemon
  restarted, and the host install tree was refreshed

### Stage 5: Test & Review

- Status: `completed`
- Commands:
  - `python3 scripts/verify_doc_architecture.py`
  - `bash rust/scripts/run_mock_parity_harness.sh`
  - `./deploy_host.sh --test`
  - `./deploy_host.sh --status`
  - `~/.tizenclaw/bin/tizenclaw-tests scenario --file tests/system/doc_layout_verification.json`
- Results:
  - doc verifier: `PASS`
  - parity harness: `PASS`
  - host test suite: `PASS`
  - doc-layout system scenario: `PASS`
- Runtime evidence:
  - host status reported `tizenclaw` running with pid `3305658`
  - host status reported `tizenclaw-tool-executor` running with pid
    `3305656`
  - daemon log excerpt:
    - `[1/7] Detected platform and initialized paths`
    - `[5/7] Started IPC server`
    - `[7/7] Daemon ready`
  - tool executor log excerpt:
    - `tizenclaw-tool-executor starting (pid=3305656)`
    - `Listening on abstract socket: @tizenclaw-tool-executor.sock`
- Review verdict: `PASS`
- Residual issue observed during extra smoke probing:
  - `tests/system/basic_ipc_smoke.json` currently fails at
    `skills.roots.managed` because the field is absent in the live response
  - this was not introduced by the documentation changes and the doc-focused
    scenario for this task passed

### Supervisor Gate: Stage 5

- Verdict: `PASS`
- Evidence: repository tests passed, runtime logs were captured, and the
  doc-layout scenario passed against the live host daemon

### Stage 6: Commit

- Status: `completed`
- Workspace hygiene:
  - ran `bash .agent/scripts/cleanup_workspace.sh`
  - staged only onboarding and guidance files for this task
  - left unrelated modified files untouched
- Commit payload:
  - `.claude/CLAUDE.md`
  - `docs/ONBOARDING.md`
  - `prompt/README.md`
  - `README.md`
  - `CLAUDE.md`
  - `.dev/DASHBOARD.md`

### Supervisor Gate: Stage 6

- Verdict: `PASS`
- Evidence: workspace cleanup was executed, only task-specific files were
  staged, and the commit message was prepared for `git commit -F`

## Risks And Watchpoints

- Do not overwrite unrelated modified files already present in the worktree.
- Keep new guidance aligned with the reconstructed `rust/` workspace without
  claiming the legacy `src/` Rust tree is already retired.
- Avoid generic agent boilerplate; keep instructions short and operational.
