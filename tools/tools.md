# TizenClaw Tool Catalog

This document provides a consolidated index of all available tools provided by the TizenClaw system.
Tools are categorized into **Native CLI Tools** and **Embedded (Built-in) Tools**. 

> **Note:** The previously used `skills`, `system_cli`, and `actions` frameworks are obsolete (e.g., the code sandbox was removed) and are no longer part of the primary tool infrastructure. The agent exclusively relies on native Tizen CLI tools and internal embedded tool capabilities.

---

## 1. Native CLI Tools

**Index:** [`cli/index.md`](cli/index.md)

CLI tools are pre-built, Tizen-native executables installed under `/opt/usr/share/tizen-tools/cli/<tool-name>/`.
They directly interface with the Tizen OS C-API and return structured JSON responses.

Available CLI tools:
- **`tizen-app-manager-cli`**: App Management (list, launch, terminate apps, get package info, query recent apps)
- **`tizen-control-display-cli`**: Display (get/set display brightness)
- **`tizen-device-info-cli`**: Device Info (battery, CPU, memory, storage, thermal, display, settings)
- **`tizen-file-manager-cli`**: File System (read, write, append, copy, move, remove, list, mkdir, stat, download)
- **`tizen-hardware-control-cli`**: Hardware (haptic vibration, camera flash LED, power lock, feedback)

- **`tizen-network-info-cli`**: Network (Wi-Fi, Bluetooth, network status, scan, data usage)
- **`tizen-notification-cli`**: Notification (send notifications, schedule alarms)
- **`tizen-sensor-cli`**: Sensor (accelerometer, gyroscope, light, proximity, pressure, magnetic, orientation)
- **`tizen-sound-cli`**: Sound (get/set volume, list devices, play tones)
- **`tizen-vconf-cli`**: Configuration (read, write, or watch vconf system settings)
- **`tizen-web-search-cli`**: Web Search (multi-engine web search including Naver, Google, Brave, Gemini, etc.)

---

## 2. Embedded Tools

Embedded tools are internally implemented functionalities within the TizenClaw AI agent. They handle agent cognition, workflow orchestrations, vector database searches, and arbitrary execution. Detailed tool schemas are provided in the individual markdown files in the `embedded/` directory.

### Agent & Workflow Management
- **[`run_supervisor`](embedded/run_supervisor.md)**: Run an autonomous supervisor to orchestrate tasks.
- **[`create_workflow`](embedded/create_workflow.md)**: Create a new execution workflow.
- **[`run_workflow`](embedded/run_workflow.md)**: Execute an existing workflow.
- **[`list_workflows`](embedded/list_workflows.md)**: Retrieve a list of all workflows.
- **[`delete_workflow`](embedded/delete_workflow.md)**: Delete an existing workflow.
- **[`create_pipeline`](embedded/create_pipeline.md)**: Set up an data/execution pipeline.
- **[`run_pipeline`](embedded/run_pipeline.md)**: Execute a configured pipeline.
- **[`list_pipelines`](embedded/list_pipelines.md)**: List all pipelines.
- **[`delete_pipeline`](embedded/delete_pipeline.md)**: Remove a pipeline.

### Task & Session Execution
- **[`create_session`](embedded/create_session.md)**: Establish a context session.
- **[`create_task`](embedded/create_task.md)**: Add a new task to the queue or current execution context.
- **[`list_tasks`](embedded/list_tasks.md)**: Show active or persistent tasks.
- **[`cancel_task`](embedded/cancel_task.md)**: Abort a running task.

### Knowledge & RAG
- **[`ingest_document`](embedded/ingest_document.md)**: Ingest a document/text into the vector database.
- **[`search_knowledge`](embedded/search_knowledge.md)**: Semantic semantic querying against the knowledge base.

### Generative Execution
- **[`generate_web_app`](embedded/generate_web_app.md)**: Generate a structured web application (HTML/CSS/JS).
