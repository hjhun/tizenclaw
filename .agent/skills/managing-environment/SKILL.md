---
name: managing-environment
description: Enforces stable and non-blocking shell command execution strategies securely mapping the TizenClaw continuous integration workflows across the WSL & Samba shared boundaries natively preventing daemon deadlocks.
---

# Managing Environment Skill

**Environment Context:** You are operating in a **Windows Antigravity**
environment, and you MUST access **Ubuntu via WSL (Windows Subsystem
for Linux)** to execute any system, build, or deployment commands. You
are NOT in a native Linux host.

> [!IMPORTANT]
> **Shell Detection Rule**: Before executing ANY terminal command, consult the [`shell-detection.md`](../../rules/shell-detection.md) rule to determine whether to use PowerShell or WSL Bash. The decision matrix in that rule is authoritative.

This architectural skillset establishes stringent constraints isolating the continuous compilation tasks safely natively inside the WSL filesystem. Because the workspace spans Windows and WSL environments (via Samba or WSL mounts), you **MUST** respect the following access constraints ensuring target isolation:

## 1. Mandatory WSL Shell Wrapper
Since Antigravity natively executes in Windows PowerShell, executing
Linux commands directly (e.g., `./deploy_host.sh`, `./deploy.sh`,
`gbs build`) will fail fatally or hang the agent loop.
* **Rule**: ALL terminal commands targeting the Tizenclaw daemon MUST be
  wrapped explicitly under WSL.
* **Pattern**: Always use the format `wsl -e bash -c "..."`.
  - Default host development example:
    `wsl -e bash -c "./deploy_host.sh"`
  - Explicit Tizen example:
    `wsl -e bash -c "./deploy.sh -a x86_64"`

## 2. Sequential Constraints for GBS and Cargo (No Background Sub-processes)
When triggering intensive build layers generating large object files
(e.g., `wsl -e bash -c "./deploy_host.sh --test"` or
`wsl -e bash -c "./deploy.sh -a x86_64"`), execute synchronously
strictly natively blocking.
* **Rule**: Spawning background sub-shells (`nohup` or `&`) crushes the I/O interface between WSL and Windows. Isolate long builds continuously foregrounded.
* **Pattern**: Chain them blocking inherently: run
  `wsl -e bash -c "./deploy_host.sh --test"` or
  `wsl -e bash -c "./deploy.sh -a x86_64"` and wait until completion
  before spawning the next task.

## 2. Mandatory I/O Timeouts Preventing Native Locking
Commands mapping immense Git object trees or stripping target outputs (`rm -rf`) generate filesystem hang vulnerabilities bridging Windows to WSL.
* **Rule**: Protect heavy abstraction traversals using standard `timeout` wrapper implementations natively.
* **Pattern**: Implement resilient file checks `timeout 30s git status`, `timeout 60s bash .agent/scripts/cleanup_workspace.sh`.
* **Resolution Guidelines**: Upon trigger, isolate standard resource locks or restart natively observing isolation constraints accurately natively.

## 3. Silencing Pagers and Suppressing Prompt Freezes
Background automation processing immense logs (`gbs build`, Git hashes, D-Bus polling traces natively) can indefinitely freeze the Antigravity session requesting user output terminal rendering internally.
* **Rule**: Assert environments disable arbitrary pagination tools natively forcing clean STDOUT streaming dynamically bypassing prompt blockers.
* **Pattern**: 
  - Git extraction pipelines: `GIT_PAGER=cat git log` or `git --no-pager [command]`
  - Package dependencies: `DEBIAN_FRONTEND=noninteractive apt-get install -y [packages]`

## 4. Subprocess Cleanup and Orphan Process Handling
If your script crashes extracting the Daemon natively via `sdb`, isolated emulator tasks natively spawn detached zombie states locking critical communication traits locally natively avoiding memory leaks continuously. Execute active `kill` routines clearing suspended tasks confirming daemon test environments regenerate faithfully verifying agent state transitions purely objectively continuously.
