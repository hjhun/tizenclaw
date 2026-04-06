<p align="center">
  <img src="data/img/tizenclaw.svg" alt="TizenClaw Logo" width="280">
</p>

<h1 align="center">TizenClaw</h1>

<p align="center">
  <strong>An autonomous Rust agent daemon for Tizen and embedded Linux.</strong><br>
  TizenClaw turns an embedded device into a persistent AI runtime with IPC,
  multi-channel delivery, dashboard access, plugin-driven extensions, and
  Tizen-aware system integration.
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-Apache_2.0-blue.svg" alt="License"></a>
  <img src="https://img.shields.io/badge/Language-Rust-orange.svg" alt="Rust">
  <img src="https://img.shields.io/badge/Platform-Tizen%20%2B%20Embedded%20Linux-brightgreen.svg" alt="Platform">
  <img src="https://img.shields.io/badge/Runtime-Tokio-black.svg" alt="Tokio">
</p>

<p align="center">
  <a href="#overview">Overview</a> •
  <a href="#capabilities">Capabilities</a> •
  <a href="#quick-start">Quick Start</a> •
  <a href="#workspace">Workspace</a> •
  <a href="#documentation">Documentation</a>
</p>

---

## Overview

TizenClaw is a long-running AI agent daemon built in Rust for devices that
need more than a chat wrapper. It is designed for Tizen and adjacent embedded
Linux environments where the agent must stay alive, manage constrained system
resources, communicate over multiple interfaces, and integrate with platform
services through carefully bounded FFI.

The project combines a Tokio-based daemon, an IPC-accessible CLI, a standalone
web dashboard, a dedicated tool-execution sidecar, and plugin metadata crates
that make it possible to extend the runtime without collapsing everything into
one binary. The result is an agent platform that is easier to reason about in
embedded deployments: orchestration in Rust, platform boundaries kept explicit,
and deployment handled through the Tizen build pipeline instead of ad hoc local
build steps.

This repository is the Rust edition of TizenClaw. It focuses on persistent
operation, safe concurrency, runtime adaptability, and deployment to emulator
or device targets through the repository's `deploy.sh` workflow.

## Why TizenClaw

Embedded agent runtimes have different requirements from desktop assistants or
cloud-only gateways. They must survive process restarts, fit within a device
budget, expose observable services, and cooperate with platform APIs that may
or may not be available at runtime.

TizenClaw is organized around those constraints:

- The main daemon owns orchestration, scheduling, channel lifecycle, IPC, and
  agent behavior.
- Tizen-specific access is isolated behind crates and adapters instead of being
  spread throughout the codebase.
- Shared libraries are loaded dynamically when needed, which lets the runtime
  degrade more safely across target environments.
- Deployment is centered on the actual Tizen packaging path, not on a separate
  host-only development story that diverges from production.

## Capabilities

### Persistent Agent Runtime

- Runs as a daemon instead of a single-shot command.
- Initializes logging, storage paths, channels, IPC, scheduler, and discovery
  during boot.
- Keeps the agent available for CLI requests, dashboard requests, background
  tasks, and outbound notifications.

### Multi-Surface Access

- IPC access through the `tizenclaw-cli` client.
- Web dashboard process for browser-based interaction and administration.
- Channel framework with built-in support for dashboard, webhook, Slack,
  Discord, voice, MCP, and related transport layers found in the workspace.

### Multi-Backend Model Support

- Built-in LLM backends in the daemon source include OpenAI, Anthropic,
  Gemini, Ollama, and plugin-managed backends.
- Configuration is kept outside the binary so deployments can adapt to the
  target environment and provider mix.

### Embedded-Friendly Extensibility

- `tizenclaw-tool-executor` isolates tool execution into a dedicated service.
- Metadata plugin crates provide extension points for CLI, skill, and LLM
  backend plugins.
- `libtizenclaw` and `libtizenclaw-core` keep FFI ownership explicit for
  consumers that need C-facing integration.

### Tizen-Aware Integration

