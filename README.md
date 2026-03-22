<p align="center">
  <img src="docs/img/tizenclaw.jpg" alt="TizenClaw Logo" width="280">
</p>

<h1 align="center">TizenClaw</h1>

<p align="center">
  <strong>AI-Powered Agent Daemon for Tizen OS — Python Port</strong><br>
  Control your Tizen device through natural language — powered by multi-provider LLMs,<br>
  containerized skill execution, and a web-based admin dashboard.
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-Apache_2.0-blue.svg" alt="License"></a>
  <img src="https://img.shields.io/badge/Python_3.x-Native-blue.svg" alt="Language">
  <img src="https://img.shields.io/badge/Tizen_10.0%2B-Supported-brightgreen.svg" alt="Platform">
  <img src="https://img.shields.io/badge/LLM_Backends-OpenAI_Compatible-purple.svg" alt="LLM Backends">
  <img src="https://img.shields.io/badge/Branch-develPython-orange.svg" alt="Branch">
</p>

<p align="center">
  <a href="#-key-features">Features</a> •
  <a href="#-quick-start">Quick Start</a> •
  <a href="#-architecture">Architecture</a> •
  <a href="#-skills--tools">Skills</a> •
  <a href="#-documentation">Documentation</a> •
  <a href="#-related-projects">Related Projects</a>
</p>

---

## 🔍 Overview

**TizenClaw (Python Port)** is the `develPython` branch of TizenClaw, which **ports the entire C++ daemon to pure Python 3** to evaluate memory, speed, and storage footprints on Tizen embedded devices. It runs as a **systemd service** in the background, receiving user prompts through IPC (JSON-RPC 2.0 over Unix Domain Sockets), interpreting them via an OpenAI-compatible LLM backend, and executing device-level actions using native CLI tool suites and containerized skill execution.

> **Branch Note**: The `main` and `devel` branches contain the original native C++20 implementation. This `develPython` branch is a **full Python rewrite** for comparison evaluation.

| | **C++ (main/devel)** | **Python (develPython)** |
|---|:---:|:---:|
| **Language** | C++20 | Python 3.x |
| **Dependencies** | libcurl, libsoup, nlohmann/json, etc. | Zero external dependencies (stdlib only) |
| **HTTP Client** | libcurl | `urllib.request` (asyncio offload) |
| **IPC** | C++ threads + UDS | `asyncio` Unix sockets |
| **LLM Backend** | 5 backends (Gemini, OpenAI, Anthropic, xAI, Ollama) | OpenAI-compatible backend |
| **Container** | crun OCI exec | `unshare` fallback-capable |

---

## ✨ Key Features

<table>
<tr>
<td width="50%">

### 🐍 Pure Python Implementation
- Zero external pip dependencies — uses only Python stdlib
- `asyncio`-based daemon with cooperative concurrency
- `ctypes` FFI for Tizen native C-API integration (dlog, vconf, app_event)
- `urllib.request` for HTTP with `asyncio.to_thread` offloading

### 🤖 LLM Support
- **OpenAI-compatible backend** with tool calling (function calling)
- Agentic Loop with iterative tool execution (max 10 iterations)
- Auto-skill intercept for direct device queries without LLM overhead
- Configurable via environment variables (`OPENAI_API_KEY`)

### 📱 Tizen C-API Access
- 13 native CLI tool suites with ctypes FFI wrappers
- Battery, Wi-Fi, Bluetooth, Display, Sensors, Notifications, etc.
- 17 embedded tool MD schemas for LLM discovery

</td>
<td width="50%">

### 🔧 Modular Architecture
- `AgentCore` — Central orchestration with agentic loop
- `ToolIndexer` — Scans `.tool.md` / `.skill.md` / `.mcp.json` schemas
- `ToolDispatcher` — Routes tool calls to container engine
- `ContainerEngine` — IPC with secure tool executor
- `WorkflowEngine` — Markdown-based deterministic pipelines

### 📡 IPC & Communication
- JSON-RPC 2.0 over abstract Unix Domain Sockets
- MCP server mode (`--mcp-stdio`) for Claude Desktop integration
- `tizenclaw-cli` Python client for interactive/single-shot usage
- Socket-activated tool executor and code sandbox services

