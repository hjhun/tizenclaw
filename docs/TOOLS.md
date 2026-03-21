# TizenClaw Tools Reference

TizenClaw provides **13 native CLI tool suites** (C++ command-line executables replacing the legacy Python skills), and **20+ built-in tools** (native C++). All tools are registered in the **Capability Registry** with function contracts.

> **Anthropic Standard Compatibility**: TizenClaw's skill system fully implements the **Anthropic Standard Skill Format** (utilizing `SKILL.md`, YAML frontmatter, and JSON schema parameters). Furthermore, the core agent daemon features a built-in **Anthropic-compliant MCP Client** (Model Context Protocol) to seamlessly connect to external MCP tool servers, vastly expanding its capability profile.

> Container skills use `ctypes` FFI to call Tizen C-API directly. Async skills use the **tizen-core** event loop for callback-based APIs.

---

> **Note**: Legacy Python container skills have been rewritten and ported directly as native CLI tools for superior performance and native Tizen API access.

---

## Native CLI Tool Suites (C++)

### App Management

| Skill | Parameters | C-API | Description |
|-------|-----------|-------|-------------|
| `list_apps` | — | `app_manager` | List all installed applications |
| `send_app_control` | `app_id`, `operation`, `uri`, `mime`, `extra_data` | `app_control` | Launch app via explicit app_id or implicit intent (operation/URI/MIME) |
| `terminate_app` | `app_id` | `app_manager` | Terminate a running app |
| `get_package_info` | `package_id` | `package_manager` | Query package details (version, type, size) |

### Device Info & Sensors

| Skill | Parameters | C-API | Description |
|-------|-----------|-------|-------------|
| `get_device_info` | — | `system_info` | Model, OS version, platform info |
| `get_system_info` | — | `system_info` | Hardware details (CPU, screen, features) |
| `get_runtime_info` | — | `runtime_info` | CPU and memory usage statistics |
| `get_storage_info` | — | `storage` | Internal/external storage space |
| `get_system_settings` | — | `system_settings` | Locale, timezone, font, wallpaper |
| `get_sensor_data` | `sensor_type` | `sensor` | Accelerometer, gyroscope, light, proximity, etc. |
| `get_thermal_info` | — | `device` (thermal) | Device temperature (AP, CP, battery) |

### Network & Connectivity

| Skill | Parameters | C-API | Description |
|-------|-----------|-------|-------------|
| `get_wifi_info` | — | `wifi-manager` | Current WiFi connection details |
| `get_bluetooth_info` | — | `bluetooth` | Bluetooth adapter state |
| `get_network_info` | — | `connection` | Network type, IP address, status |
| `get_data_usage` | — | `connection` (statistics) | WiFi/cellular data usage stats |
| `scan_wifi_networks` | — | `wifi-manager` + **tizen-core** ⚡ | Scan nearby WiFi access points (async) |
| `scan_bluetooth_devices` | `action` | `bluetooth` + **tizen-core** ⚡ | Discover nearby BT devices or list bonded (async) |

### Display & Hardware Control

| Skill | Parameters | C-API | Description |
|-------|-----------|-------|-------------|
| `get_display_info` | — | `device` (display) | Brightness, state, max brightness |
| `control_display` | `brightness` | `device` (display) | Set display brightness level |
| `control_haptic` | `duration_ms` | `device` (haptic) | Vibrate the device |
| `control_led` | `action`, `brightness` | `device` (flash) | Camera flash LED on/off |
| `control_volume` | `action`, `sound_type`, `volume` | `sound_manager` | Get/set volume levels |
| `control_power` | `action`, `resource` | `device` (power) | Request/release CPU/display lock |

### Media & Content

| Skill | Parameters | C-API | Description |
|-------|-----------|-------|-------------|
| `get_battery_info` | — | `device` (battery) | Battery level and charging status |
| `get_sound_devices` | — | `sound_manager` (device) | List audio devices (speakers, mics) |
| `get_media_content` | `media_type`, `max_count` | `media-content` | Search media files on device |
| `get_metadata` | `file_path` | `metadata-extractor` | Extract media file metadata (title, artist, album, duration, etc.) |
| `get_mime_type` | `file_extension`, `file_path`, `mime_type` | `mime-type` | MIME type ↔ extension lookup |

### System Actions

