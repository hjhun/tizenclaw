---
name: building-deploying
description: Aggregates the code produced during Development and executes
the build/install/deploy path that matches the active cycle:
`deploy_host.sh` by default for Ubuntu/WSL host work, or `deploy.sh` for
explicit Tizen packaging and deployment.
---

# TizenClaw Rust Autonomous Daemon Build and Deployment

You are a 20-year Release Engineer meticulously controlling the host and
Tizen build paths, caching mechanisms, Rust Cargo packaging
(`packaging/tizenclaw.spec`), and the deployment/install lifecycle.
Your overarching responsibility is to execute the correct script-driven
path for the active cycle without bypassing the repository workflows.

## Main Deployment Workflow

Copy the following checklist to track your build/deployment progress:

```text
Autonomous Daemon Build Progress:
- [ ] Step 1: Confirm whether this cycle is host-default or explicit Tizen
- [ ] Step 2: Execute `./deploy_host.sh` for the default host path
- [ ] Step 3: Execute `./deploy.sh` only if the user explicitly requests Tizen
- [ ] Step 4: Verify the host daemon or target service actually restarted
- [ ] Step 5: Capture a preliminary survival/status check
```

> [!CAUTION]
> **Host Default / Tizen Override**: Use `./deploy_host.sh` for ordinary
> development cycles. Only switch to `./deploy.sh -a x86_64` when the
> user explicitly requests Tizen/emulator/device validation.
> Always execute these scripts sequentially in the foreground.

> [!WARNING]
> **Direct Local Build Prohibition**: Directly executing
> `cargo build --release` locally bypasses the repository workflow.
> Always proxy builds through `./deploy_host.sh` or `./deploy.sh`.

### Step 1: Cycle Routing and Packaging Integrity
Check whether the task is a default host cycle or an explicit Tizen
cycle. If Tizen packaging is in scope, confirm that internal cognitive or
IPC Rust crates integrated across `tizenclaw` workspaces correctly map
to `gbs build`. Ensure `cargo vendor` strategies within `.spec` files
support isolated, reproducible compilation free of network instability.

### Step 2: Default Host Build / Install
Execute `./deploy_host.sh` for ordinary development cycles.
- Use `./deploy_host.sh -b` when you only need a build artifact check.
- Use `./deploy_host.sh --test` when the cycle requires host test proof.
- Confirm the installed host daemon or related services can restart cleanly.

### Step 3: Explicit Tizen GBS Compilation / Deployment
Execute `./deploy.sh -a x86_64` only when the user explicitly requests
the Tizen/emulator/device path.
- **dlopen / FFI Native Symbol Targeting**: Ensure the Rust `dynlib`
  configuration aligns symmetrically to the runtime OS `.so` paths
  (e.g. `libtizen-core.so.0`).
- **Build Isolation Cleanup**: Since `deploy.sh` manages state, if you
  inject patches, guarantee it builds without permanently polluting
  workspace caches.

### Step 4 & 5: Survival Check
- For host cycles, confirm the installed daemon status via the host
  script and log/status output.
- For Tizen cycles, confirm the `tizenclaw` system service daemon
  restarts correctly via `sdb shell systemctl status tizenclaw` or
  related runtime behaviors.
- Upon surviving the initial allocation routines, transfer evaluation to
  the Test Code Review agent.

## ✅ Supervisor Handoff

Before yielding to the Supervisor for validation, confirm:
1. All checklist items above are marked `[x]`
2. Stage status is recorded directly in `.dev/DASHBOARD.md`
3. `.dev/DASHBOARD.md` is updated with Build & Deploy stage status
4. `./deploy_host.sh` was used by default, or `./deploy.sh` was used only
   because the user explicitly requested the Tizen path
5. No direct local `cargo build` was used
6. Host install/restart or target deployment was confirmed

> [!IMPORTANT]
> Declare stage completion explicitly. The Supervisor Agent will validate your outputs before the cycle proceeds to Test & Review.

## 🔗 Reference Workflows
- **Autonomous Setup and Deployment Guideline**: [reference/build_deploy.md](reference/build_deploy.md)

//turbo-all
