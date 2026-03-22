# TizenClaw Feature Matrix — Python Port

> **Last Updated**: 2026-03-23
> **Branch**: `develPython`

This document provides a comprehensive matrix of all TizenClaw Python port features, organized by category, with their current implementation status compared to the C++ version.

---

## Legend

| Symbol | Meaning |
|:------:|---------| 
| ✅ | Fully implemented and verified |
| 🟡 | Partially implemented / stub |
| 🔴 | Not yet implemented / planned |
| ➖ | Not applicable to Python port |

---

## 1. Core Agent System

| Feature | C++ | Python | Details |
|---------|:---:|:------:|---------| 
| Agentic Loop (iterative tool calling) | ✅ | ✅ | Max 10 iterations in `AgentCore.process_prompt()` |
| LLM streaming responses | ✅ | 🟡 | Stub wrapping single response (`generate_stream`) |
| Context compaction | ✅ | 🔴 | Not yet implemented |
| Multi-session support | ✅ | ✅ | Per-session history with `asyncio.Lock` isolation |
| Edge memory management | ✅ | 🔴 | No `malloc_trim` equivalent (GC-managed) |
| JSON-RPC 2.0 IPC | ✅ | ✅ | Same protocol, same framing (`[4B len][JSON]`) |
| Concurrent client handling | ✅ | ✅ | asyncio cooperative concurrency (vs C++ thread pool) |
| UID authentication | ✅ | 🔴 | No `SO_PEERCRED` validation |
| System prompt externalization | ✅ | 🔴 | Hardcoded (no config fallback chain) |
| Dynamic tool injection | ✅ | ✅ | `ToolIndexer.get_tool_schemas()` feeds LLM |
| Auto-skill intercept | ✅ | ✅ | Direct tool execution for `get_device_info` queries |
| Parallel tool execution | ✅ | 🔴 | Sequential tool execution only |

## 2. LLM Backends

| Backend | C++ | Python | Default Model | Streaming | Token Counting |
|---------|:---:|:------:|:---:|:---------:|:--------------:|
| Google Gemini | ✅ | 🔴 | — | — | — |
| OpenAI | ✅ | ✅ | `gpt-4o` | 🟡 | 🔴 |
| Anthropic (Claude) | ✅ | 🔴 | — | — | — |
| xAI (Grok) | ✅ | 🔴 | — | — | — |
| Ollama (local) | ✅ | 🔴 | — | — | — |
| RPK Plugin backends | ✅ | 🔴 | — | — | — |

| Feature | C++ | Python | Details |
|---------|:---:|:------:|---------| 
| Unified priority switching | ✅ | 🔴 | Single backend only |
| Automatic fallback | ✅ | 🔴 | No fallback chain |
| API key encryption | ✅ | 🔴 | Environment variable only |
| Per-session usage tracking | ✅ | 🔴 | Not implemented |
| System prompt customization | ✅ | 🔴 | Hardcoded default |
| Zero external dependencies | 🔴 | ✅ | stdlib `urllib.request` + `asyncio.to_thread` |

## 3. Communication Channels

| Channel | C++ | Python | Details |
|---------|:---:|:------:|---------| 
| Telegram | ✅ | 🔴 | Not ported |
| Slack | ✅ | 🔴 | Not ported |
| Discord | ✅ | 🔴 | Not ported |
| MCP (Claude Desktop) | ✅ | ✅ | `--mcp-stdio` mode in daemon |
| Webhook | ✅ | 🔴 | Not ported |
| Voice (STT/TTS) | ✅ | 🔴 | Not ported |
| Web Dashboard | ✅ | ✅ | Static files preserved from C++ |
| SO Plugin | ✅ | 🔴 | Not applicable (no dlopen) |

| Feature | C++ | Python | Details |
|---------|:---:|:------:|---------| 
| Channel abstraction interface | ✅ | 🔴 | No ChannelRegistry |
| tizenclaw-cli | ✅ | ✅ | Full parity (`-s`, `--stream`, `--list-agents`, etc.) |
| IPC client library | ✅ | ✅ | `SocketClient` class in `tizenclaw_cli.py` |

## 4. Skills & Tool Ecosystem

### 4.1 Native CLI Tool Suites (13 directories)

