# TizenClaw Usage Guide

## Purpose

This guide explains how to build, deploy, operate, and inspect TizenClaw using
the repository's supported workflow. It is written for developers and operators
who want to use the current Rust workspace as an embedded agent runtime rather
than treat it as a generic local Rust application.

## Operating Model

TizenClaw is designed to run as a long-lived daemon. The normal flow is:

1. build the Tizen packages through `deploy.sh`
2. deploy to an emulator or device
3. restart the service
4. interact through the CLI, dashboard, or configured channels

The repository workflow is intentionally deployment-oriented. For this project,
the important validation path is the target-oriented one, not a detached local
host loop.

## Prerequisites

You should have the following available before you begin:

- Tizen Studio tooling with `sdb`
- Tizen GBS build support
- a reachable Tizen emulator or physical device
- a shell environment that can run repository scripts

It also helps to know the target device serial if more than one emulator or
device is connected.

## Core Command: `deploy.sh`

The root `deploy.sh` script is the operational entry point for the project.
According to the script header and option parser, common commands include:

```bash
./deploy.sh -a x86_64
./deploy.sh -a x86_64 -n
./deploy.sh -a x86_64 -i -n
./deploy.sh -a x86_64 -d <device-serial>
./deploy.sh -a x86_64 -s
./deploy.sh --dry-run
```

### What the script handles

- prerequisite checks
- architecture selection
- GBS build orchestration
- package deployment through `sdb`
- service restart steps

### Common flags

- `-a, --arch <arch>`: choose the build architecture
- `-n, --noinit`: reuse the build environment for faster iteration
- `-i, --incremental`: request the faster iterative build path
- `-s, --skip-build`: deploy existing artifacts without rebuilding
- `-S, --skip-deploy`: build without deploying
- `-d, --device <serial>`: target a specific emulator or device
- `--dry-run`: print the planned commands without executing them

## Standard Development Deployment Flow

For the common emulator-oriented path:

```bash
./deploy.sh -a x86_64
```

For a faster rebuild after making changes:

```bash
./deploy.sh -a x86_64 -n
```

If you already have a build and only need to push it again:

```bash
./deploy.sh -a x86_64 -s
```

If you need to pin deployment to a specific target:

```bash
./deploy.sh -a x86_64 -d emulator-26101
```

## Service Lifecycle

After deployment, TizenClaw runs as a device service. In practice, useful
checks usually include:

- verifying that the main daemon is active
- confirming the dashboard process is available
- checking that the tool executor socket is listening

The exact commands depend on your environment, but the project workflow and
internal notes regularly use device-side service inspection through `sdb shell`
plus `systemctl` or log inspection tools.

## Using the CLI

The CLI is the most direct operator surface for the daemon.

### Send a prompt

```bash
tizenclaw-cli "What is the current system status?"
```

### Stream a response

```bash
tizenclaw-cli --stream "Explain the active channels"
```

### Use interactive mode

```bash
tizenclaw-cli
```

### Manage the dashboard channel

```bash
tizenclaw-cli dashboard start
tizenclaw-cli dashboard start --port 9091
tizenclaw-cli dashboard stop
tizenclaw-cli dashboard status
```

## Using the Web Dashboard

The standalone dashboard binary serves both the UI and HTTP API.

Based on the current code:

- Tizen runtime default port: `9090`
- non-Tizen host default port: `8080`

The dashboard binary accepts runtime options such as:

```bash
tizenclaw-web-dashboard --port 9090
tizenclaw-web-dashboard --web-root <path>
tizenclaw-web-dashboard --config-dir <path>
tizenclaw-web-dashboard --data-dir <path>
tizenclaw-web-dashboard --localhost-only
```

In normal deployments the daemon or deployment flow is expected to manage the
dashboard lifecycle for you, but the flags are useful for debugging and custom
bring-up.

## Runtime Paths and Data

The codebase uses runtime path detection so the daemon can behave sensibly on
Tizen and non-Tizen environments.

Examples of what gets stored under runtime-managed directories include:

- logs
- sessions
- tasks
- outbound dashboard message queues
- web dashboard assets and app data

When debugging environment-specific issues, confirm which data and config
directories were resolved at startup.

## Configuration Touchpoints

The source tree and dashboard service indicate several configuration files and
surfaces, including:

- LLM configuration
- channel configuration
- tool policy configuration
- agent role configuration
- tunnel and web search configuration

Operators should treat those files as part of the deployed runtime contract,
especially when reproducing issues between emulator, host, and device setups.

## Extension Model

TizenClaw supports extension through a mix of runtime modules and metadata
plugins.

Important extension paths include:

- built-in LLM backend modules
- plugin-managed LLM backend metadata
- skill metadata plugins
- CLI plugin metadata
- tool execution through the sidecar

This split keeps the daemon core responsible for orchestration while allowing
new behaviors to be described or loaded through narrower extension points.

## Troubleshooting Checklist

If the daemon does not behave as expected, start with these checks:

1. Confirm the target is reachable through `sdb`.
2. Re-run `./deploy.sh -a x86_64` or `./deploy.sh -a x86_64 -n`.
3. Verify the main service restarted successfully.
4. Check the dashboard port and whether the dashboard process is alive.
5. Use `tizenclaw-cli dashboard status` to confirm dashboard state.
6. Inspect device logs for daemon boot failures or configuration issues.

## Recommended Reading Order

To go deeper after using the project:

1. [README.md](../README.md)
2. [STRUCTURE.md](STRUCTURE.md)
3. `deploy.sh`
4. `src/tizenclaw/src/main.rs`
5. `src/tizenclaw-cli/src/main.rs`
6. `src/tizenclaw-web-dashboard/src/main.rs`
