# TizenClaw

TizenClaw is a Rust-first autonomous agent daemon that supports a
host-first Linux workflow and an explicit Tizen deployment path. The
repository already includes a runnable daemon, a sandboxed tool executor,
a web dashboard, IPC/system test scenarios, and the packaging scripts
used for host and device validation.

## Current Layout

- `src/`: active Rust workspace for the daemon, CLI, shared libraries,
  tool executor, metadata plugins, system-test client, web dashboard,
  and repository support tools
- `rust/`: forward-looking Rust workspace for the newer modular runtime
  split (`tclaw-api`, `tclaw-runtime`, `tclaw-tools`, `tclaw-plugins`)
- `tests/system/`: JSON system scenarios for daemon-visible contracts
- `data/`: bundled host assets, web UI files, sample configs, and docs
  data consumed by the runtime
- `tools/`: embedded and CLI tool payloads used by the daemon
- `packaging/`: Tizen RPM packaging assets

## Implemented Components

- `tizenclaw`: Tokio-based daemon with platform detection, logging,
  startup indexing, IPC server startup, task scheduling, mDNS scanning,
  optional devel mode, and channel registry management
- `libtizenclaw-core`: core framework and plugin SDK with dynamic Tizen
  library loading and C headers for integration boundaries
- `tizenclaw-tool-executor`: isolated tool execution daemon that speaks a
  length-prefixed JSON protocol over Unix sockets or stdio
- `tizenclaw-web-dashboard`: Axum-based web dashboard that proxies daemon
  IPC endpoints and serves the bundled UI
- `tizenclaw-tests`: system-test client used with the scenarios in
  `tests/system/`
- metadata plugins: parser/export libraries for CLI, skill, and LLM
  backend metadata

## Workflows

### Host-first development

Use the repository scripts instead of direct `cargo build` or
`cargo test` commands.

- Build, install, and restart on the host: `./deploy_host.sh`
- Run the host validation path: `./deploy_host.sh --test`
- Check daemon status (source checkout): `./deploy_host.sh --status`
- Follow daemon logs (source checkout): `./deploy_host.sh --log`
- Install from the current checkout: `./install.sh --local-checkout`

Host installs live under `~/.tizenclaw/`, including binaries, configs,
logs, tools, and the bundled web assets.

### Installed bundle management

`./deploy_host.sh` is the **source-checkout** development entrypoint and
assumes the full repository layout (Cargo workspace, `data/`, `tools/`,
Git metadata). It must not be used to manage a standalone installed
bundle.

Installed TizenClaw bundles expose a dedicated control script,
`tizenclaw-hostctl`, which lives in `~/.tizenclaw/bin/` after running
`./install.sh` (any mode). It is a lifecycle-only interface and supports
only the following actions:

- `tizenclaw-hostctl --help`
- `tizenclaw-hostctl --status`
- `tizenclaw-hostctl --restart-only`
- `tizenclaw-hostctl --stop` (or `-s`)
- `tizenclaw-hostctl --log`

Build, test, install, and remove flags are intentionally **not** part of
`tizenclaw-hostctl`; they fail fast with a message directing the user
back to `./deploy_host.sh` in a repository checkout. This keeps the
installed bundle interface smaller and harder to misuse.

The installer (`install.sh`) and post-install automation drive
`tizenclaw-hostctl` for restart and stop — the same interface remains
available for the user under `~/.tizenclaw/bin/tizenclaw-hostctl` after
setup.

### Tizen deployment

Use `./deploy.sh` only when you need the Tizen packaging and deployment
flow.

- Full build and deploy: `./deploy.sh`
- Build for an explicit architecture: `./deploy.sh -a x86_64`
- Deploy to a specific device: `./deploy.sh -d <serial>`

## Verification Surfaces

- `./deploy_host.sh --test` runs the full host validation path:
  1. Cargo unit and integration tests for all workspace crates
  2. Canonical Rust workspace tests (`rust/`)
  3. Reconstruction parity harness (`rust/scripts/run_mock_parity_harness.sh`)
  4. Documentation architecture verification (`scripts/verify_doc_architecture.py`)
  5. **Offline system contract suite** — boots a temporary isolated daemon
     instance and runs the scenarios listed in
     `tests/system/offline_suite.json` against it. The daemon and all
     companion processes run with `TIZENCLAW_DATA_DIR` pointed at a
     `mktemp` root, `HOME` redirected to an empty directory inside that
     root (blocking `~/.codex/auth.json` and other home-relative
     credential files), and `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, and
     `GEMINI_API_KEY` unset — so no ambient credentials or live hosted
     backends can affect the run regardless of the host machine's state.
     The isolation environment is always cleaned up, even when a scenario
     fails.
- `tests/system/offline_suite.json` is the authoritative list of
  scenarios that run automatically. It covers JSON-RPC routing, runtime
  topology, command registry, channel registry, dashboard start/stop,
  key management, ClawHub update, Rust workspace parity, file-manager
  bridge backend selection (`file_manager_bridge.json`), and
  shortcut-backed prompt flows (`agent_loop_shortcuts_runtime_contract.json`).
  Every scenario in this suite declares `"offline_safe": true`; the
  runner enforces this and fails clearly if a scenario is added without
  the declaration. All shortcut-backed scenarios are fully deterministic
  offline: they exercise pre-LLM shortcut paths and require no
  configured backend.
- Scenarios in `tests/system/` that are **not** in `offline_suite.json`
  (e.g. `devel_mode_*`, `openai_oauth_regression`,
  `research_grounding_runtime_contract`, `prediction_market_briefing_runtime_contract`,
  internet-backed research scenarios) remain opt-in and must be run
  manually against a live daemon with `tizenclaw-tests scenario
  --file <path>`. These intentionally lack `"offline_safe": true`.
- `tests/test_porting_workspace.py` covers the repository support tools
  used for inventory, manifest, and audit checks
- `rust/scripts/run_mock_parity_harness.sh` checks the newer Rust
  workspace against the documented Rust-only repository layout

## Related Files

- [ROADMAP.md](/home/hjhun/samba/github/tizenclaw/ROADMAP.md)
- [prompt/README.md](/home/hjhun/samba/github/tizenclaw/prompt/README.md)
- [rust/README.md](/home/hjhun/samba/github/tizenclaw/rust/README.md)
- [.claude/CLAUDE.md](/home/hjhun/samba/github/tizenclaw/.claude/CLAUDE.md)