- Runtime path handling for device-oriented data, config, and web roots.
- Platform-specific adapters under `src/tizenclaw/src/tizen` and generic
  Linux fallbacks under `src/tizenclaw/src/generic`.
- Dynamic shared library loading through `libloading` where platform features
  must be discovered at runtime instead of assumed at link time.

## Quick Start

### Prerequisites

Before you deploy TizenClaw, make sure the following are available:

- Tizen Studio tooling, including `sdb`
- Tizen GBS build environment
- A reachable Tizen emulator or device
- A Linux environment capable of running the repository scripts

The repository's operational workflow is centered on `deploy.sh`, which drives
build, package, deploy, and service restart steps for the target.

### Build and Deploy

For the standard x86_64 emulator-oriented workflow:

```bash
./deploy.sh -a x86_64
```

Useful variants:

```bash
./deploy.sh -a x86_64 -n
./deploy.sh -a x86_64 -d <device-serial>
./deploy.sh -a x86_64 -s
```

What this gives you:

- Tizen package build through the project pipeline
- Device deployment through `sdb`
- Service restart for the daemon and related components

### Talk to the Daemon

Once deployed, the CLI can send prompts over the daemon IPC interface:

```bash
tizenclaw-cli "Summarize the current device state"
tizenclaw-cli --stream "Explain the active channels"
tizenclaw-cli dashboard status
```

### Open the Dashboard

The standalone dashboard process uses port `9090` on Tizen targets and `8080`
on non-Tizen host environments by default. After deployment, the dashboard can
be reached through the forwarded or device-exposed port configured in your
environment.

## Workspace

TizenClaw is a Rust workspace rather than a single crate. The main members are:

- `src/tizenclaw`: the primary daemon binary
- `src/tizenclaw-cli`: IPC client for prompt and dashboard control
- `src/tizenclaw-web-dashboard`: standalone web UI and HTTP API
- `src/tizenclaw-tool-executor`: sidecar for controlled tool execution
- `src/libtizenclaw-core`: shared core framework, loader, and plugin support
- `src/libtizenclaw`: C-facing library for external integration
- `src/tizenclaw-metadata-*`: plugin metadata crates for skills, CLI, and LLM
  backend extensions

At runtime, the daemon brings together core orchestration, storage, network
discovery, model backends, channel management, scheduler behavior, and Tizen or
generic infrastructure adapters.

## Architecture Snapshot

At a high level, the repository works like this:

```text
CLI / Dashboard / Channels
          |
          v
    TizenClaw Daemon
          |
          +-- Core agent orchestration
          +-- Scheduling and background tasks
          +-- Storage and session state
          +-- LLM backend routing
          +-- Channel lifecycle management
          +-- Tizen and generic infrastructure adapters
          |
          +-- Tool Executor Sidecar
          +-- Plugin Metadata Crates
          +-- C/FFI Bridge Libraries
```

The daemon keeps orchestration in Rust, while platform boundaries are isolated
to the crates and adapters that need them. That split is especially important
for Tizen deployments where shared library availability and device services can
vary by image, emulator, or firmware configuration.

## Documentation

Additional project documentation is available in this repository:

- [Structure Guide](docs/STRUCTURE.md)
- [Usage Guide](docs/USAGE.md)

These guides explain how the workspace is organized, what each major component
is responsible for, and how to operate the project through its supported build,
deploy, and runtime entry points.

## Status and Scope

This repository is actively focused on the Rust-based TizenClaw runtime and its
embedded deployment story. Some subsystems are clearly under active evolution,
but the project already exposes a substantial operational surface: daemon boot,
IPC, dashboard service, model backends, storage, plugin metadata, and Tizen
integration layers.

If you want to understand the codebase quickly, start with:

1. `src/tizenclaw/src/main.rs`
2. `src/tizenclaw/src/core/`
3. `src/tizenclaw-web-dashboard/src/main.rs`
4. `deploy.sh`
5. `docs/STRUCTURE.md`

## License

Apache License 2.0. See [LICENSE](LICENSE).