### 🧠 Intelligence & Storage
- SQLite-based RAG store (FTS5 + vector cosine similarity)
- On-device ONNX embedding (all-MiniLM-L6-v2, lazy-loaded)
- Persistent memory (long-term / episodic / short-term)
- Markdown-based session persistence with YAML frontmatter

</td>
</tr>
</table>

---

## 🚀 Quick Start

### Prerequisites

- **Tizen 10.0** or later target device/emulator
- **GBS** (Git Build System) — [Tizen build tools](https://docs.tizen.org/platform/developing/installing/)
- **sdb** (Smart Development Bridge) for device deployment
- **Python 3.x** available on the target device

### 1. Install Build Tools

```bash
# Add Tizen tools repository (Ubuntu)
echo "deb [trusted=yes] http://download.tizen.org/tools/latest-release/Ubuntu_$(lsb_release -rs)/ /" | \
sudo tee /etc/apt/sources.list.d/tizen.list > /dev/null

sudo apt update && sudo apt install gbs mic
```

### 2. Build & Deploy (Recommended)

```bash
# One-command build + deploy to connected device
./deploy.sh

# With secure tunnel (ngrok) for remote dashboard access
./deploy.sh --with-ngrok
```

The `deploy.sh` script handles building the RPM via GBS, installing it on the device, and restarting the daemon automatically.

### 3. Verify Installation

```bash
# Check daemon status
sdb shell systemctl status tizenclaw -l

# Send a natural language command
sdb shell tizenclaw-cli "What is the battery level?"

# Interactive mode
sdb shell tizenclaw-cli

# Streaming mode
sdb shell tizenclaw-cli --stream "List all installed apps"

# Access the Web Dashboard
sdb forward tcp:9090 tcp:9090
# Open http://localhost:9090 in your browser
```

### 4. Configure LLM Backend

Set the OpenAI API key on the device:

```bash
# Set API key as environment variable (in systemd service or shell)
export OPENAI_API_KEY="YOUR_API_KEY"
```

The Python port currently uses the OpenAI-compatible backend (`gpt-4o` by default). The model can be configured by modifying `OpenAiBackend` initialization parameters.

> 💡 **Tip**: Configuration files remain editable through the [Web Dashboard](docs/DESIGN.md#web-dashboard) at port 9090.

---

## 🏗 Architecture

TizenClaw Python port uses an **asyncio-based daemon architecture** with systemd socket activation for companion services:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Communication Channels                              │
│        MCP (stdio) · Web Dashboard (port 9090) · tizenclaw-cli             │
└──────┬──────────────────────────────────────────────────────────┬───────────┘
       │                                                          │
       ▼                                                          ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│  TizenClaw Daemon (Python / systemd)                                        │
│                                                                              │
│  ┌────────────────┐   ┌───────────────┐   ┌──────────────────────────────┐  │
│  │  IPC Server    │──▶│   AgentCore   │──▶│    OpenAI-compatible LLM     │  │
│  │ (asyncio UDS)  │   │ (Agentic Loop)│   │   Backend (urllib.request)   │  │
│  │ JSON-RPC 2.0   │   │ max 10 iters  │   └──────────────────────────────┘  │
│  └────────────────┘   └──┬───┬───┬────┘                                     │
│                  ┌───────┘   │   └────────┐                                  │
│                  ▼           ▼            ▼                                   │
│  ┌──────────────────┐ ┌──────────┐ ┌────────────────┐  ┌────────────────┐   │
│  │  ToolDispatcher  │ │ Session  │ │ WorkflowEngine │  │ EmbeddingStore │   │
│  │  (type routing)  │ │ Store    │ │ (Markdown-based│  │ (SQLite + FTS5)│   │
│  └────┬─────────────┘ └──────────┘ │  pipelines)    │  └────────────────┘   │
│       │                            └────────────────┘                        │
│       ▼                                                                      │
│  ┌──────────────────┐   ┌─────────────────┐   ┌────────────────────────┐    │
│  │  ToolIndexer     │   │  TaskScheduler  │   │ MemoryStore            │    │
│  │ (.tool.md scan)  │   │ (asyncio tasks) │   │ (Markdown + YAML)      │    │
│  └──────────────────┘   └─────────────────┘   └────────────────────────┘    │
└───────┬──────────────────────────────────────────────────────────────────────┘
        │
        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│  Container Engine (abstract UDS IPC)                                         │
│                                                                              │
│  ┌──────────────────────┐   ┌──────────────────────┐                        │
│  │ Tool Executor        │   │ Code Sandbox          │                        │
│  │ (socket-activated)   │   │ (socket-activated)    │                        │
│  │ asyncio subprocess   │   │ asyncio stub          │                        │
│  └──────────────────────┘   └──────────────────────┘                        │
│                                                                              │
│  13 CLI Tool Suites (ctypes FFI → Tizen C-API)                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

> 📖 For detailed architecture documentation, see [System Design](docs/DESIGN.md).

---

## 🧰 Skills & Tools

TizenClaw Python port ships with **13 native CLI tool suites** and **17 embedded tool MD schemas**. All tools are discovered by `ToolIndexer` at startup.

### Native CLI Tool Suites

| Category | Tools | Key Examples |
|----------|:-----:|---------| 
| **App Management** | 4 | `list_apps`, `send_app_control`, `terminate_app`, `get_package_info` |
| **Device Info & Sensors** | 7 | `get_device_info`, `get_sensor_data`, `get_thermal_info`, `get_runtime_info` |
| **Network & Connectivity** | 6 | `get_wifi_info`, `scan_wifi_networks` ⚡, `scan_bluetooth_devices` ⚡ |
| **Display & Hardware** | 6 | `control_display`, `control_volume`, `control_haptic`, `control_led` |
| **Media & Content** | 5 | `get_metadata`, `get_media_content`, `get_sound_devices`, `get_mime_type` |
| **System Actions** | 6 | `download_file` ⚡, `send_notification`, `schedule_alarm`, `web_search` |

> ⚡ Async skill using tizen-core event loop

### Embedded Tool Schemas (17 built-in)

| Category | Tools |
|----------|-------|
| **Task Management** | `create_task`, `list_tasks`, `cancel_task` |
| **Knowledge (RAG)** | `ingest_document`, `search_knowledge` |
| **Session Management** | `create_session` |
| **Workflow & Pipeline** | `create_workflow`, `list_workflows`, `run_workflow`, `delete_workflow`, `create_pipeline`, `list_pipelines`, `run_pipeline`, `delete_pipeline` |
| **Multi-Agent** | `run_supervisor` |
| **Code Execution** | `execute_code` |
| **Web App** | `generate_web_app` |

📖 **Full reference**: [Tools Reference](docs/TOOLS.md)

---

## ⚙️ Configuration

TizenClaw reads configuration from `/opt/usr/share/tizenclaw/` on the device. Configuration files are the same as the C++ version and remain editable via the **Web Dashboard** (port 9090).

| Config File | Purpose |
|---|---|
| `llm_config.json` | LLM backend selection, API keys, model settings |
| `channels.json` | Channel activation and plugin paths |
| `tool_policy.json` | Tool execution policy (max iterations, blocked skills) |
| `agent_roles.json` | Agent roles and specialized system prompts |
| `memory_config.json` | Memory retention periods, size limits |

> Sample configurations are included in `data/sample/`.

---

## 🏗 Build

### Automated Build & Deploy (Recommended)

```bash
./deploy.sh                    # Build + deploy to connected device
./deploy.sh --with-ngrok       # With secure tunnel
./deploy.sh --debug            # Debug mode (no container)
```

### Manual Build

```bash
# x86_64 (emulator, default)
gbs build -A x86_64 --include-all

# armv7l (32-bit ARM devices)
gbs build -A armv7l --include-all

# aarch64 (64-bit ARM devices)
gbs build -A aarch64 --include-all
```

**RPM output**: `~/GBS-ROOT/local/repos/tizen/<arch>/RPMS/tizenclaw-1.0.0-1.<arch>.rpm`

### What Gets Installed

| Path | Description |
|------|-------------|
| `/usr/bin/tizenclaw` | Daemon entry point (Python script) |
| `/usr/bin/tizenclaw-daemon` | Daemon alias |
| `/usr/bin/tizenclaw-cli` | CLI client (Python script) |
| `/usr/bin/tizenclaw-tool-executor` | Socket-activated tool executor |
| `/usr/bin/tizenclaw-code-sandbox` | Socket-activated code sandbox |
| `/opt/usr/share/tizenclaw-python/` | Python package tree (`tizenclaw/` module) |
| `/usr/lib/systemd/system/` | systemd service and socket units |

---

## 📋 Project Structure

```
tizenclaw/  (develPython branch)
├── src_py/                           # Python daemon source
│   ├── tizenclaw_daemon.py           # Main daemon (IPC Server + MCP stdio)
│   ├── tizenclaw_cli.py              # CLI client tool
│   ├── tizenclaw_tool_executor.py    # Socket-activated tool executor
│   ├── tizenclaw_code_sandbox.py     # Socket-activated code sandbox
│   └── tizenclaw/                    # Core Python package
│       ├── core/                     # Agent core, tool indexer, dispatcher, workflow
│       │   ├── agent_core.py         # Agentic loop + LLM orchestration
│       │   ├── tool_indexer.py       # .tool.md / .skill.md / .mcp.json scanner
│       │   ├── tool_dispatcher.py    # Tool dispatch routing
│       │   └── workflow_engine.py    # Markdown-based workflow pipelines
│       ├── llm/                      # LLM backend providers
│       │   ├── llm_backend.py        # Abstract base class + data types
│       │   └── openai_backend.py     # OpenAI-compatible REST backend
│       ├── infra/                    # Infrastructure
│       │   ├── container_engine.py   # Tool executor IPC (UDS)
│       │   └── tizen_system_event_adapter.py  # ctypes app_event adapter
│       ├── storage/                  # Data persistence
│       │   ├── session_store.py      # Markdown session serialization
│       │   ├── memory_store.py       # Long/episodic/short-term memory
│       │   └── embedding_store.py    # SQLite RAG + FTS5 + cosine similarity
│       ├── embedding/                # On-device ML embedding
│       │   └── on_device_embedding.py  # ONNX Runtime inference (lazy-loaded)
│       ├── scheduler/                # Task automation
│       │   └── task_scheduler.py     # asyncio-based cron/interval scheduler
│       └── utils/                    # Utilities
│           ├── tizen_dlog.py         # Python logging → Tizen dlog handler
│           └── native_wrapper.py     # ctypes bindings for Tizen native APIs
├── tools/cli/                        # 13 Native CLI tool suites
├── tools/embedded/                   # 17 Embedded tool MD schemas
├── scripts/                          # Container setup, CI, hooks
├── tests/
│   ├── unit/                         # Legacy C++ test files (from main branch)
│   ├── e2e/                          # E2E smoke tests
│   └── verification/                 # Shell-based verification suites (28 tests)
├── data/                             # Config, web dashboard, rootfs images
├── packaging/                        # RPM spec, systemd services & sockets
├── docs/                             # Design, analysis, roadmap
├── deploy.sh                         # Automated build & deploy script
├── CMakeLists.txt                    # Install-only CMake (Python, no compilation)
└── LICENSE                           # Apache License 2.0
```

---

## 📚 Documentation

| Document | Description |
|----------|-------------|
| **[System Design](docs/DESIGN.md)** | Architecture, module design, data flow |
| **[Tools Reference](docs/TOOLS.md)** | Complete skill/tool catalog with parameters and C-API mapping |
| **[Project Analysis](docs/ANALYSIS.md)** | Code statistics, module inventory |
| **[Development Roadmap](docs/ROADMAP.md)** | Feature roadmap, completed phases |
| **[Supported Features](docs/FEATURES.md)** | Detailed supported/unsupported feature matrix |

---

## 🔗 Related Projects

| Project | Description |
|---------|-------------|
| **[tizenclaw-assets](https://github.com/hjhun/tizenclaw-assets)** | Consolidated ML/AI asset package — ONNX Runtime, RAG databases, embedding model, PaddleOCR engine |
| **[tizenclaw-webview](https://github.com/hjhun/tizenclaw-webview)** | Companion Tizen web app for on-device dashboard access |
| **[tizenclaw-skill-plugin-sample](https://github.com/hjhun/tizenclaw-skill-plugin-sample)** | Sample RPK plugin for Python skill injection |

---

## 📊 Project Statistics

| Category | Count |
|----------|------:|
| Python Source (src_py) | 20 files (~1,800 LOC) |
| CLI Tools (Python/C) | 38 files |
| Verification Tests (Shell) | 28 files (~3,400 LOC) |
| Web Frontend | 5 files (~3,900 LOC) |
| Embedded Tool Schemas | 17 files |
| **Total Python** | **~7,600 LOC** |

---

## 📄 License

This project is licensed under the [Apache License 2.0](LICENSE).

Copyright 2024-2026 Samsung Electronics Co., Ltd.
