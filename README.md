<p align="center">
  <img src="data/img/tizenclaw.svg" alt="TizenClaw Logo" width="280">
</p>

<h1 align="center">TizenClaw</h1>

<p align="center">
  <strong>A persistent Rust AI agent runtime for Tizen and embedded Linux.</strong><br>
  TizenClaw turns a device into an always-on agent system with Tizen-aware
  integration, multi-surface access, plugin-ready boundaries, and a Telegram
  coding workflow that can drive local <code>codex</code>, <code>gemini</code>,
  and <code>claude</code> CLIs remotely.
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-Apache_2.0-blue.svg" alt="License"></a>
  <img src="https://img.shields.io/badge/Language-Rust-orange.svg" alt="Rust">
  <img src="https://img.shields.io/badge/Platform-Tizen%20%2B%20Embedded%20Linux-brightgreen.svg" alt="Platform">
  <img src="https://img.shields.io/badge/Runtime-Tokio-black.svg" alt="Tokio">
</p>

<p align="center">
  <a href="#why-tizenclaw">Why TizenClaw</a> •
  <a href="#at-a-glance">At a Glance</a> •
  <a href="#telegram-coding-over-chat">Telegram Coding Over Chat</a> •
  <a href="#install-on-ubuntu-or-wsl">Install on Ubuntu or WSL</a> •
  <a href="#deploy-to-a-tizen-target">Deploy to a Tizen Target</a>
</p>

---

## Why TizenClaw

TizenClaw is not a one-shot assistant wrapper. It is a long-running agent
daemon built for devices that need to stay alive, react to platform events,
expose stable control surfaces, and survive the messy reality of embedded
Linux deployments.

The project is designed around the constraints that matter on Tizen-class
systems:

- a persistent runtime instead of a fire-and-forget script
- explicit Tizen and generic-Linux boundaries instead of hidden platform
  assumptions
- dynamic loading for platform libraries that may differ by image or firmware
- deploy-first validation through the real Tizen packaging path
- host workflows that still reuse the same workspace and runtime model

If you want an agent that feels closer to an embedded control plane than a
demo chatbot, this is what TizenClaw is for.

## At a Glance

| Area | What TizenClaw Provides |
| --- | --- |
| Runtime model | A persistent Tokio-based daemon with IPC, scheduling, storage, and background automation |
| Platform focus | Tizen-first behavior with generic Linux fallbacks where device APIs are unavailable |
| Access surfaces | CLI, web dashboard, Telegram, webhook, Slack, Discord, MCP, and other channel layers present in the workspace |
| Coding workflow | Telegram can switch into coding mode and drive local `codex`, `gemini`, or `claude` CLIs on the host |
| Extensibility | Dedicated tool executor, metadata plugins, C-facing library, and dynamic `.so` loading |
| Deployment story | `deploy.sh` for emulator/device packaging and deployment, `deploy_host.sh` for Ubuntu/WSL host runs |

## What Makes It Strong

### Built for real device runtimes

TizenClaw keeps orchestration, concurrency, IPC, and state management in Rust,
which makes the system easier to reason about when the process has to stay up
for long periods on constrained hardware.

### Tizen-aware without hard-wiring the whole system to Tizen

Tizen-specific integrations live behind dedicated crates and adapters. Generic
Linux infrastructure is available in parallel, so the runtime can remain useful
on host Linux while still speaking to device-oriented services where they exist.

### Remote coding from Telegram

One of the most distinctive pieces of the project is the Telegram coding mode:
you can chat with the device over Telegram, switch the chat into coding mode,
choose a local coding-agent CLI backend, point that chat at a project
directory, and receive progress and result messages back in Telegram while the
host executes the request.

### Clean boundaries for plugins and external consumers

The repository includes `libtizenclaw`, `libtizenclaw-core`, and metadata
plugin crates so runtime extensions and C-facing integrations do not have to be
bolted onto the daemon as afterthoughts.

## Telegram Coding Over Chat

TizenClaw can use Telegram as a remote control surface for coding workflows.
This is not just "send a prompt to the daemon" behavior. The Telegram channel
can switch into a host-backed coding mode that runs real coding-agent CLIs.

### Supported flow

1. Switch the chat into coding mode with `/select coding`
2. Choose a backend with `/cli_backend codex`, `/cli_backend gemini`, or
   `/cli_backend claude`
3. Bind the chat to a repository with `/project /path/to/repo`
4. Choose execution style with `/mode plan` or `/mode fast`
5. Toggle auto-approval where supported with `/auto_approve on`
6. Inspect the current state with `/status` or start fresh with
   `/new_session`

