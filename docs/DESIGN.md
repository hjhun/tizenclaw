# TizenClaw System Design Document — Python Port

> **Last Updated**: 2026-03-23
> **Version**: 4.0 (develPython branch)

---

## Table of Contents

- [1. Overview](#1-overview)
- [2. System Architecture](#2-system-architecture)
- [3. Core Modules](#3-core-modules)
- [4. LLM Backend Layer](#4-llm-backend-layer)
- [5. Communication & IPC](#5-communication--ipc)
- [6. Container & Skill Execution](#6-container--skill-execution)
- [7. Data Persistence & Storage](#7-data-persistence--storage)
- [8. RAG & Semantic Search](#8-rag--semantic-search)
- [9. Workflow Engine](#9-workflow-engine)
- [10. Task Scheduler](#10-task-scheduler)
- [11. Tizen Native Integration](#11-tizen-native-integration)
- [12. Web Dashboard](#12-web-dashboard)
- [13. Design Principles](#13-design-principles)

---

## 1. Overview

**TizenClaw (Python Port)** is the `develPython` branch rewrite of TizenClaw, porting the entire C++20 daemon to **pure Python 3** to evaluate memory, speed, and storage footprints on Tizen embedded devices. It runs as a **systemd service**, receiving user prompts through IPC (JSON-RPC 2.0 over Unix Domain Sockets), interpreting them via an OpenAI-compatible LLM backend, and executing device-level actions using native CLI tool suites via a containerized tool executor.

The Python port maintains the same IPC protocol, tool schema format, and CLI interface as the C++ version, ensuring backward compatibility with existing tools and testing infrastructure.

### System Environment

| Property | Details |
|----------|---------|
| **OS** | Tizen Embedded Linux (Tizen 10.0+) |
| **Runtime** | systemd daemon with socket-activated companion services |
| **Language** | Python 3.x (daemon, CLI, tool executor, all modules) |
| **External Dependencies** | Zero (stdlib only: asyncio, json, urllib, sqlite3, ctypes) |
| **HTTP Client** | `urllib.request` with `asyncio.to_thread` offloading |
| **IPC** | Abstract Unix Domain Sockets, JSON-RPC 2.0 |
| **LLM Backend** | OpenAI-compatible REST API (gpt-4o default) |

### Design Goals

1. **Zero External Dependencies** — Use only Python stdlib for maximum portability
2. **C++ Parity Evaluation** — Same IPC protocol, tool schemas, and CLI interface
3. **Asyncio-First** — Cooperative concurrency without threading complexity
4. **Platform Integration** — Tizen C-API access via `ctypes` FFI
5. **Comparison Metrics** — Memory, CPU, binary size benchmarking against C++ version

---

## 2. System Architecture

### High-Level Architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│                    External Interfaces                                │
│    tizenclaw-cli (Python)  ·  MCP stdio  ·  Web Dashboard (:9090)   │
└────────┬──────────────────────┬──────────────────────────┬──────────┘
         │                      │                          │
         ▼                      ▼                          ▼
┌──────────────────────────────────────────────────────────────────────┐
│  TizenClaw Daemon  (tizenclaw_daemon.py / systemd)                  │
│                                                                      │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │  IPC Server (asyncio.start_unix_server)                       │  │
│  │  Protocol: JSON-RPC 2.0, [4-byte len][JSON] framing           │  │
│  │  Socket: abstract namespace \0tizenclaw.sock                  │  │
│  └──────────────────────┬────────────────────────────────────────┘  │
│                          │                                           │
│  ┌───────────────────────▼───────────────────────────────────────┐  │
│  │  AgentCore (agent_core.py)                                    │  │
│  │  • Agentic Loop (max 10 tool iterations)                      │  │
│  │  • Auto-skill intercept for direct device queries             │  │
│  │  • Per-session history with asyncio.Lock                      │  │
│  │  • LLM tool schema injection via ToolIndexer                  │  │
│  └──┬──────────┬──────────┬──────────┬──────────┬───────────────┘  │
│     │          │          │          │          │                    │
│     ▼          ▼          ▼          ▼          ▼                   │
│  ToolIndex  ToolDisp   OpenAI    Session   Embedding               │
│  er         atcher     Backend   /Memory   Store                   │
│  (.md scan) (routing)  (urllib)  Store     (SQLite)                 │
│                                                                      │
│  ┌────────────────────────────────────────────────────────────────┐  │
│  │  Additional Modules                                           │  │
│  │  • WorkflowEngine — Markdown-based deterministic pipelines    │  │
│  │  • TaskScheduler — asyncio-based cron/interval scheduling     │  │
│  │  • MemoryStore — Long-term/episodic/short-term (Markdown)     │  │
│  │  • TizenSystemEventAdapter — ctypes app_event integration     │  │
│  └────────────────────────────────────────────────────────────────┘  │
└──────────┬───────────────────────────────────────────────────────────┘
           │
           ▼
┌──────────────────────────────────────────────────────────────────────┐
│  ContainerEngine (container_engine.py)                                │
│  Communicates with tool executor via abstract UDS IPC                │
│                                                                      │
│  ┌─────────────────────┐    ┌─────────────────────┐                 │
│  │ tizenclaw-tool-     │    │ tizenclaw-code-     │                 │
│  │ executor.py         │    │ sandbox.py          │                 │
│  │ (socket-activated)  │    │ (socket-activated)  │                 │
│  │ asyncio subprocess  │    │ stub listener       │                 │
│  └─────────────────────┘    └─────────────────────┘                 │
│                                                                      │
│  13 CLI Tool Suites (ctypes FFI → Tizen C-API)                      │
└──────────────────────────────────────────────────────────────────────┘
```

### Service Topology

```
systemd
├── tizenclaw.service           (Type=simple, ExecStart=/usr/bin/tizenclaw)
├── tizenclaw-tool-executor.socket  (ListenStream, socket activation)
│   └── tizenclaw-tool-executor.service
└── tizenclaw-code-sandbox.socket   (ListenStream, socket activation)
    └── tizenclaw-code-sandbox.service
```

### Request Flow (Agentic Loop)

```
User ──▶ tizenclaw-cli ──▶ IPC Socket ──▶ AgentCore.process_prompt()
                                                 │
                                          ┌──────▼──────┐
                                          │  Loop (max  │
                                          │  10 iters)  │
                                          └──┬──────┬───┘
                                             │      │
                                     Has tool │      │ Text only
                                     calls   │      │ → return
                                             ▼      
                                      ToolDispatcher
                                      .execute_tool()
                                             │
                                    ┌────────┼────────┐
                                    ▼        ▼        ▼
                                  cli      skill     mcp
                                Container  Container Container
                                Engine     Engine    Engine
                                    │
                                    ▼
                            Tool Executor
                            (subprocess)
                                    │
                                    ▼
                              CLI binary
                            (Tizen C-API)
```

---

## 3. Core Modules

### 3.1 Daemon Process (`tizenclaw_daemon.py`)

The main daemon process manages the overall lifecycle:

| Responsibility | Implementation |
|---------------|---------------|
| **systemd integration** | `Type=simple` service, graceful `KeyboardInterrupt` handling |
| **IPC Server** | `asyncio.start_unix_server` on abstract socket (`\0tizenclaw.sock`) |
| **Protocol** | JSON-RPC 2.0 with `[4-byte network-endian len][JSON]` framing |
| **Payload guard** | Reject payloads > 10MB |
| **MCP Mode** | `--mcp-stdio` flag for Claude Desktop integration (stdin/stdout JSON-RPC) |
| **Methods** | `prompt`, `connect_mcp`, `list_mcp`, `list_agents` |

### 3.2 Agent Core (`agent_core.py`)

The central orchestration engine implementing the **Agentic Loop**:

| Feature | Details |
|---------|---------|
| **Iterative Tool Calling** | LLM → tool_calls → execute → feed results → repeat (max 10) |
| **Auto-Skill Intercept** | Direct tool execution for known queries (e.g., `get_device_info`) bypassing LLM |
| **Multi-Session** | Per-session message history with `asyncio.Lock` isolation |
| **Tool Integration** | `ToolIndexer` provides schemas, `ToolDispatcher` executes |
| **Initialization** | Lazy loading of ToolIndexer, ContainerEngine, OpenAiBackend |
| **Work Directory** | `/opt/usr/share/tizenclaw/work/sessions/` |

### 3.3 Tool Indexer (`tool_indexer.py`)

Discovers and parses tool schemas from the filesystem:

| Source | Path Pattern | Parser |
|--------|-------------|--------|
| CLI tools | `*.tool.md` | YAML frontmatter regex extraction |
| Skills | `*.skill.md` | YAML frontmatter regex extraction |
| MCP tools | `*.mcp.json` | JSON file loading |

- Base directory: `/opt/usr/share/tizenclaw/tools/`
- Generates `tools.md` index and `skills/index.md` at load time
- `get_tool_schemas()` returns list of `{name, description, parameters}` for LLM

### 3.4 Tool Dispatcher (`tool_dispatcher.py`)

Routes LLM tool calls to the appropriate execution backend:

| Tool Type | Execution Path |
|-----------|---------------|
| `cli` | `ContainerEngine.execute_cli_tool()` |
| `skill` | `ContainerEngine.execute_skill()` |
| `mcp` | `ContainerEngine.execute_mcp_tool()` |

- Validates tool existence via `ToolIndexer.get_tool_metadata()`
- Handles argument serialization (dict → JSON string)
- Unified error reporting back to AgentCore

---

## 4. LLM Backend Layer

### Abstract Interface (`llm_backend.py`)

```python
class LlmBackend(ABC):
    async def initialize(self, config: Dict) -> bool: ...
    async def chat(self, messages, tools, on_chunk, system_prompt) -> LlmResponse: ...
    def get_name(self) -> str: ...
```

### Data Types

| Type | Fields |
|------|--------|
| `LlmMessage` | `role`, `text`, `tool_calls`, `tool_name`, `tool_call_id` |
| `LlmResponse` | `success`, `text`, `tool_calls`, `prompt_tokens`, `completion_tokens` |
| `LlmToolCall` | `id`, `name`, `args` |
| `LlmToolDecl` | `name`, `description`, `parameters_schema` |

### OpenAI Backend (`openai_backend.py`)

| Feature | Implementation |
|---------|---------------|
| **HTTP** | `urllib.request` with `asyncio.to_thread` (zero dependencies) |
| **Model** | `gpt-4o` (default), configurable via constructor |
| **API Key** | Constructor param or `OPENAI_API_KEY` env var |
| **Tool Calling** | OpenAI function calling format (`tools` + `tool_choice: auto`) |
| **Streaming** | Wrapper over single response (full streaming not yet implemented) |
| **Timeout** | 30 seconds per request |

### Comparison with C++ Version

| Feature | C++ (main/devel) | Python (develPython) |
|---------|:---:|:---:|
| **Backends** | 5 (Gemini, OpenAI, Anthropic, xAI, Ollama) | 1 (OpenAI-compatible) |
| **Priority switching** | Yes (unified priority queue) | No (single backend) |
| **Automatic fallback** | Yes (sequential retry) | No |
| **API key encryption** | Device-bound (GLib SHA-256 + XOR) | Environment variable |
| **Streaming** | Chunked SSE parsing | Stub (wraps single response) |
| **HTTP library** | libcurl | urllib.request |

---

## 5. Communication & IPC

### IPC Protocol

| Property | Value |
|----------|-------|
| **Socket** | Abstract Unix Domain Socket (`\0tizenclaw.sock`) |
| **Framing** | `[4-byte network-endian length][JSON payload]` |
| **Protocol** | JSON-RPC 2.0 |
| **Concurrency** | asyncio `StreamReader`/`StreamWriter` per client |

### Supported Methods

| Method | Parameters | Description |
|--------|-----------|-------------|
| `prompt` | `session_id`, `text`, `stream` | Process natural language prompt |
| `connect_mcp` | `config_path` | Load MCP tools from config |
| `list_mcp` | — | List connected MCP tools |
| `list_agents` | — | List running agents |

### MCP Server Mode

When started with `--mcp-stdio`, the daemon operates as an MCP stdio server:

| MCP Method | Implementation |
|-----------|---------------|
| `initialize` | Returns protocol version, capabilities, server info |
| `tools/list` | Returns all ToolIndexer schemas + `ask_tizenclaw` |
| `tools/call` | Dispatches via ToolDispatcher |
| `notifications/*` | Silently ignored |

### CLI Client (`tizenclaw_cli.py`)

| Feature | Implementation |
|---------|---------------|
| **Connection** | `socket.AF_UNIX` to `\0tizenclaw.sock` |
| **Timeout** | 10 seconds |
| **Commands** | `--list-agents`, `--connect-mcp`, `--list-mcp`, `--stream`, positional prompt |
| **Session** | `-s` / `--session` flag (default: `cli_test`) |

---

## 6. Container & Skill Execution

### Container Engine (`container_engine.py`)

| Feature | Implementation |
|---------|---------------|
| **IPC** | Abstract UDS (`\0tizenclaw-tool-executor.sock`) |
| **Protocol** | Length-prefixed JSON (4-byte length) |
| **Timeout** | Configurable per-request (default 30s) |
| **CLI Execution** | `execute_cli_tool(name, args, timeout)` |
| **Skill Execution** | `execute_skill(skill_name, args)` |

### Tool Executor (`tizenclaw_tool_executor.py`)

Socket-activated service that executes tools via `asyncio.create_subprocess_exec`:

| Feature | Details |
|---------|---------|
| **Socket** | Abstract namespace `\0tizenclaw_tool_executor.sock` |
| **Activation** | systemd socket activation |
| **Execution** | `asyncio.create_subprocess_exec(command, *args)` |
| **Response** | `{status, stdout, stderr, exit_code}` JSON |

### Code Sandbox (`tizenclaw_code_sandbox.py`)

Minimal stub service for sandboxed code execution:
- Listens on `\0tizenclaw_code_sandbox.sock`
- Currently a placeholder (no-op handler)

### Comparison with C++ Version

| Feature | C++ | Python |
|---------|:---:|:---:|
| **Container runtime** | crun 1.26 (OCI) | unshare fallback |
| **Isolation** | PID/Mount/User namespaces + seccomp | OS-level unshare |
| **Process exec** | `popen()` / `crun exec` | `asyncio.create_subprocess_exec` |
| **Tool Executor IPC** | C++ length-prefix UDS | Python length-prefix UDS |

---

## 7. Data Persistence & Storage

### Session Store (`session_store.py`)

| Feature | Implementation |
|---------|---------------|
| **Format** | JSON serialized as Markdown (YAML frontmatter planned) |
| **Path** | `/opt/usr/share/tizenclaw/sessions/{session_id}.md` |
| **Atomic write** | Write to `.tmp` then `os.replace()` |
| **Logging** | Daily skill execution log (`skills_YYYY-MM-DD.log`, JSON-lines) |

### Memory Store (`memory_store.py`)

Three-tier memory system with Markdown persistence:

| Type | Subdir | Retention | Max Size |
|------|--------|-----------|----------|
| **Short-term** | `short_term/` | 24h, max 50 entries | — |
| **Long-term** | `long_term/` | Permanent | 2KB/file |
| **Episodic** | `episodic/` | 30 days | 2KB/file |

Each entry is serialized with YAML frontmatter:

```yaml
---
type: long_term
title: User Preference
importance: medium
created: 2026-03-23T07:00:00
updated: 2026-03-23T07:00:00
---
Content text here...
```

### Configuration

`MemoryConfig` defaults:

| Setting | Default |
|---------|---------|
| `short_term_max_age_hours` | 24 |
| `short_term_max_entries` | 50 |
| `long_term_max_file_bytes` | 2048 |
| `episodic_max_age_days` | 30 |
| `summary_max_bytes` | 8192 |

---

## 8. RAG & Semantic Search

### EmbeddingStore (`embedding_store.py`)

| Feature | Implementation |
|---------|---------------|
| **Storage** | SQLite3 with FTS5 virtual table |
| **Vector Storage** | Embeddings as `BLOB` (struct-packed floats) |
| **Search** | Brute-force cosine similarity over all embeddings |
| **Hybrid Search** | FTS5 keyword + vector (placeholder, currently vector-only) |
| **Token Budget** | `estimate_tokens(text)` → `len(text.split()) * 1.3` |
| **Chunking** | `chunk_text(text, chunk_size=500, overlap=50)` |
| **Multi-DB** | `ATTACH DATABASE` for external knowledge bases |

### On-Device Embedding (`on_device_embedding.py`)

| Feature | Details |
|---------|---------|
| **Model** | `all-MiniLM-L6-v2` (384-dim) |
| **Runtime** | ONNX Runtime (`onnxruntime` + `numpy`) |
| **Provider** | `CPUExecutionProvider` (for armv7l) |
| **Initialization** | Lazy-loaded on first use |
| **Fallback** | Returns zero vector if model unavailable |
| **Tokenizer** | Placeholder (WordPiece tokenizer not yet integrated) |

---

## 9. Workflow Engine

### WorkflowEngine (`workflow_engine.py`)

Executes Markdown-based deterministic skill pipelines:

| Feature | Details |
|---------|---------|
| **Persistence** | `/opt/usr/share/tizenclaw/workflows/*.md` |
| **Step Types** | `prompt` (LLM), `tool` (ToolDispatcher) |
| **Variable Interpolation** | `{{variable_name}}` in instructions and args |
| **Output Capture** | `output_var` for step result chaining |
| **Error Handling** | `skip_on_failure` per step |
| **Retry** | `max_retries` per step |
| **Parsing** | YAML frontmatter + `## Step` heading extraction |

### Workflow Markdown Format

```markdown
---
id: my_workflow
name: Device Health Check
description: Check device health metrics
---

## Step 1
Tool: get_device_info
Output: device_info

## Step 2
Prompt: Summarize this device info: {{device_info}}
Output: summary
```

---

## 10. Task Scheduler

### TaskScheduler (`task_scheduler.py`)

| Feature | Implementation |
|---------|---------------|
| **Schedule Types** | `once`, `daily`, `weekly`, `interval` |
| **Execution** | Two asyncio tasks: `_scheduler_loop` + `_executor_loop` |
| **Queue** | `asyncio.Queue` for pending task execution |
| **Concurrency** | `asyncio.Lock` for task dictionary access |
| **Retry** | Max 3 retries per task |
| **Status** | `active`, `paused`, `completed`, `failed`, `cancelled` |

---

## 11. Tizen Native Integration

### Tizen Dlog Handler (`tizen_dlog.py`)

Routes Python `logging` to Tizen's native dlog system:

| Feature | Details |
|---------|---------|
| **Library** | `libdlog.so.0` via ctypes |
| **Log Tag** | `TIZENCLAW` |
| **Priority Mapping** | `DEBUG→3`, `INFO→4`, `WARN→5`, `ERROR→6` |
| **Fallback** | No-op if dlog library not found (non-Tizen environments) |

### System Event Adapter (`tizen_system_event_adapter.py`)

| Feature | Details |
|---------|---------|
| **Library** | `libcapi-appfw-app-common.so.0` via ctypes |
| **Events** | Battery charger/level, USB status, network state |
| **Callback** | ctypes `CFUNCTYPE` C callback registration |
| **Fallback** | Mock mode if library not found |

### Native Wrapper (`native_wrapper.py`)

Placeholder ctypes wrapper for:
- `libdlog.so` — Tizen logging
- `libvconf.so` — Tizen virtual configuration

---

## 12. Web Dashboard

The Web Dashboard from the C++ version is preserved as static files in `data/web/`:
- Port 9090
- 5 frontend files (~3,900 LOC)
- Dark glassmorphism SPA design

> **Note**: In the Python port, the web server is currently served by the same infrastructure as the C++ version. A Python-native web server (e.g., `asyncio` HTTP) may be implemented in a future iteration.

---

## 13. Design Principles

### Zero-Dependency Python

1. **stdlib only** — No pip packages required (except optional `onnxruntime`/`numpy` for embedding)
2. **asyncio-first** — All I/O operations use async/await with cooperative scheduling
3. **ctypes FFI** — Direct Tizen C-API access without compilation

### C++ Parity Protocol

- Same IPC socket path and framing protocol
- Same tool schema format (`.tool.md` YAML frontmatter)
- Same CLI interface (`tizenclaw-cli` with identical flags)
- Same systemd service/socket topology

### Lazy Initialization

- ONNX Runtime loaded only on first embedding request
- Tizen C-API libraries loaded only if available
- Work directories created on-demand

### Schema-Execution Separation

- Markdown schema files provide LLM context
- Execution logic handled by ToolDispatcher → ContainerEngine
- Schema updates don't require code changes

---

## Appendix: Technology Stack

| Component | Technology |
|-----------|-----------|
| **Language** | Python 3.x |
| **Build System** | CMake (install-only, `LANGUAGES NONE`), GBS (RPM) |
| **HTTP** | `urllib.request` (client), Web Dashboard static files |
| **Database** | SQLite3 (RAG, FTS5) |
| **ML Inference** | ONNX Runtime (optional, lazy-loaded) |
| **Container** | `unshare` fallback (OCI-compatible) |
| **IPC** | Abstract Unix Domain Sockets, JSON-RPC 2.0 |
| **JSON** | stdlib `json` module |
| **Logging** | `logging` → Tizen dlog (ctypes) |
| **Concurrency** | `asyncio` (event loop, tasks, locks) |
| **Packaging** | RPM (GBS), systemd services/sockets |

## Appendix: Module Inventory

| Module | File | LOC | Description |
|--------|------|----:|-------------|
| Daemon | `tizenclaw_daemon.py` | 179 | IPC server + MCP stdio |
| CLI | `tizenclaw_cli.py` | 101 | CLI client |
| Tool Executor | `tizenclaw_tool_executor.py` | 62 | Socket-activated tool runner |
| Code Sandbox | `tizenclaw_code_sandbox.py` | 19 | Stub sandbox listener |
| AgentCore | `core/agent_core.py` | 143 | Agentic loop orchestration |
| ToolIndexer | `core/tool_indexer.py` | 119 | Tool schema discovery |
| ToolDispatcher | `core/tool_dispatcher.py` | 44 | Tool routing |
| WorkflowEngine | `core/workflow_engine.py` | 162 | Pipeline execution |
| LlmBackend | `llm/llm_backend.py` | 70 | Abstract LLM interface |
| OpenAiBackend | `llm/openai_backend.py` | 99 | OpenAI REST client |
| ContainerEngine | `infra/container_engine.py` | 66 | Tool executor IPC |
| EventAdapter | `infra/tizen_system_event_adapter.py` | 98 | ctypes event handler |
| SessionStore | `storage/session_store.py` | 81 | Session persistence |
| MemoryStore | `storage/memory_store.py` | 117 | Tiered memory system |
| EmbeddingStore | `storage/embedding_store.py` | 152 | SQLite RAG store |
| OnDeviceEmbedding | `embedding/on_device_embedding.py` | 69 | ONNX inference |
| TaskScheduler | `scheduler/task_scheduler.py` | 108 | Cron/interval scheduler |
| TizenDlog | `utils/tizen_dlog.py` | 58 | Logging to dlog |
| NativeWrapper | `utils/native_wrapper.py` | 26 | ctypes C-API bindings |
| **Total** | | **~1,773** | |
