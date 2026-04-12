# Shell Detection & Adaptive Command Execution Rule

This project runs on a **Windows + WSL hybrid** machine, but the primary
development session operates **directly inside the WSL Ubuntu shell**.
The correct command pattern depends on where the agent was invoked from.

## Detecting the Current Shell Context

Before executing any terminal command, determine the active shell:

| Signal | Conclusion |
|--------|-----------|
| `$PSVersionTable` is defined, or `$env:OS` is `Windows_NT` | PowerShell (Windows host) |
| `$SHELL` contains `bash`/`zsh`, or `uname` returns `Linux` | WSL Ubuntu shell (direct) |
| Working directory starts with `\\wsl.localhost\` or `C:\` | PowerShell (Windows host) |
| Working directory is a POSIX path (`/home/...`) | WSL Ubuntu shell (direct) |

## Decision Matrix

| Active shell | Target | Command pattern | Example |
|-------------|--------|-----------------|---------|
| **WSL Ubuntu (direct)** | Any project command | Run directly | `./deploy_host.sh` |
| **WSL Ubuntu (direct)** | Windows-native tool | Use `.exe` suffix | `explorer.exe .` |
| **PowerShell (Windows)** | Linux/project command | `wsl -e bash -c "..."` | `wsl -e bash -c "./deploy_host.sh"` |
| **PowerShell (Windows)** | Windows-native tool | Direct command | `Get-ChildItem` |

## Primary Path — WSL Ubuntu Shell (Direct)

**This is the normal operating context for TizenClaw development.**
No wrapper is needed. Run all project commands directly:

```bash
# Build and deploy (host default)
./deploy_host.sh

# Run tests
./deploy_host.sh --test

# Git operations
git status
git commit -F .tmp/commit_msg.txt

# Tizen path (explicit request only)
./deploy.sh -d emulator-26101
```

## Edge Case — Invoked from Windows PowerShell

When the agent session originates from Windows (e.g., an IDE running on the
Windows host, or `pwsh.exe` directly), Linux commands must be wrapped:

```powershell
# ✅ Correct — WSL wrapper required from PowerShell
wsl -e bash -c "cd /home/hjhun/samba/github/tizenclaw && ./deploy_host.sh"

# ❌ Wrong — Direct PowerShell execution will fail
./deploy_host.sh
```

### Quoting inside the WSL wrapper

```powershell
# Single quotes inside double quotes
wsl -e bash -c "echo 'hello world'"

# Escape double quotes if needed
wsl -e bash -c "grep \"pattern\" file.txt"

# Complex commands — use single-quote wrapper
wsl -e bash -c 'find . -name "*.rs" -exec grep -l "TODO" {} +'
```

### Working directory from PowerShell

```powershell
# Option A: explicit cd inside bash
wsl -e bash -c "cd /home/hjhun/samba/github/tizenclaw && git status"

# Option B: UNC Cwd + bare command (WSL resolves the path)
# Cwd: \\wsl.localhost\Ubuntu\home\hjhun\samba\github\tizenclaw
wsl -e bash -c "git status"
```

## Sequential Execution (Both Contexts)

Heavy build commands (`./deploy_host.sh`, `./deploy.sh`) must run
**synchronously in the foreground** — never background them with `nohup`
or `&`. Background sub-shells across the WSL/Samba boundary cause I/O
lockups regardless of the originating shell.

## Pager and Prompt Suppression

Long-running commands that produce paged output must suppress interactive
prompts:

```bash
# Disable pager for git
GIT_PAGER=cat git log
git --no-pager log

# Non-interactive apt (if needed)
DEBIAN_FRONTEND=noninteractive apt-get install -y <packages>
```

## Windows-Native Tools from WSL

Calling Windows executables from inside WSL requires the `.exe` suffix:

```bash
explorer.exe .          # open Windows Explorer
code .                  # VS Code (if on Windows PATH)
powershell.exe -c "..."  # run a one-off PowerShell command
```

## Quick Reference

```
Determine active shell first:
├── WSL Ubuntu (direct) — NORMAL CASE
│   ├── Project commands → run directly: ./deploy_host.sh, git, etc.
│   └── Windows tools → use .exe suffix: explorer.exe, code
└── PowerShell (Windows) — EDGE CASE
    ├── Project commands → wsl -e bash -c "..."
    └── Windows tools → direct PowerShell command
```
