---
description: WSL Development Environment Guide
---

# WSL Development Environment Guide

The TizenClaw project is developed in a WSL (Ubuntu) environment. There are two development methods.

---

## Method 1: Direct Development in WSL Terminal

Run commands directly from the WSL terminal (bash).
All commands behave as they would in a native Linux environment. This is the most natural and recommended method.

### Environment Setup
- `sdb`, `gbs`, etc. must have their PATH configured in `~/.bashrc` or `~/.profile`.
  ```bash
  # ~/.bashrc example
  export PATH=$HOME/bin:$HOME/tizen-studio/tools:$PATH
  ```

### Build and Deploy
```bash
cd /home/hjhun/samba/github/tizenclaw
./deploy.sh
```

### CLI Testing
```bash
sdb shell tizenclaw-cli "your prompt here"
sdb shell tizenclaw-cli --stream "Tell me about Tizen"
```

### Log Inspection
```bash
sdb shell dlogutil TIZENCLAW TizenClawWebView -d -v time | tail -20
```

> [!TIP]
> All output is displayed correctly in the WSL terminal. This method is recommended whenever possible.

---

## Method 2: Development via WSL from Windows PowerShell

Execute WSL commands remotely from Windows PowerShell or an agent environment.

### Command Execution Pattern
You **must** use the `bash -lic` flags:
```powershell
wsl -d Ubuntu -- bash -lic "<command>"
```

| Flag | Purpose |
|---|---|
| `-l` (login) | Loads `~/.profile`, `~/.bashrc` → ensures `sdb`, `gbs`, `dirname`, etc. are in PATH |
| `-i` (interactive) | Loads interactive shell settings |
| `-c` (command) | Executes the following string as a command |

> [!CAUTION]
> Without `-lic`, commands will fail with errors like `dirname: command not found` or `sdb: not found`.

### Build and Deploy
```powershell
wsl -d Ubuntu -- bash -lic "cd /home/hjhun/samba/github/tizenclaw && ./deploy.sh"
```

### Reading Files (UNC Path)
To access WSL files from Windows tools:
```powershell
# Read directly from PowerShell
Get-Content "\\wsl.localhost\Ubuntu\home\hjhun\samba\github\tizenclaw\filename"

# Or copy to a Windows path first, then read
Copy-Item "\\wsl.localhost\Ubuntu\...\file" -Destination "C:\tmp\file"
```

### Known Limitations

#### Cannot Capture `sdb shell` Output
`sdb shell` uses a PTY internally, so the agent's `command_status` tool cannot capture stdout/stderr (displays `No output`).

**Workarounds:**
- **Detecting build completion**: Poll RPM file timestamps or check the build log directory
  ```bash
  ls -lt ~/GBS-ROOT/local/repos/tizen/${ARCH}/logs/success/ | head -5
  ls -lt ~/GBS-ROOT/local/repos/tizen/${ARCH}/logs/fail/ | head -5
  ```
- **CLI test results**: Cross-check with daemon logs
  ```bash
  sdb shell dlogutil -d TIZENCLAW | grep -i "ProcessPrompt\|tool\|workflow" | tail -20
  ```

#### `gbs build` Output Has the Same Issue
`gbs build` runs inside `sudo`/`chroot`, so the same limitation applies. See the **AGENT Only: How to Detect Build Completion** section in `gbs_build.md` for details.

// turbo-all
