# TizenClaw Tools Reference — Python Port

> **Last Updated**: 2026-03-23
> **Branch**: `develPython`

TizenClaw Python port provides **13 native CLI tool suites** (standalone executables shared with the C++ version), **17 embedded tool MD schemas**, and a `ToolIndexer` system for LLM tool discovery. All tools are discovered at daemon startup by scanning `.tool.md`, `.skill.md`, and `.mcp.json` files under `/opt/usr/share/tizenclaw/tools/`.

> **Tool Discovery**: The `ToolIndexer` class parses YAML frontmatter from Markdown schema files using regex, extracting `name` and `description` fields. Each tool gets a catch-all `arguments` parameter for flexible LLM invocation.

> CLI tool suites use `ctypes` FFI to call Tizen C-API directly. Async skills (⚡) use the **tizen-core** event loop for callback-based APIs.

---

## Tool Architecture (Python Port)

```
AgentCore
    │
    ▼
ToolIndexer                          ToolDispatcher
(scans filesystem for schemas)       (routes tool calls)
    │                                     │
    ├── tools/cli/*/*.tool.md        ┌────┤
    ├── tools/embedded/*.md          │    │
    └── *.mcp.json                   │    │
                                     ▼    ▼
                            ContainerEngine
                            (abstract UDS IPC)
                                     │
                                     ▼
                            Tool Executor
                            (asyncio subprocess)
                                     │
                                     ▼
                            CLI binary / Python script
                            (Tizen C-API via ctypes)
```

### ToolIndexer (`tool_indexer.py`)

| Feature | Details |
|---------|---------|
| **Base directory** | `/opt/usr/share/tizenclaw/tools/` |
| **Scan pattern** | `os.walk()` for `*.tool.md`, `*.skill.md`, `*.mcp.json` |
| **YAML parser** | Simplified regex (`^---\n(.*?)\n---`) + line-by-line `key: value` |
| **Schema output** | `{name, description, parameters}` with catch-all `arguments` string |
| **Indexing** | Writes `tools/tools.md` and `tools/skills/index.md` at load time |

### ToolDispatcher (`tool_dispatcher.py`)

| Tool Type | Execution Path |
|-----------|---------------|
| `cli` | `ContainerEngine.execute_cli_tool(name, args)` |
| `skill` | `ContainerEngine.execute_skill(path, args)` |
| `mcp` | `ContainerEngine.execute_mcp_tool(name, args)` |

---

## Native CLI Tool Suites (13 directories)

These are standalone executable tools shared between C++ and Python daemon versions. They are located in `tools/cli/` and interface with Tizen C-APIs via `ctypes` FFI.

### App Management

| Skill | Parameters | C-API | Description |
|-------|-----------|-------|-------------|
| `list_apps` | — | `app_manager` | List all installed applications |
| `send_app_control` | `app_id`, `operation`, `uri`, `mime`, `extra_data` | `app_control` | Launch app via explicit app_id or implicit intent |
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
| `get_metadata` | `file_path` | `metadata-extractor` | Extract media file metadata |
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

## Embedded Tool Schemas (17 files)

Located in `tools/embedded/`, these Markdown files define tool schemas that the `ToolIndexer` loads for LLM discovery. In the Python port, these are read-only schema definitions — the actual execution logic is handled by `ToolDispatcher`.

| Tool | File | Category |
|------|------|----------|
| `execute_code` | `execute_code.md` | Code Execution |
| `create_task` | `create_task.md` | Task Scheduler |
| `list_tasks` | `list_tasks.md` | Task Scheduler |
| `cancel_task` | `cancel_task.md` | Task Scheduler |
| `create_session` | `create_session.md` | Multi-Agent |
| `ingest_document` | `ingest_document.md` | RAG |
| `search_knowledge` | `search_knowledge.md` | RAG |
| `create_workflow` | `create_workflow.md` | Workflow Engine |
| `list_workflows` | `list_workflows.md` | Workflow Engine |
| `run_workflow` | `run_workflow.md` | Workflow Engine |
| `delete_workflow` | `delete_workflow.md` | Workflow Engine |
| `create_pipeline` | `create_pipeline.md` | Pipeline Engine |
| `list_pipelines` | `list_pipelines.md` | Pipeline Engine |
| `run_pipeline` | `run_pipeline.md` | Pipeline Engine |
| `delete_pipeline` | `delete_pipeline.md` | Pipeline Engine |
| `run_supervisor` | `run_supervisor.md` | Multi-Agent |
| `generate_web_app` | `generate_web_app.md` | Web App |

---

## Tool Dispatch Architecture (Python)

Tool execution uses a modular `ToolDispatcher` class for routing:

- **Dict Lookup**: `Dict[str, Dict]` for O(1) registered tool access
- **Type Routing**: `cli` → ContainerEngine CLI, `skill` → ContainerEngine Skill, `mcp` → ContainerEngine MCP
- **Error Handling**: Unknown tools return descriptive error messages
- **Argument Serialization**: Dict arguments auto-serialized to JSON strings

### Execution Flow

```
LLM Response
    │
    ▼
AgentCore.process_prompt()
    │
    ├── tool_call.name = "get_device_info"
    │   tool_call.arguments = {"arguments": ""}
    │
    ▼
ToolDispatcher.execute_tool("get_device_info", args)
    │
    ├── ToolIndexer.get_tool_metadata("get_device_info")
    │   → {type: "cli", path: "...", ...}
    │
    ▼
ContainerEngine.execute_cli_tool("get_device_info", "", timeout=30)
    │
    ▼
Tool Executor (UDS IPC) → asyncio subprocess → CLI binary
    │
    ▼
{status: "success", stdout: "...", stderr: "...", exit_code: 0}
```

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

---

## Comparison: C++ vs Python Tool System

| Feature | C++ (main/devel) | Python (develPython) |
|---------|:---:|:---:|
| **ToolIndexer** | C++ YAML parser | Python regex YAML parser |
| **ToolDispatcher** | `std::unordered_map` + `std::shared_mutex` | `Dict` + `asyncio.Lock` |
| **CapabilityRegistry** | Full FunctionContract system | Not ported |
| **CLI execution** | `popen()` via C++ | `asyncio.create_subprocess_exec` |
| **Container runtime** | crun 1.26 (OCI) | `unshare` fallback |
| **Plugin discovery** | pkgmgrinfo (RPK/TPK) | Not ported |
| **Skill hot-reload** | inotify watcher | Not ported |
| **`.tool.md` format** | Same | Same |
