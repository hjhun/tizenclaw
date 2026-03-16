# TizenClaw Tool Selection & Routing Guide

You must follow this guide strictly when selecting tools to fulfill user requests. Tools are categorized by implementation type and priority.

## 1. Tool Categories

### A. Tizen Actions (`action_*`) - Highest Priority
Native Tizen Platform features. These are the fastest and most reliable for core device control.
- **Usage**: Use for display brightness, volume, flashlight, notifications, and core system settings.
- **Priority**: Always check if an `action_` tool exists for a task before using a Python skill.

### B. Embedded Tools (`embedded`) - High Priority
C++ built-in tools for system management and agent coordination.
- **Core Operations**: `file_manager` (file I/O), `task_scheduler` (automation).
- **Agent Coordination**: `supervisor_engine` (multi-agent delegation), `session_manager` (context handling).

### C. Standard Skills (`skills/`) - Medium Priority
Pre-defined Python scripts for specific functionalities (e.g., `web_search`, `get_battery_info`).
- **Usage**: Use when a native Tizen Action is not available or for specialized logic like web scraping or data parsing.

### D. Custom Skills (`custom_skills/`) - Dynamic Priority
User-defined or AI-generated scripts added at runtime.
- **Usage**: Use when standard tools/skills are insufficient for a specific, newly defined requirement.

### E. System CLI Tools (`system_cli`) - Medium Priority
System-level CLI tools from `/usr/bin` whitelisted via `system_cli_config.json`.
- **Available tools**: `aul_test` (app lifecycle management), `winfo` (window compositor info/control).
- **Usage**: Use for app listing/launching/termination (`aul_test`) and window/display diagnostics (`winfo`).
- **Priority**: Use after Tizen Actions but before Python skills for system-level queries.

## 2. Selection Strategy & Logic

1. **Prefer Native**: If `action_brightness` and `control_display` are both available, you MUST use `action_brightness`.
2. **Confirm State First**: Before changing a system state, use a `get_` skill (e.g., `get_display_info`, `get_battery_info`) to verify current values unless the user is explicit.
3. **Handle Failure Gracefully**:
   - If an `action_` tool fails, try the corresponding Python `skill` if it exists.
   - If a Python skill fails, explain the error and suggest an alternative if possible.
4. **App Interaction**:
   - Never guess an `app_id`. Use `list_apps` to find the correct identifier before calling `send_app_control` or `terminate_app`.
5. **Security & Safety**:
   - For irreversible operations (e.g., `delete_file`, `terminate_app`), always ask for confirmation unless the user's intent is absolutely clear and specific.
   - Paths for `file_manager` MUST start with `/tools/skills/` (for code) or `/data/` (for data).

## 3. Decision Tree Examples

- **"Make the screen brighter"**
  -> `get_display_info` (check current level) -> `action_brightness` (set new level).
- **"Search for the weather in Seoul"**
  -> `web_search(query="weather in Seoul")`.
- **"Kill the music player"**
  -> `list_apps(filter="music")` -> `terminate_app(app_id="...")`.
- **"Remind me to take medicine in 2 hours"**
  -> `create_task(command="send_notification(...)", trigger_type="interval", interval_seconds=7200)`.

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
1. **Tizen Actions** (`action_*`) — Native platform features
2. **Embedded Tools** — C++ built-in tools
3. **System CLI Tools** — Host-level CLI tools
4. **Standard Skills** — Python container skills
5. **CLI Tool Plugins** — TPK-based CLI tools
6. **RPK Plugins** — Resource package tools

### Routing Mechanisms
- **Manual Aliases**: Configured in `tool_policy.json` (e.g., `control_display` → `action_brightness`)
- **Auto-Detection**: If two tools share the same category but different source priorities, the lower-priority tool is automatically redirected

### Behavior
- When routing occurs, the output includes a `[Routed: original → target]` hint
- You do **NOT** need to call the redirected tool again — it was already executed with the correct target
- If the higher-priority tool fails, the error is returned as-is (no automatic fallback to the original)

