# TizenClaw Tool Selection & Routing Guide

You must follow this guide strictly when selecting tools to fulfill user requests. Tools are categorized by implementation type and priority.

## 1. Tool Categories

### A. Standard Skills & Custom Skills (`skills/` + `custom_skills/`) — Priority 1 & 2 (Highest)
- **Standard Skills** (`skills/`) [Priority 1]: Pre-defined Python scripts for specific functionalities (e.g., `web_search`, `get_battery_info`).
- **Custom Skills** (`custom_skills/`) [Priority 2]: User-defined or AI-generated scripts added at runtime via `manage_custom_skill`.
- **Usage**: Use when complex logic, web scraping, data parsing, or specialized workflows are needed.

### B. Tizen Actions (`action_*`) — Priority 3
Native Tizen Platform features. These are fast and reliable for core device control.
- **Usage**: Use for display brightness, volume, flashlight, notifications, and core system settings.

### C. Embedded Tools (`embedded`) — Priority 4
C++ built-in tools for system management and agent coordination.
- **Core Operations**: `task_scheduler` (automation), `execute_code`.
- **Agent Coordination**: `supervisor_engine`.

### D. System CLI (`system_cli/`) — Priority 5
System-level tools registered via `tizenclaw-cli --register-tool <path>`.
- **Usage**: General system commands, host-level operations.

### E. CLI Tools (`cli/`) & TPK Plugins — Priority 6 & 7
Native C++ CLI tools and TPK-based resource plugins.
- **Usage**: Pre-built native CLI tools (e.g., `tizen-file-manager-cli`).



## 2. Selection Strategy & Logic

1. **Prefer Native**: If `action_brightness` and `control_display` are both available, you MUST use `action_brightness`.
2. **Prefer CLI over Skills**: If a CLI tool (e.g., `tizen-file-manager-cli list`) can achieve the same result as a Python skill, prefer the CLI tool for better performance and reliability.
3. **Confirm State First**: Before changing a system state, use a `get_` skill or CLI query (e.g., `tizen-device-info-cli`, `get_battery_info`) to verify current values unless the user is explicit.
4. **Handle Failure Gracefully**:
   - If an `action_` tool fails, try the corresponding CLI tool or Python `skill` if it exists.
   - If a CLI tool fails, try the Python skill fallback.
   - If a Python skill fails, explain the error and suggest an alternative if possible.
5. **App Interaction**:
   - Never guess an `app_id`. Use `tizen-app-manager-cli list` or `list_apps` to find the correct identifier before calling `send_app_control` or `terminate_app`.
6. **Security & Safety**:
   - For irreversible operations (e.g., `delete_file`, `terminate_app`), always ask for confirmation unless the user's intent is absolutely clear and specific.
   - Paths for `file_manager` MUST start with `/tools/skills/` (for code) or `/data/` (for data).

## 3. Decision Tree Examples

- **"Make the screen brighter"**
  -> `action_brightness` (native Tizen action, highest priority).
- **"List files in /tmp"**
  -> `execute_cli(tool_name="tizen-file-manager-cli", arguments="list --path /tmp")` (CLI tool).
- **"Search for the weather in Seoul"**
  -> `web_search(query="weather in Seoul")` (standard skill).
- **"Kill the music player"**
  -> `execute_cli(tool_name="tizen-app-manager-cli", arguments="running")` -> `terminate_app(app_id="...")`.
- **"Remind me to take medicine in 2 hours"**
  -> `create_task(command="send_notification(...)", trigger_type="interval", interval_seconds=7200)`.
- **"What's the MIME type of this file?"**
  -> `execute_cli(tool_name="tizen-media-cli", arguments="mime --path /path/to/file")` (CLI tool).
- **"Check Wi-Fi networks"**
  -> `execute_cli(tool_name="tizen-network-info-cli", arguments="wifi-scan")` (CLI tool).
- **"Download a file from URL"**
  -> `execute_cli(tool_name="tizen-file-manager-cli", arguments="download --url https://... --dest /tmp/file")`.

## 4. Agent Routing Strategy

For complex or domain-specific requests, use `run_supervisor` to delegate to specialist agents.
Each agent has its own system prompt and tool restrictions — it will produce higher quality results
than handling everything in the main session.

### Available Specialist Agents

| Agent | Domain | Delegate When... |
|-------|--------|-----------------|
| `device_monitor` | Device Health | Battery, temperature, memory, storage, network status queries |
| `knowledge_retriever` | Knowledge Search | Document search, knowledge lookup, semantic queries, Tizen API docs |
| `task_planner` | Automation | Scheduling tasks, creating pipelines, managing workflows |
| `skill_manager` | Skill Development | Creating new Python skills, Tizen C-API integration |
| `security_auditor` | Security | Security analysis, audit review, risk assessment |
| `recovery_agent` | Error Recovery | Failure diagnosis, fallback strategies, error correction |
| `file_operator` | File & Code | File read/write, code execution, data processing |

### When to Delegate vs Handle Directly
1. **Direct handling**: Simple tool calls (brightness, volume, notifications)
2. **Delegate to single agent**: Domain-specific queries (device status → `device_monitor`)
3. **Multi-agent delegation**: Complex multi-domain tasks → `run_supervisor` with appropriate strategy

### Agent Delegation Decision Tree
- **"Check device health"** → `run_supervisor(goal="...", strategy="sequential")` with `device_monitor`
- **"Find documentation about Tizen WiFi API"** → `run_supervisor` → `knowledge_retriever`
- **"Create a daily battery check automation"** → `run_supervisor` → `task_planner` + `device_monitor`
- **"Analyze security of recent operations"** → `run_supervisor` → `security_auditor`

## 5. Automatic Tool Routing

TizenClaw includes a **ToolRouter** that automatically redirects tool calls to higher-priority alternatives at runtime. If a duplicate tool redirection occurs, the system has already chosen the best tool — you do not need to retry.

### Priority Order (highest to lowest)
1. **Standard Skills** (`skill`) — Pre-defined standard python skills
2. **Custom Skills** (`custom_skill`) — Runtime-generated custom skills
3. **Tizen Actions** (`action_`) — Native platform features
4. **Embedded Tools** (`embedded`) — C++ built-in tools
5. **System CLI** (`system_cli`) — System-level host tools
6. **CLI Tools** (`cli`) — Native CLI tools plugins
7. **RPK Plugins** (`rpk`) — Resource package tools

### Routing Mechanisms
- **Manual Aliases**: Configured in `tool_policy.json` (e.g., `control_display` → `action_brightness`)
- **Auto-Detection**: If two tools share the same category but different source priorities, the lower-priority tool is automatically redirected

### Behavior
- When routing occurs, the output includes a `[Routed: original → target]` hint
- You do **NOT** need to call the redirected tool again — it was already executed with the correct target
- If the higher-priority tool fails, the error is returned as-is (no automatic fallback to the original)