| Category | Tools | C++ | Python | Notes |
|----------|:-----:|:---:|:------:|-------|
| App Management | 4 | ✅ | ✅ | Same CLI tools, executed via tool executor |
| Device Info | 7 | ✅ | ✅ | Same CLI tools, ctypes FFI |
| Network | 6 | ✅ | ✅ | Same CLI tools |
| Display & HW | 6 | ✅ | ✅ | Same CLI tools |
| Media | 5 | ✅ | ✅ | Same CLI tools |
| System | 6 | ✅ | ✅ | Same CLI tools |

> CLI tools are shared between C++ and Python versions — they are standalone executables.

### 4.2 Tool Discovery & Dispatch

| Feature | C++ | Python | Details |
|---------|:---:|:------:|---------| 
| ToolIndexer (schema scanning) | ✅ | ✅ | Regex YAML frontmatter parser |
| ToolDispatcher (routing) | ✅ | ✅ | `cli` / `skill` / `mcp` type routing |
| Capability Registry | ✅ | 🔴 | No FunctionContract system |
| O(1) tool lookup | ✅ | ✅ | `Dict[str, Dict]` hash map |
| `.tool.md` format | ✅ | ✅ | Same format, same parser |
| `.skill.md` format | ✅ | ✅ | Same format |
| `.mcp.json` format | ✅ | ✅ | JSON loading |

### 4.3 Embedded Tool Schemas (17 files)

| Tool | C++ | Python |
|------|:---:|:------:|
| `execute_code` | ✅ | ✅ |
| `create_task` / `list_tasks` / `cancel_task` | ✅ | ✅ |
| `create_session` | ✅ | ✅ |
| `ingest_document` / `search_knowledge` | ✅ | ✅ |
| `create/list/run/delete_workflow` | ✅ | ✅ |
| `create/list/run/delete_pipeline` | ✅ | ✅ |
| `run_supervisor` | ✅ | ✅ |
| `generate_web_app` | ✅ | ✅ |

### 4.4 Extensibility

| Feature | C++ | Python | Details |
|---------|:---:|:------:|---------| 
| RPK Skill Plugins | ✅ | 🔴 | SkillPluginManager not ported |
| CLI Tool Plugins (TPK) | ✅ | 🔴 | CliPluginManager not ported |
| LLM Backend Plugins | ✅ | 🔴 | PluginManager not ported |
| Channel Plugins (.so) | ✅ | ➖ | Not applicable |
| Skill hot-reload (inotify) | ✅ | 🔴 | No file watcher |
| SKILL.md format | ✅ | ✅ | Standard format, ToolIndexer parses it |

## 5. Security

| Feature | C++ | Python | Details |
|---------|:---:|:------:|---------| 
| OCI container isolation | ✅ | 🟡 | `unshare` fallback instead of crun |
| Tool execution policy | ✅ | 🔴 | No ToolPolicy class |
| Loop detection | ✅ | 🔴 | No repeat-detection |
| API key encryption | ✅ | 🔴 | Env var only |
| Audit logging | ✅ | 🔴 | No AuditLogger |
| UID authentication | ✅ | 🔴 | No SO_PEERCRED |
| Admin authentication | ✅ | 🔴 | No web auth |
| Payload size guard | 🟡 | ✅ | 10MB limit in daemon |

## 6. Knowledge & Intelligence

| Feature | C++ | Python | Details |
|---------|:---:|:------:|---------| 
| Hybrid RAG search | ✅ | 🟡 | Placeholder (vector-only fallback) |
| On-device embedding | ✅ | 🟡 | ONNX session loads, but tokenizer missing → zero vector |
| SQLite FTS5 | ✅ | ✅ | FTS5 virtual table created |
| Multi-DB support | ✅ | ✅ | `ATTACH DATABASE` implemented |
| Token budget estimation | ✅ | ✅ | `words × 1.3` |
| Cosine similarity | ✅ | ✅ | Pure Python math implementation |
| Text chunking | ✅ | ✅ | Sliding window with overlap |
| Persistent memory | ✅ | ✅ | Long-term/episodic/short-term stores |
| Memory summary | ✅ | 🟡 | Stub `regenerate_summary()` |

## 7. Automation & Orchestration