| Skill | Parameters | C-API | Description |
|-------|-----------|-------|-------------|
| `play_tone` | `tone`, `duration_ms` | `tone_player` | Play DTMF or beep tones |
| `play_feedback` | `pattern` | `feedback` | Play sound/vibration patterns |
| `send_notification` | `title`, `body` | `notification` | Post notification to device |
| `schedule_alarm` | `app_id`, `datetime` | `alarm` | Schedule alarm at specific time |
| `download_file` | `url`, `destination`, `file_name` | `url-download` + **tizen-core** ⚡ | Download URL to device (async) |
| `web_search` | `query` | — (Wikipedia) | Web search via Wikipedia API |

> ⚡ = Async skill using **tizen-core** event loop (`tizen_core_task_create` → `add_idle_job` → `task_run` → callback → `task_quit`)

---

## Built-in Tools (AgentCore, Native C++)

| Tool | Description |
|------|-------------|
| `execute_code` | Execute Python code in sandbox |
| `file_manager` | Read/write/list files on device |
| `manage_custom_skill` | Create/update/delete/list custom skills at runtime |
| `create_task` | Create a scheduled task |
| `list_tasks` | List active scheduled tasks |
| `cancel_task` | Cancel a scheduled task |
| `create_session` | Create a new chat session |
| `list_sessions` | List active sessions |
| `send_to_session` | Send message to another session |
| `ingest_document` | Ingest document into RAG store |
| `search_knowledge` | Hybrid semantic search in RAG store (BM25 + vector RRF) |
| `execute_action` | Execute a Tizen Action Framework action |
| `action_<name>` | Per-action tools (auto-discovered from Action Framework) |
| `execute_cli` | Execute CLI tool plugins installed via TPK packages |
| `create_workflow` | Create a deterministic skill pipeline |
| `list_workflows` | List registered workflows |
| `run_workflow` | Execute a workflow pipeline |
| `remember` | Save information to persistent memory |
| `recall` | Search persistent memory by keyword |
| `forget` | Delete a specific memory entry |
| `delete_pipeline` | Delete a registered pipeline |
| `delete_workflow` | Delete a registered workflow |

### Tool Dispatch Architecture

Tool execution uses a modular `ToolDispatcher` class (`tool_dispatcher.cc`) extracted from `AgentCore` for independent testability:

- **O(1) Lookup**: `std::unordered_map<std::string, ToolHandler>` for registered tools
- **Dynamic Fallback**: `starts_with` matching for dynamically named tools (e.g., `action_*`)
- **Thread Safety**: `std::shared_mutex` for concurrent read access
- **Capability Integration**: All dispatched tools are registered in `CapabilityRegistry` with `FunctionContract`

---

## RPK Tool Distribution & Extensibility

TizenClaw's capability ecosystem extends beyond built-in tools via **Tizen Resource Packages (RPKs)**. This approach supersedes the legacy `manage_custom_skill` method by providing a structural delivery mechanism for enterprise environments.

An RPK tool package can contain:
1. **Sandboxed Python Skills**: New tools executed safely inside the OCI container.
2. **Host/Container CLI Tools**: Binary utilities or scripts to be invoked via `execute_action` or `execute_code`.

### Capability Registry (`capability_registry.cc`)
All dynamic RPK plugins, along with CLI tools and built-in skills, are registered in TizenClaw's unified **Capability Registry** (`CapabilityRegistry` singleton). This provides:
- Clear **Function Contracts** (`FunctionContract` struct with Input/Output JSON Schemas).
- Defined **Side Effects** (`SideEffect` enum: `kNone`, `kReversible`, `kIrreversible`, `kUnknown`).
- Retry policies and required Sandbox/Tizen (SMACK) permissions.
- **Category-based queries**: `GetByCategory()`, `GetBySideEffect()`, `GetByPermission()`.
- **LLM Integration**: `GetSummaryForLLM()` generates a category-grouped JSON summary injected via `{{CAPABILITY_SUMMARY}}` placeholder.

At startup, 21 built-in capabilities are automatically registered. Skills loaded from manifests are also registered with their declared contracts.

Once an RPK is installed via the system package manager (e.g. `pkgcmd`), TizenClaw automatically discovers and registers its capabilities, making them immediately available to the Planning Agent without daemon recompilation.

---

## CLI Tool Plugins (TPK-based)

In addition to Python skills, TizenClaw supports **native CLI tool plugins** packaged as TPKs (Tizen Packages). CLI tools run directly on the host for full Tizen C-API access, making them ideal for device queries that require privileged APIs.

### Architecture

