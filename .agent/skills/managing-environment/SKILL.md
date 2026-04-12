---
name: managing-environment
description: Enforces stable and non-blocking shell command execution
strategies across the WSL & Samba shared boundaries, preventing daemon
deadlocks. Primary context is a direct WSL Ubuntu shell; Windows
PowerShell invocation is the edge case.
---

# Managing Environment Skill

## Shell Context

**Primary context**: The agent runs **directly inside a WSL Ubuntu shell**.
Commands are executed as plain bash — no `wsl -e bash -c "..."` wrapper
is needed.

**Edge case**: If the agent is invoked from a Windows PowerShell session
(e.g., an IDE running on the Windows host), all Linux project commands
must be wrapped with `wsl -e bash -c "..."`.

Before executing any command, apply the decision matrix in
[`shell-detection.md`](../../rules/shell-detection.md) to confirm the
active shell context.

> [!IMPORTANT]
> **Shell Detection Rule**: The `.agent/rules/shell-detection.md` decision
> matrix is authoritative. Read it first whenever the shell context is
> ambiguous.

---

## 1. Direct Execution (WSL Ubuntu Shell — Normal Case)

Run all project commands directly without a wrapper:

```bash
# Default host build and deploy
./deploy_host.sh

# Run tests
./deploy_host.sh --test

# Explicit Tizen path (only on user request)
./deploy.sh -d emulator-26101

# Git operations
git status
git commit -F .tmp/commit_msg.txt
git push origin develRust
```

---

## 2. WSL Wrapper (PowerShell — Edge Case)

When invoked from Windows PowerShell, wrap every Linux command:

```powershell
wsl -e bash -c "cd /home/hjhun/samba/github/tizenclaw && ./deploy_host.sh"
wsl -e bash -c "cd /home/hjhun/samba/github/tizenclaw && ./deploy_host.sh --test"
wsl -e bash -c "cd /home/hjhun/samba/github/tizenclaw && git status"
```

---

## 3. Sequential Execution — No Background Sub-processes

Spawning background sub-shells (`nohup` or `&`) across the WSL/Samba
boundary causes I/O lockups regardless of the originating shell.

**Rule**: Always run heavy build/deploy commands synchronously in the
foreground. Wait for completion before starting the next task.

```bash
# ✅ Correct — foreground, sequential
./deploy_host.sh
./deploy_host.sh --test

# ❌ Wrong — background sub-shell risks I/O lockup
./deploy_host.sh &
```

---

## 4. I/O Timeouts for Heavy Filesystem Operations

Commands traversing large Git object trees or deleting build outputs
can hang on the WSL/Samba boundary.

Use `timeout` for safety on heavy filesystem operations:

```bash
timeout 30s git status
timeout 60s bash .agent/scripts/cleanup_workspace.sh
```

---

## 5. Pager and Prompt Suppression

Long-running commands that trigger interactive pagers will freeze
automated sessions.

```bash
# Disable git pager
GIT_PAGER=cat git log
git --no-pager log -10

# Non-interactive package installs
DEBIAN_FRONTEND=noninteractive apt-get install -y <packages>
```

---

## 6. Windows-Native Tools from WSL

Invoke Windows executables from inside WSL using the `.exe` suffix:

```bash
explorer.exe .            # open Windows Explorer at current directory
powershell.exe -c "..."   # run a one-off PowerShell command
```

---

## 7. Subprocess Cleanup and Orphan Process Handling

If a build or `sdb` emulator task crashes, orphaned processes can lock
the daemon test environment. Use `kill` routines to clear suspended tasks
before restarting.
