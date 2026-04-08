# Shell Detection & Adaptive Command Execution Rule

When executing terminal commands, the agent **must** detect the current shell environment and use the appropriate command syntax. This project operates in a **Windows + WSL hybrid** environment.

## Shell Environment Detection

The AI agent's `run_command` tool executes commands via **PowerShell (pwsh)** on the Windows host.
To run Linux/bash commands, they **must** be wrapped with `wsl -e bash -c "..."`.

### Decision Matrix

| Target | Shell | Command Pattern | Example |
|--------|-------|-----------------|---------|
| Linux filesystem / build / deploy | Bash (via WSL) | `wsl -e bash -c "..."` | `wsl -e bash -c "./devel_host.sh"` |
| Linux file content (cat, grep, find) | Bash (via WSL) | `wsl -e bash -c "..."` | `wsl -e bash -c "cat Cargo.toml"` |
| Git operations on WSL repo | Bash (via WSL) | `wsl -e bash -c "..."` | `wsl -e bash -c "git status"` |
| Windows-native tools (explorer, notepad) | PowerShell | Direct command | `explorer.exe .` |
| PowerShell-specific operations | PowerShell | Direct command | `Get-ChildItem` |
| WSL path translation | PowerShell | Direct command | `wsl wslpath -u "C:\path"` |

## Rules

### 1. Default to WSL Bash for All Project Commands
Since the project lives entirely on the WSL filesystem (`\\wsl.localhost\Ubuntu\...`), **all project-related commands** (build, test, git, file manipulation) must use the WSL wrapper.

```powershell
# ✅ Correct — WSL wrapper, host-default path
wsl -e bash -c "cd /home/hjhun/samba/github/tizenclaw && ./devel_host.sh"

# ❌ Wrong — Direct PowerShell
./devel_host.sh
```

### 2. Working Directory Handling
PowerShell's `Cwd` parameter accepts UNC paths (`\\wsl.localhost\Ubuntu\...`), but many Linux tools do not understand them. Always use the `cd` inside the bash command or set the Cwd to the UNC path and let WSL handle the translation.

```powershell
# Option A: Explicit cd inside bash
wsl -e bash -c "cd /home/hjhun/samba/github/tizenclaw && git status"

# Option B: Use Cwd with WSL-aware path (preferred when Cwd is set)
# Cwd: \\wsl.localhost\Ubuntu\home\hjhun\samba\github\tizenclaw
wsl -e bash -c "git status"
```

### 3. Quoting and Escaping
When wrapping bash commands inside PowerShell's `wsl -e bash -c`, be careful with quoting:

```powershell
# Single quotes inside double quotes
wsl -e bash -c "echo 'hello world'"

# Escape double quotes if needed inside the command
wsl -e bash -c "grep \"pattern\" file.txt"

# Use single-quote wrapper for complex commands
wsl -e bash -c 'find . -name "*.rs" -exec grep -l "TODO" {} +'
```

### 4. Environment Variables
PowerShell and Bash handle environment variables differently:

```powershell
# ✅ Bash env vars inside WSL wrapper
wsl -e bash -c "export PAGER=cat && git log -5"

# ❌ Wrong — PowerShell syntax won't work in bash
wsl -e bash -c "$env:PAGER='cat'; git log -5"
```

### 5. PowerShell-Only Operations
The following are the **only** cases where direct PowerShell commands are appropriate:

- Checking Windows system state (`Get-Process`, `Get-Service`)
- Opening Windows GUI apps (`explorer.exe`, `code`)
- Managing WSL itself (`wsl --list`, `wsl --shutdown`)
- Windows path/registry operations

## Quick Reference

```
Project command?
├── YES → wsl -e bash -c "..."
│   ├── Host default → wsl -e bash -c "./devel_host.sh ..."
│   ├── Tizen on demand → wsl -e bash -c "./deploy.sh ..."
│   ├── Git → wsl -e bash -c "git ..."
│   ├── File ops → wsl -e bash -c "cat/grep/find ..."
│   └── Scripts → wsl -e bash -c "bash script.sh"
└── NO (Windows-native) → Direct PowerShell command
```