| Component | Role |
|-----------|------|
| `CliPluginManager` | Discovers TPKs with `http://tizen.org/metadata/tizenclaw/cli` metadata, creates symlinks into `tools/cli/` |
| `tizenclaw-metadata-cli-plugin.so` | Parser plugin enforcing platform-level certificate signing at install |
| `execute_cli` (built-in tool) | Executes CLI tools via `popen()`, returns JSON output to LLM |
| `.tool.md` descriptors | Rich Markdown files injected into system prompt for LLM tool discovery |

### Tool Descriptor Format (`.tool.md`)

Each CLI tool ships a `.tool.md` file describing its commands, arguments, and output format. This enables the LLM to construct correct invocations:

```markdown
# get_package_info

**Category**: Package Management

Query Tizen package information.

## Commands

| Command | Description | Arguments |
|---------|-------------|-----------|
| `list` | List all packages | `--type <tpk\|wgt>` (optional) |
| `info` | Get package details | `--pkgid <id>` (required) |
```

### Manifest Declaration

CLI tools use `<service-application>` in `tizen-manifest.xml`:

```xml
<service-application appid="org.tizen.sample.get_package_info"
                     exec="get_package_info" type="capp">
    <metadata key="http://tizen.org/metadata/tizenclaw/cli"
              value="get_package_info"/>
</service-application>
```

> **Security**: Only platform-signed TPKs can register CLI tools.

### Built-in CLI Tool: `aurum-cli`

`aurum-cli` is a built-in native C++ CLI tool for **Aurum UI Automation**. It supports two backends:

| Mode | Usage | Backend |
|------|-------|---------|
| **Default** | `aurum-cli <cmd>` | Direct `libaurum` (AT-SPI2) |
| **gRPC** | `aurum-cli --grpc <cmd>` | `aurum-bootstrap` server (port 50051) |

**Subcommands** (22): `screen-size`, `screenshot`, `get-angle`, `device-time`, `click`, `flick`, `send-key`, `touch-down/move/up`, `mouse-down/move/up`, `find-element`, `find-elements`, `dump-tree`, `click-element`, `set-focus`, `do-action`, `set-value`, `wait-event`, `watch`

**Installation**: `/opt/usr/share/tizenclaw/tools/cli/aurum-cli/aurum-cli`

> gRPC mode requires `aurum-bootstrap` running on the device (`app_launcher -s org.tizen.aurum-bootstrap`). Use `--grpc-addr HOST:PORT` for custom addresses.

---

## Multi-Agent Ecosystem

TizenClaw utilizes a highly decentralized **11 MVP Agent Set** to manage requests and device states reliably:

| Category | Agent | Primary Responsibility |
|----------|-------|------------------------|
| **Understanding** | `Input Understanding Agent` | Standardizes user input across all 7 channels into a unified intent structure. |
| **Perception** | `Environment Perception Agent` | Subscribes to the Event Bus to maintain the Common State Schema. |
| **Memory** | `Session / Context Agent` | Manages working, long-term, and episodic memory Retrieval |
| **Planning** | `Planning Agent` | Decomposes goals into logical steps based on the Capability Registry. |
| **Execution** | `Action Execution Agent` | Invokes the actual OCI Container Skills and Action Framework commands. |
| **Protection** | `Policy / Safety Agent` | Intercepts plans prior to execution to enforce restrictions (e.g. constraints). |
| **Utility** | `Knowledge Retrieval Agent` | Interfaces with the SQLite RAG store for semantic lookups. |
| **Monitoring** | `Health Monitoring Agent` | Monitors memory pressure (PSS constraints) and container health. |
| | `Recovery Agent` | Analyzes structured failures and attempts error correction via the LLM. |
| | `Logging / Trace Agent` | Centralizes context for debugging and audit logs. |

Agents coordinate using the shared `Event Bus` and communicate via internal message passing. The *Planning Agent* serves as the primary gateway for translating user intents into executed actions based on real-time perception state.

---

## Async Pattern (tizen-core)

Skills marked with ⚡ use an async pattern for callback-based Tizen APIs:

```
tizen_core_init()
  → tizen_core_task_create("main", false)
    → tizen_core_add_idle_job(start_api_call)
    → tizen_core_add_timer(timeout_ms, safety_timeout)
    → tizen_core_task_run()          ← blocks until quit
      → API callback fires
        → collect results
        → tizen_core_task_quit()
  → return results
```

This enables Python FFI to use any callback-based Tizen C-API without threading.
