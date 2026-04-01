# Planning: Reinstall & CLI Tool Inspection

**Date**: 2026-04-01
**Cycle Type**: Build/Deploy + Test-Only (No code changes)

---

## Objective

Reinstall the TizenClaw package on the x86_64 emulator and perform a comprehensive
inspection of all CLI tools that our project ships.

## Scope

### Binaries Shipped by Our Package (from `packaging/tizenclaw.spec`)

**Rust Daemon & CLI:**
1. `tizenclaw` — Main AI Agent daemon
2. `tizenclaw-cli` — User-facing CLI for interacting with the daemon
3. `tizenclaw-tool-executor` — Socket-activated tool execution service

**C-based Native CLI Tools (from `tools/cli/CMakeLists.txt`):**
Currently only 4 tools have `ADD_SUBDIRECTORY` entries (built):
4. `tizen-network-info-cli` — Network, Wi-Fi, Bluetooth, data usage
5. `tizen-notification-cli` — Send notifications, schedule alarms
6. `tizen-file-manager-cli` — File system operations (read/write/list/stat/copy/move)
7. `tizen-vconf-cli` — Read/write/watch vconf system settings

**Defined but NOT built (targets declared, no ADD_SUBDIRECTORY):**
- `tizen-control-display-cli`
- `tizen-device-info-cli`
- `tizen-hardware-control-cli`
- `tizen-sound-cli`
- `tizen-app-manager-cli`
- `tizen-sensor-cli`
- `tizen-web-search-cli`
- `tizen-media-cli`

### Execution Mode Classification
- `tizenclaw` — **Daemon Sub-task** (persistent systemd service)
- `tizenclaw-cli` — **One-shot Worker** (sends request, receives response, exits)
- `tizenclaw-tool-executor` — **Daemon Sub-task** (socket-activated on-demand service)
- All `tizen-*-cli` tools — **One-shot Worker** (execute single command, return JSON, exit)

## Verification Plan

1. **Build & Deploy**: Run `./deploy.sh -a x86_64` to rebuild and reinstall the full package
2. **Service Verification**: Check `tizenclaw` and `tizenclaw-tool-executor` service status
3. **CLI Tool Inspection**: Execute each of our shipped CLI tools with sample commands
4. **Gap Analysis**: Identify which tools in `tools.md` / `index.md` are listed but not actually built

## Key Finding from CMakeLists.txt Analysis

The `tools/cli/CMakeLists.txt` defines 12 target variables, but only 4 have `ADD_SUBDIRECTORY()` calls:
- `tizen-network-info-cli` ✅
- `tizen-notification-cli` ✅
- `tizen-file-manager-cli` ✅
- `tizen-vconf-cli` ✅

**8 tools are declared but NOT being built.** This is a significant gap between documentation (12 tools) and reality (4 tools + documentation directories).
