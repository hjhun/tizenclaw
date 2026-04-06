# TizenClaw Structure Guide

## Purpose

This guide explains how the TizenClaw repository is organized and how the
major binaries, libraries, and subsystem directories fit together. It is aimed
at contributors, reviewers, and operators who need a quick mental model of the
workspace before making changes or deploying the daemon.

## Top-Level Layout

```text
tizenclaw/
├── .agent/                 Agent workflow rules and stage skills
├── .dev_note/              Internal planning, design, and dashboard tracking
├── data/                   Static assets, including dashboard resources
├── docs/                   Public documentation
├── packaging/              Tizen packaging and build metadata
├── src/                    Rust workspace members
├── third_party/            Vendored third-party sources
├── Cargo.toml              Workspace definition
└── deploy.sh               Main build, deploy, and restart entry point
```

## Workspace Members

The root `Cargo.toml` defines a multi-crate workspace. Each member exists for
an operational reason rather than convenience alone.

### `src/tizenclaw`

This is the main daemon binary. It is responsible for:

- platform detection
- runtime path creation
- logging initialization
- agent core boot and shutdown
- task scheduler startup
- channel registry initialization
- IPC server startup
- network discovery bootstrap

This crate is where the long-running service comes together.

### `src/tizenclaw-cli`

This crate provides the command-line client that talks to the daemon over the
IPC socket. It is the most direct way to:

- send prompts
- receive streamed responses
- request usage data
- start, stop, or inspect the web dashboard channel

It behaves like a thin operational client rather than a second agent runtime.

### `src/tizenclaw-web-dashboard`

This is the standalone HTTP dashboard service. It serves the web UI, exposes
REST endpoints, and bridges browser interactions back to the daemon. It also
owns dashboard-specific concerns such as:

- static asset serving
- admin authentication endpoints
- dashboard session summaries
- outbound message polling
- bridge-style APIs for app and tool interactions

### `src/tizenclaw-tool-executor`

This sidecar binary handles tool execution separately from the main daemon.
That separation keeps the daemon focused on orchestration while the executor
owns the lower-level responsibility of running approved tools safely.

### `src/libtizenclaw-core`

This shared library crate provides reusable framework pieces that other parts
of the workspace depend on. It includes:

- platform and path detection
- loader and plugin support
- curl and network-related helpers
- Tizen system bindings support modules

It is a key boundary between the application layer and lower-level integration
details.

### `src/libtizenclaw`

This crate is the C-facing library surface. It exists so external consumers can
interact with TizenClaw through a stable foreign-function boundary rather than
linking directly against daemon internals.

### `src/tizenclaw-metadata-plugin`

This base metadata crate is shared by plugin-specific metadata crates.

### `src/tizenclaw-metadata-llm-backend-plugin`

Plugin metadata for LLM backend extensions.

### `src/tizenclaw-metadata-skill-plugin`

Plugin metadata for skill extensions.

### `src/tizenclaw-metadata-cli-plugin`

Plugin metadata for CLI-oriented extensions.

## Main Daemon Source Layout

Inside `src/tizenclaw/src`, the daemon code is grouped by responsibility:

```text
src/tizenclaw/src/
├── channel/
├── common/
├── core/
├── generic/
├── infra/
├── llm/
├── network/
├── storage/
├── tizen/
└── main.rs
```

### `channel/`

The channel layer owns communication surfaces and channel registration. Based
on the current source tree, this area includes support or scaffolding for:

- web dashboard
- webhook
- Slack
- Discord
- voice
- MCP client/server behavior
- agent-to-agent style handling
- channel factory and registry wiring

### `common/`

Shared application utilities such as logging, JSON helpers, boot status
tracking, and generally reusable helpers live here.

### `core/`

This is the heart of the daemon. Important responsibilities in this directory
include:

- `agent_core.rs`: central agent runtime and orchestration logic
- `agent_factory.rs`: agent construction and role wiring
- prompt building
- tool declaration and dispatch
- context fusion and perception
- workflow execution
- task scheduling
- safety and policy enforcement
- runtime path utilities
- IPC server support

If you only have time to understand one subsystem, start here.

### `generic/`

Generic Linux-compatible infrastructure and fallback integrations live here.
This is useful for environments that are not running the full Tizen stack but
still need the daemon to behave sensibly.

### `infra/`

Shared infrastructure helpers that do not belong to a single feature area are
grouped here.

### `llm/`

The model backend layer lives here. The current source tree includes modules
for:

- OpenAI
- Anthropic
- Gemini
- Ollama
- plugin-managed backends

This is the area to inspect when model selection, provider behavior, or backend
capabilities need to change.

### `network/`

Network discovery features such as mDNS scanning are located here.

### `storage/`

Persistent and semi-persistent state handling is grouped into this directory.
Current modules indicate support for:

- SQLite-backed storage
- session storage
- memory storage
- embedding storage
- audit logging

### `tizen/`

Tizen-specific code is isolated here so platform logic stays visible and
contained. This includes Tizen-facing core actions and infrastructure adapters
for things like application or package lifecycle handling.

## Runtime Relationships

The major runtime relationships look like this:

1. The daemon boots and detects the current platform.
2. Shared runtime paths are prepared.
3. Logging, scheduler, channels, IPC, and discovery services are started.
4. External clients interact through the CLI, dashboard, or configured
   channels.
5. Tool execution and plugin metadata extend the daemon without requiring
   every concern to live in the main binary.

## Build and Deployment Files

Several non-source paths matter operationally:

- `deploy.sh`: the primary command for build and deployment
- `packaging/`: spec and package-related assets used by the Tizen flow
- `data/`: web assets and related bundled resources
- `third_party/openssl-src`: vendored OpenSSL source used by the workspace

## How to Read the Repository Efficiently

For a practical code walk, this order works well:

1. `Cargo.toml`
2. `deploy.sh`
3. `src/tizenclaw/src/main.rs`
4. `src/tizenclaw/src/core/agent_core.rs`
5. `src/tizenclaw/src/core/agent_factory.rs`
6. `src/tizenclaw-web-dashboard/src/main.rs`
7. `src/tizenclaw-cli/src/main.rs`

That sequence gives you the workspace layout, operational entry point,
orchestration center, and the two most visible client surfaces.