### What you get

- Per-chat backend selection
- Per-chat project directory overrides
- Separate chat and coding sessions
- Progress updates while the CLI is still running
- Usage tracking for the selected backend
- Host-auth hints when a CLI has not been logged in yet

### Backend examples

TizenClaw maps Telegram coding requests onto the real installed CLIs:

| Backend | Example execution shape |
| --- | --- |
| Codex | `codex exec --json --full-auto -C <project> <prompt>` |
| Gemini | `gemini --prompt <prompt> --output-format text --approval-mode auto_edit` |
| Claude | `claude --print --output-format text --permission-mode auto <prompt>` |

This makes TizenClaw useful as a mobile coding bridge: Telegram becomes the
control surface, while the actual code work happens through the local CLI tools
you already trust on the host.

## Architecture Snapshot

```text
Telegram / CLI / Dashboard / Channels
                |
                v
        +-------------------+
        | TizenClaw Daemon  |
        | Tokio runtime     |
        | IPC + scheduling  |
        | storage + routing |
        +---------+---------+
                  |
      +-----------+--------------------+
      |           |                    |
      v           v                    v
  Tizen adapters  Generic Linux        LLM backends
  and dynloaded   infrastructure        and plugins
  platform APIs   fallbacks
      |
      +-------------------------------+
                                      |
                                      v
                         Tool executor / C API / metadata plugins

Telegram coding mode can also invoke:
  codex / gemini / claude
on the host and stream progress back into chat.
```

## Install on Ubuntu or WSL

If you want to try TizenClaw on host Linux first, the repository now includes a
GitHub-friendly bootstrap script that installs prerequisites, clones or updates
the repository, and delegates the actual host install to `deploy_host.sh`.

### One-line bootstrap

```bash
curl -fsSL https://raw.githubusercontent.com/hjhun/tizenclaw/develRust/install.sh | bash
```

Useful variants:

```bash
curl -fsSL https://raw.githubusercontent.com/hjhun/tizenclaw/develRust/install.sh | bash -s -- --build-only
curl -fsSL https://raw.githubusercontent.com/hjhun/tizenclaw/develRust/install.sh | bash -s -- --ref develRust
curl -fsSL https://raw.githubusercontent.com/hjhun/tizenclaw/develRust/install.sh | bash -s -- --dir "$HOME/src/tizenclaw"
```

What the bootstrap does:

- installs Ubuntu packages needed for host builds
- installs Rust through `rustup` when missing
- clones or updates `https://github.com/hjhun/tizenclaw.git`
- checks out the requested Git ref, defaulting to `develRust`
- runs `deploy_host.sh` to build, install, and optionally start the host tools

### Manual host flow

```bash
git clone https://github.com/hjhun/tizenclaw.git
cd tizenclaw
./deploy_host.sh
```

Useful host commands:

```bash
./deploy_host.sh -b
./deploy_host.sh --status
./deploy_host.sh --log
./deploy_host.sh -s
```

## Deploy to a Tizen Target

For the emulator or device-oriented workflow, use the repository's Tizen deploy
pipeline:

```bash
./deploy.sh -a x86_64
```

Useful variants:

```bash
./deploy.sh -a x86_64 -n
./deploy.sh -a x86_64 -d <device-serial>
./deploy.sh -a x86_64 -s
```

This path is the canonical Tizen validation flow. It handles build, packaging,
deployment, and service restart on the target.

## Workspace

TizenClaw is a Rust workspace with clearly separated runtime roles:

- `src/tizenclaw`: main daemon
- `src/tizenclaw-cli`: IPC client and operational CLI
- `src/tizenclaw-web-dashboard`: standalone web dashboard
- `src/tizenclaw-tool-executor`: isolated tool-execution sidecar
- `src/libtizenclaw-core`: shared framework and plugin/runtime support
- `src/libtizenclaw`: C-facing client library
- `src/tizenclaw-metadata-*`: metadata plugin crates for skills, CLI, and LLM
  backend extensions

## Documentation

Additional repository docs:

- [Structure Guide](docs/STRUCTURE.md)
- [Usage Guide](docs/USAGE.md)

## Status

The project is actively evolving, but the central direction is already clear:
TizenClaw aims to be a serious autonomous agent runtime for Tizen and embedded
Linux, not just a sample app. Its strengths are persistence, explicit platform
boundaries, flexible access surfaces, and unusually practical remote coding
control through Telegram plus local coding-agent CLIs.