| Feature | C++ | Python | Details |
|---------|:---:|:------:|---------| 
| Task scheduler | ✅ | ✅ | asyncio-based (cron/interval/once/weekly) |
| Workflow engine | ✅ | ✅ | Markdown parsing + variable interpolation |
| Variable interpolation | ✅ | ✅ | `{{variable}}` in instructions and args |
| Conditional branching | ✅ | 🔴 | Not implemented in workflow parser |
| Supervisor agent | ✅ | 🔴 | No SupervisorEngine |
| Skill pipelines | ✅ | 🟡 | Via WorkflowEngine steps |
| Autonomous triggers | ✅ | 🔴 | No AutonomousTrigger |
| Event Bus | ✅ | 🔴 | No pub/sub system |
| A2A protocol | ✅ | 🔴 | No cross-device protocol |

## 8. Operations & Deployment

| Feature | C++ | Python | Details |
|---------|:---:|:------:|---------| 
| systemd service | ✅ | ✅ | `tizenclaw.service` (Python script) |
| Socket activation | ✅ | ✅ | Tool executor + code sandbox sockets |
| GBS RPM packaging | ✅ | ✅ | Install-only CMake (`LANGUAGES NONE`) |
| Automated deploy | ✅ | ✅ | `deploy.sh` script |
| Web Dashboard | ✅ | ✅ | Static files (5 files, ~3,900 LOC) |
| Health metrics | ✅ | 🔴 | No `/api/metrics` |
| OTA updates | ✅ | 🔴 | No OtaUpdater |
| Fleet management | 🟡 | 🔴 | Not ported |
| Secure tunneling | ✅ | 🔴 | No TunnelManager |
| Debug service | ✅ | ✅ | `tizenclaw-debug.service` |

## 9. MCP (Model Context Protocol)

| Feature | C++ | Python | Details |
|---------|:---:|:------:|---------| 
| MCP Server (built-in) | ✅ | ✅ | `--mcp-stdio` mode |
| MCP Client (built-in) | ✅ | 🔴 | No McpClientManager |
| MCP Sandbox | ✅ | 🔴 | No container-based MCP server |
| Tools exposed via MCP | ✅ | ✅ | All ToolIndexer schemas available |

## 10. Testing

| Feature | C++ | Python | Details |
|---------|:---:|:------:|---------| 
| Unit tests (gtest) | ✅ | ➖ | Legacy C++ test files remain but not compiled |
| Shell verification tests | ✅ | ✅ | 28 test scripts in `tests/verification/` |
| E2E smoke tests | ✅ | ✅ | `tests/e2e/` |
| CLI tool validation | ✅ | ✅ | `tests/verification/cli_tools/` (13 tests) |
| MCP compliance tests | ✅ | ✅ | `tests/verification/mcp/` (2 tests) |
| LLM integration tests | ✅ | ✅ | `tests/verification/llm_integration/` (3 tests) |
| Regression tests | ✅ | ✅ | `tests/verification/regression/` |
| Python unit tests (pytest) | ➖ | 🔴 | Not yet created |

## 11. Tizen Native Integration

| Feature | C++ | Python | Details |
|---------|:---:|:------:|---------| 
| Tizen dlog routing | ✅ | ✅ | `ctypes` → `libdlog.so.0` |
| System event handler | ✅ | ✅ | `ctypes` → `libcapi-appfw-app-common.so.0` |
| vconf integration | ✅ | 🟡 | Placeholder in NativeWrapper |
| Action Framework | ✅ | 🔴 | No ActionBridge |

---

## Summary: Python Port Coverage

| Category | C++ Features | Python Ported | Coverage |
|----------|:---:|:---:|:---:|
| Core Agent | 11 | 6 | 55% |
| LLM Backends | 6 + 5 features | 1 + 1 feature | ~18% |
| Channels | 8 | 2 (CLI + MCP) | 25% |
| Tools & Skills | 13 CLI + 17 embedded | 13 CLI + 17 embedded | 100% |
| Security | 8 | 1 | 13% |
| Knowledge | 8 | 5 | 63% |
| Automation | 9 | 2 | 22% |
| Operations | 10 | 5 | 50% |
| MCP | 4 | 2 | 50% |
| Testing | 7 | 5 | 71% |

> **Overall**: The Python port provides core agent functionality (agentic loop, tool dispatch, CLI parity) with the same tool ecosystem. Areas like multi-backend LLM, channels, security, and advanced automation remain to be ported.
