# TizenClaw Automated Test Suite

End-to-end test automation framework for TizenClaw. Runs against real devices
(emulators, TVs, refrigerators, etc.) via `sdb` connection.

## Quick Start

```bash
# Run all test suites
./tests/run_all.sh

# Target a specific device
./tests/run_all.sh -d <device-serial>

# Run a specific suite
./tests/run_all.sh -s cli_tools

# Run multiple suites
./tests/run_all.sh -s service,mcp,regression

# List available suites
./tests/run_all.sh --list

# Run a single test file
./tests/cli_tools/test_device_info.sh -d <device-serial>
```

## Prerequisites

1. **Device connected** — Verify with `sdb devices`
2. **TizenClaw deployed** — Run `./deploy.sh` first
3. **Service running** — `sdb shell systemctl is-active tizenclaw` → `active`
4. **jq installed** (host) — Required for MCP and JSON assertion tests
   ```bash
   sudo apt-get install jq
   ```

## Directory Structure

```
tests/
├── run_all.sh                     # Master runner
├── lib/
│   └── test_framework.sh          # Shared assertion & utility library
├── service/
│   └── test_service.sh            # Daemon health & infrastructure
├── cli_tools/
│   ├── test_app_manager.sh        # App list, launch, terminate
│   ├── test_aurum.sh              # UI automation (screen, elements)
│   ├── test_device_info.sh        # Battery, CPU, storage, thermal
│   ├── test_display.sh            # Brightness control
│   ├── test_file_manager.sh       # File CRUD operations
│   ├── test_hardware.sh           # Haptic, LED, power lock
│   ├── test_media.sh              # Media DB query, MIME types
│   ├── test_network.sh            # WiFi, BT, network status
│   ├── test_notification.sh       # Send notifications
│   ├── test_sensor.sh             # Accelerometer, light, proximity
│   ├── test_sound.sh              # Volume control, audio devices
│   ├── test_vconf.sh              # VConf key read/write
│   └── test_web_search.sh         # Web search API
├── embedded_tools/
│   ├── test_code_execution.sh     # Python code execution
│   ├── test_pipeline.sh           # Pipeline CRUD
│   ├── test_session.sh            # Session management
│   ├── test_task.sh               # Task management
│   └── test_workflow.sh           # Workflow CRUD
├── llm_integration/
│   ├── test_prompt_response.sh    # Basic prompt/response
│   ├── test_streaming.sh          # Streaming mode
│   └── test_tool_invocation.sh    # LLM-driven tool calls
├── mcp/
│   └── test_mcp_protocol.sh       # MCP JSON-RPC compliance
└── regression/
    └── test_known_issues.sh        # Crash resilience, edge cases
```

## Test Suites

### `service` — Daemon Health
Checks service status, binary installation, IPC socket, tool loading, work
directories, restart resilience, and web dashboard access.

### `cli_tools` — CLI Tool Validation
Tests each CLI tool binary directly on the device. Validates JSON output
structure, correct data fields, and CRUD operations (for file manager).
Gracefully skips tests when hardware is unavailable (e.g., sensors on emulator).

### `embedded_tools` — Embedded Tool Operations
Tests session management, workflow CRUD, pipeline CRUD, task management,
and code execution through `tizenclaw-cli` prompts.

### `llm_integration` — LLM Agent Tests
Validates the full agentic loop: natural language prompt → LLM reasoning →
tool invocation → response. Tests Korean/English prompts, multi-tool calls,
streaming mode, and error handling.

### `mcp` — MCP Protocol Compliance
Validates MCP JSON-RPC 2.0 protocol: `initialize`, `tools/list`, error codes,
malformed input, notifications, and edge cases.

### `regression` — Regression & Stability
Tests for crash resilience under rapid calls, concurrent sessions, empty
prompts, Unicode, special characters, and memory usage monitoring.

## Options

| Flag | Description |
|------|-------------|
| `-d, --device <serial>` | Target a specific device (from `sdb devices`) |
| `-s, --suite <names>` | Comma-separated suite names to run |
| `-v, --verbose` | Enable verbose log output |
| `-t, --timeout <seconds>` | Per-command timeout (default: 30) |
| `--list` | List available test suites |

## Writing New Tests

1. Create a new `test_<feature>.sh` file in the appropriate suite directory
2. Source the framework:
   ```bash
   SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
   source "${SCRIPT_DIR}/../lib/test_framework.sh"
   tc_parse_args "$@"
   tc_preflight
   ```
3. Use `suite_begin` / `section` / assertions / `suite_end`
4. Available assertions:
   - `assert_contains`, `assert_not_contains`
   - `assert_not_empty`, `assert_empty`
   - `assert_eq`, `assert_ne`, `assert_ge`, `assert_le`
   - `assert_file_exists`, `assert_dir_exists`
   - `assert_json_valid`, `assert_json`, `assert_json_eq`
   - `assert_json_array_ge`
5. Device helpers:
   - `sdb_shell` — remote shell command
   - `cli_exec <tool> <args>` — execute a CLI tool
   - `tc_cli <prompt>` — send prompt to tizenclaw-cli
   - `tc_cli_session <id> <prompt>` — with session
   - `tc_device_profile` — detect TV/mobile/wearable
   - `tc_tool_exists <path>` — check binary on device

## Device Profiles

Tests automatically detect the device profile and skip hardware-specific
tests on unsupported devices:

| Profile | Example Devices |
|---------|----------------|
| `tv` | Samsung Smart TV |
| `mobile` | Tizen Mobile Emulator |
| `wearable` | Galaxy Watch |
| `iot` | Smart Refrigerator, etc. |

## CI Integration

The master runner returns exit code `0` only when all suites pass,
making it suitable for CI pipelines:

```yaml
# Example CI step
- name: E2E Tests
  run: |
    sdb connect $DEVICE_IP
    ./tests/run_all.sh -d $DEVICE_SERIAL
```

## Relationship to Existing Tests

| Location | Type | Purpose |
|----------|------|---------|
| `test/unit_tests/` | gtest (C++) | Unit tests — run during `gbs build` |
| `test/e2e/` | Shell | Legacy E2E smoke/MCP tests |
| **`tests/`** | **Shell** | **Comprehensive E2E automation (this framework)** |
