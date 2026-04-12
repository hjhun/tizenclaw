# TizenClaw

TizenClaw is a Rust-first autonomous agent daemon that supports a
host-first Linux workflow and an explicit Tizen deployment path. The
repository already includes a runnable daemon, a sandboxed tool executor,
a web dashboard, IPC/system test scenarios, and the packaging scripts
used for host and device validation.

## Current Layout

- `src/`: active Rust workspace for the daemon, CLI, shared libraries,
  tool executor, metadata plugins, system-test client, and web dashboard
- `rust/`: forward-looking Rust workspace for the newer modular runtime
  split (`tclaw-api`, `tclaw-runtime`, `tclaw-tools`, `tclaw-plugins`)
- `tests/system/`: JSON system scenarios for daemon-visible contracts
- `tests/python/`: repository and parity checks for the Python support
  modules
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
- Check daemon status: `./deploy_host.sh --status`
- Follow daemon logs: `./deploy_host.sh --log`
- Install from the current checkout: `./install.sh --local-checkout`

Host installs live under `~/.tizenclaw/`, including binaries, configs,
logs, tools, and the bundled web assets.

### Tizen deployment

Use `./deploy.sh` only when you need the Tizen packaging and deployment
flow.

- Full build and deploy: `./deploy.sh`
- Build for an explicit architecture: `./deploy.sh -a x86_64`
- Deploy to a specific device: `./deploy.sh -d <serial>`

## Verification Surfaces

- `./deploy_host.sh --test` runs the repository's host validation path
- `tests/system/*.json` covers daemon-visible runtime contracts such as
  IPC, dashboard control, command registry behavior, devel mode, and
  session/runtime shape
- `tests/python/` and `tests/test_porting_workspace.py` cover the Python
  parity and repository support modules
- `rust/scripts/run_mock_parity_harness.sh` checks the newer Rust
  workspace against the Python parity surface

## Related Files

- [ROADMAP.md](/home/hjhun/samba/github/tizenclaw/ROADMAP.md)
- [prompt/README.md](/home/hjhun/samba/github/tizenclaw/prompt/README.md)
- [rust/README.md](/home/hjhun/samba/github/tizenclaw/rust/README.md)
- [.claude/CLAUDE.md](/home/hjhun/samba/github/tizenclaw/.claude/CLAUDE.md)
