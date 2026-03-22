<p align="center">
  <img src="docs/img/tizenclaw.jpg" alt="TizenClaw Logo" width="280">
</p>

<h1 align="center">TizenClaw</h1>

<p align="center">
  <strong>AI-Powered Agent Daemon for Tizen OS</strong><br>
  Control your Tizen device through natural language — powered by multi-provider LLMs,<br>
  containerized skill execution, and a web-based admin dashboard.
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-Apache_2.0-blue.svg" alt="License"></a>
  <img src="https://img.shields.io/badge/C%2B%2B20-Native-orange.svg" alt="Language">
  <img src="https://img.shields.io/badge/Tizen_10.0%2B-Supported-brightgreen.svg" alt="Platform">
  <img src="https://img.shields.io/badge/LLM_Backends-5%2B_Extensible-purple.svg" alt="LLM Backends">
  <img src="https://img.shields.io/badge/Channels-7%2B_Extensible-blue.svg" alt="Channels">
  <img src="https://img.shields.io/badge/Binary-~812KB-red.svg" alt="Binary Size">
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

**TizenClaw** is a native C++ system daemon that brings LLM-based AI agent capabilities to [Tizen](https://www.tizen.org/) devices. It receives natural language commands via multiple communication channels, interprets them through configurable LLM backends, and executes device-level actions using sandboxed Python skills inside OCI containers and the **Tizen Action Framework**.

> **Part of the Claw Family** — TizenClaw is the embedded-optimized member of the Claw AI agent runtime family.

| | **TizenClaw** | **OpenClaw** | **NanoClaw** | **ZeroClaw** |
|---|:---:|:---:|:---:|:---:|
| **Language** | C++20 | TypeScript | TypeScript | Rust |
| **Target** | Tizen embedded | Cloud / Desktop | Container hosts | Edge hardware |
| **Binary Size** | ~812KB | Node.js runtime | Node.js runtime | ~8.8MB |
| **Channels** | 7+ (extensible) | 22+ | 5 | 17 |
| **LLM Backends** | 5+ (extensible) | 4+ | 1 (Claude) | 5+ |
| **Sandboxing** | OCI (crun) | Docker | Docker | Docker |

---

## ✨ Key Features

<table>
<tr>
<td width="50%">

### 🚀 Native Performance
- ~812KB stripped binary (armv7l)
- ~8.5MB idle PSS memory footprint
- Aggressive idle memory reclamation via `malloc_trim` and SQLite cache flushing
- No Node.js/Docker runtime dependency

### 🤖 Multi-LLM Support
- **5 built-in backends**: Gemini, OpenAI, Anthropic, xAI (Grok), Ollama
- Unified priority-based automatic fallback
- Runtime RPK plugin extension — no recompilation
- Streaming responses with per-model token counting

### 📱 Direct Tizen C-API Access
- 35+ device APIs via ctypes FFI wrappers
- Battery, Wi-Fi, Bluetooth, Display, Sensors, Notifications, etc.
- Tizen Action Framework native integration
- per-action typed LLM tools with MD schema caching

</td>
<td width="50%">

### 🔒 Security First
- OCI container isolation (crun + seccomp + namespace)
- Device-bound encrypted API key storage
- Tool execution policy with risk levels & loop detection
- HMAC-SHA256 webhook authentication
- Structured Markdown audit logging

### 📡 7+ Communication Channels
- Telegram, Slack, Discord, MCP (Claude Desktop)
- Webhook, Voice (STT/TTS), Web Dashboard
- Pluggable `.so` channel plugins
- LLM-initiated outbound messaging & broadcast

### 🧠 Intelligence & Automation
- Agentic Loop with iterative tool calling
- Hybrid RAG search (BM25 + vector RRF)
- On-device ONNX embedding (all-MiniLM-L6-v2)
- Persistent long-term/episodic/short-term memory
- Cron/interval/weekly task scheduler

</td>
</tr>
</table>

### More Capabilities

| Category | Details |
|----------|---------|
| **Multi-Agent System** | 11-agent MVP set with supervisor pattern, skill pipelines, A2A cross-device protocol |
| **Skill Ecosystem** | 13 native CLI tool suites + 20+ built-in tools, RPK/TPK plugin distribution, inotify hot-reload |
| **Web Dashboard** | Dark glassmorphism SPA (port 9090), chat interface, session monitor, config editor, admin auth |
| **Workflow Engine** | Deterministic skill pipelines with variable interpolation and conditional branching |
| **Health Monitoring** | Prometheus-style `/api/metrics` endpoint with live dashboard panel |
| **OTA Updates** | Over-the-air skill updates with version checking and automatic rollback |
| **MCP Integration** | Built-in C++ MCP server + MCP client for external tool servers (Anthropic standard) |
| **Event-Driven Triggers** | Autonomous rule engine with LLM-based evaluation for context-aware actions |

---

## 🚀 Quick Start

### Prerequisites

- **Tizen 10.0** or later target device/emulator
- **GBS** (Git Build System) — [Tizen build tools](https://docs.tizen.org/platform/developing/installing/)
- **sdb** (Smart Development Bridge) for device deployment

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

The `deploy.sh` script handles building the RPM, installing it on the device, and restarting the daemon automatically.

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

Edit `llm_config.json` on the device or use the Web Dashboard config editor:

```json
{
  "active_backend": "gemini",
  "fallback_backends": ["openai", "ollama"],
  "backends": {
    "gemini": {
      "api_key": "YOUR_API_KEY",
      "model": "gemini-2.5-flash"
    },
    "openai": {
      "api_key": "YOUR_API_KEY",
      "model": "gpt-4o"
    },
    "ollama": {
      "model": "llama3",
      "endpoint": "http://localhost:11434"
    }
  }
}
```

> 💡 **Tip**: All configuration files can be edited through the [Web Dashboard](docs/DESIGN.md#web-dashboard) at port 9090.

---

## 🏗 Architecture

TizenClaw uses a **dual-container architecture** powered by OCI-compliant runtimes (`crun`) with **systemd socket activation** for on-demand service startup:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Communication Channels                              │
│  Telegram · Slack · Discord · MCP · Webhook · Voice (STT/TTS) · Dashboard  │
└──────┬──────────┬────────────┬─────────┬──────────────────────────┬─────────┘
       │          │            │         │                          │
       ▼          ▼            │         ▼                         ▼
┌──────────────────────────────┼────────────────────────────────────────────────┐
│  TizenClaw Daemon (systemd)  │                                               │
│                              │                                               │
│  ┌────────────────┐   ┌──────┴──────┐   ┌──────────────────────────────────┐ │
│  │ ChannelRegistry│──▶│  IPC Server │   │        LLM Backend Layer         │ │
│  └────────────────┘   │ (JSON-RPC   │   │  ┌────────┐ ┌────────┐          │ │
│                       │  2.0 / UDS) │   │  │ Gemini │ │ OpenAI │          │ │
│                       └──────┬──────┘   │  └────────┘ └────────┘          │ │
│                              │          │  ┌────────┐ ┌────────┐ ┌──────┐ │ │
│                              ▼          │  │Anthropic│ │ Ollama │ │Plugin│ │ │
│                       ┌─────────────┐   │  └────────┘ └────────┘ └──────┘ │ │
│                       │  AgentCore  │──▶│         (priority-based)         │ │
│                       │(Agentic Loop│   └──────────────────────────────────┘ │
│                       │ + Streaming)│                                        │
│                       └──┬───┬───┬──┘                                       │
│                ┌─────────┘   │   └──────────┐                               │
│                ▼             ▼               ▼                              │
│  ┌──────────────────┐ ┌───────────┐ ┌──────────────┐  ┌────────────────┐   │
│  │ ContainerEngine  │ │ Session   │ │ ActionBridge │  │ EmbeddingStore │   │
│  │  (crun OCI)      │ │ Store     │ │(Action FW)   │  │  (SQLite RAG)  │   │
│  └────┬────────┬────┘ └───────────┘ └──────┬───────┘  └────────────────┘   │
│       │        │                           │                                │
│       │        │     ┌─────────────────┐   │   ┌────────────────────────┐   │
│       │        │     │  TaskScheduler  │   │   │ WebDashboard (:9090)  │   │
│       │        │     └─────────────────┘   │   └────────────────────────┘   │
└───────┼────────┼───────────────────────────┼────────────────────────────────┘
        │        │                           │
        ▼        ▼                           ▼
┌──────────┐ ┌─────────────────┐   ┌──────────────────────┐
│Tool Exec │ │Secure Container │   │ Tizen Action          │
│(socket-  │ │    (crun)       │   │ Framework             │
│activated)│ │                 │   │                       │
│          │ │ Python Skills   │   │ Device-specific       │
│ CLI exec │ │ (sandboxed)     │   │ actions               │
│ via IPC  │ │ 13 CLI suites   │   │ (auto-discovered)     │
└──────────┘ └─────────────────┘   └──────────────────────┘
```

> 📖 For detailed architecture documentation, see [System Design](docs/DESIGN.md).

---

## 🧰 Skills & Tools

TizenClaw ships with **13 native CLI tool suites** and **20+ built-in tools**. All tools are registered in the Capability Registry with function contracts.

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

### Built-in Tools (Native C++)

| Category | Tools |
|----------|-------|
| **Code Execution** | `execute_code` (sandboxed Python) |
| **Task Management** | `create_task`, `list_tasks`, `cancel_task` |
| **Multi-Agent** | `create_session`, `list_sessions`, `send_to_session`, `run_supervisor` |
| **Knowledge (RAG)** | `ingest_document`, `search_knowledge` |
| **Workflow & Pipeline** | `create_workflow`, `run_workflow`, `create_pipeline`, `run_pipeline` |
| **Memory** | `remember`, `recall`, `forget` |
| **Device Actions** | `execute_action`, `action_<name>` (per-action), `execute_cli` |

📖 **Full reference**: [Tools Reference](docs/TOOLS.md)

### Extensibility

TizenClaw supports three extensibility mechanisms for tools:

| Mechanism | Package Type | Runtime | Use Case |
|-----------|:---:|:---:|----------|
| **[RPK Skill Plugins](docs/TOOLS.md#rpk-tool-distribution--extensibility)** | RPK | Python (OCI sandbox) | Sandboxed device analysis tools |
| **[CLI Tool Plugins](docs/TOOLS.md#cli-tool-plugins-tpk-based)** | TPK | Native binary (host) | Privileged Tizen C-API access |
| **[LLM Backend Plugins](https://github.com/hjhun/tizenclaw-llm-plugin-sample)** | RPK | Shared library | Custom LLM backends |

All plugins use platform-level certificate signing for security.

---

## ⚙️ Configuration

TizenClaw reads configuration from `/opt/usr/share/tizenclaw/` on the device. All files are editable via the **Web Dashboard** (port 9090).

| Config File | Purpose |
|---|---|
| `llm_config.json` | LLM backend selection, API keys, model settings, fallback order |
| `channels.json` | Channel activation and plugin paths |
| `telegram_config.json` | Telegram bot token and allowed chat IDs |
| `slack_config.json` | Slack app/bot tokens and channel lists |
| `discord_config.json` | Discord bot token and guild/channel allowlists |
| `webhook_config.json` | Webhook route mapping and HMAC secrets |
| `web_search_config.json` | Web search engine keys (Naver, Google, Brave, Gemini, Grok) |
| `tool_policy.json` | Tool execution policy (max iterations, blocked skills, risk overrides) |
| `agent_roles.json` | Agent roles and specialized system prompts |
| `memory_config.json` | Memory retention periods, size limits, summary parameters |
| `autonomous_trigger.json` | Autonomous trigger rules, cooldown, LLM evaluation settings |
| `fleet_config.json` | Fleet management endpoint, heartbeat interval (disabled by default) |

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

# Subsequent builds (faster)
gbs build -A x86_64 --include-all --noinit
```

Unit tests are automatically executed during the build via `%check`.

**RPM output**: `~/GBS-ROOT/local/repos/tizen/<arch>/RPMS/tizenclaw-1.0.0-1.<arch>.rpm`

### What Gets Installed

| Package | Description |
|---------|-------------|
| **`tizenclaw`** | Core AI daemon, Action Framework bridge, CLI tools, and built-in skills |
| **[`tizenclaw-assets`](https://github.com/hjhun/tizenclaw-assets)** | ONNX Runtime, RAG databases, embedding model, OCR engine *(recommended)* |

> `deploy.sh` automatically detects and builds `tizenclaw-assets` if it exists at `../tizenclaw-assets`.

---

## 📋 Project Structure

```
tizenclaw/
├── src/
│   ├── tizenclaw/                 # Daemon core (151 files across 7 subdirectories)
│   │   ├── core/                  # Agent core, policies, tools (55 files)
│   │   ├── llm/                   # LLM backend providers (14 files)
│   │   ├── channel/               # Communication channels (23 files)
│   │   ├── storage/               # Data persistence (8 files)
│   │   ├── infra/                 # Infrastructure (28 files)
│   │   ├── embedding/             # On-device ML embedding (5 files)
│   │   └── scheduler/             # Task automation (2 files)
│   ├── tizenclaw-cli/             # CLI client tool
│   ├── tizenclaw-tool-executor/   # Tool executor daemon (socket-activated)
│   ├── libtizenclaw/              # C-API client library (SDK)
│   ├── libtizenclaw-core/         # Core library (curl, LLM backend)
│   ├── pkgmgr-metadata-plugin/    # Metadata parser plugins
│   └── common/                    # Logging, shared utilities
├── tools/cli/                     # Native CLI tool suites (13 directories)
├── tools/embedded/                # Embedded tool MD schemas (17 files)
├── scripts/                       # Container setup, CI, hooks
├── tests/
│   ├── unit/                      # Google Test (42 test files)
│   ├── e2e/                       # E2E smoke tests
│   └── verification/              # Full verification suites
├── data/                          # Config, web dashboard, rootfs images
├── packaging/                     # RPM spec, systemd services & sockets
├── docs/                          # Design, analysis, roadmap
├── deploy.sh                      # Automated build & deploy script
├── CMakeLists.txt                 # Build system (C++20)
└── LICENSE                        # Apache License 2.0
```

---

## 📚 Documentation

| Document | Description |
|----------|-------------|
| **[System Design](docs/DESIGN.md)** | Architecture, module design, data flow, security model |
| **[Tools Reference](docs/TOOLS.md)** | Complete skill/tool catalog with parameters and C-API mapping |
| **[Project Analysis](docs/ANALYSIS.md)** | Code statistics, module inventory, competitive gap analysis |
| **[C-API Guide](docs/API_GUIDE.md)** | `libtizenclaw` SDK usage guide with code examples |
| **[ML/AI Assets](docs/ASSETS.md)** | RAG databases, ONNX Runtime, OCR engine, embedding model |
| **[Development Roadmap](docs/ROADMAP.md)** | Feature roadmap, completed phases, future enhancements |
| **[Multi-Agent Roadmap](docs/ROADMAP_MULTI_AGENT.md)** | 11-agent MVP set and perception architecture plan |
| **[PSS Memory Profiling](docs/PSS_PROFILING.md)** | Memory footprint optimization results |
| **[Supported Features](docs/FEATURES.md)** | Detailed supported/unsupported feature matrix |

---

## 🔗 Related Projects

| Project | Description |
|---------|-------------|
| **[tizenclaw-assets](https://github.com/hjhun/tizenclaw-assets)** | Consolidated ML/AI asset package — ONNX Runtime, RAG databases, embedding model, PaddleOCR engine |
| **[tizenclaw-webview](https://github.com/hjhun/tizenclaw-webview)** | Companion Tizen web app for on-device dashboard access |
| **[tizenclaw-llm-plugin-sample](https://github.com/hjhun/tizenclaw-llm-plugin-sample)** | Sample RPK plugin for custom LLM backends |
| **[tizenclaw-skill-plugin-sample](https://github.com/hjhun/tizenclaw-skill-plugin-sample)** | Sample RPK plugin for Python skill injection |
| **[tizenclaw-cli-plugin-sample](https://github.com/hjhun/tizenclaw-cli-plugin-sample)** | Sample TPK plugin for native CLI tools |

---

## 📊 Project Statistics

| Category | Count |
|----------|------:|
| C++ Source & Headers | 151 files (~34,200 LOC) |
| Python Skills & Utils | 36 files (~4,700 LOC) |
| Unit Tests | 42 files (~7,800 LOC) |
| Web Frontend | 3 files (~3,700 LOC) |
| **Total** | **~243 files (~52,150 LOC)** |

---

## 📄 License

This project is licensed under the [Apache License 2.0](LICENSE).

Copyright 2024-2026 Samsung Electronics Co., Ltd.
