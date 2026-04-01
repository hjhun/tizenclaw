# Agent Loop Comparison & Improvement Plan

## 1. Comparison of Agent Loops

| Feature | TizenClaw (Current) | OpenClaw | HermesAgent | Suggestions |
| :--- | :--- | :--- | :--- | :--- |
| **Language** | Rust (Tokio) | TypeScript (Node.js) | Python (Asyncio) | - |
| **Tool Execution** | Parallel (`join_all`) | Lane-based (Queued) | ThreadPool (Sync) | **Lanes/Queuing**: Add per-session lanes to prevent race conditions on shared state. |
| **Context Management** | SQLite SessionStore | ContextEngine (Compaction) | List-based | **Compaction**: Implement proactive context compaction upon timeout or overflow. |
| **LLM Failover** | Simple Ordered Fallback | Sophisticated rotation + Auth Profiles | Implicit (Server-side) | **Failover**: Enhance failover reason classification (e.g., rate-limit, billing). |
| **Reliability** | Structured Only | Structured + Fallback Tags | Structured + Fallback Tags | **Text Parser**: Add regex-based fallback for tool calls in text content. |
| **Reasoning** | Implicit in msg | `ThinkLevel` mapping | Explicit extraction | **Explicit Reasoning**: Store reasoning content separately in session history. |
| **Hooks** | None | Extensive (runtime plugins) | None | **Hooks**: Introduce a basic hook system for lifecycle events. |

## 2. Implementation Objectives

### [MODIFY] `src/tizenclaw/src/core/agent_core.rs`
- **Reasoning Extraction**: Capture and store reasoning content from LLM responses before tool execution.
- **Fallback Tool Parser**: Add a secondary parser to detect tool calls in plain text if structured calls are missing.
- **Failover Classification**: Improve `chat_with_fallback` to handle different error types (rate limits, context window) differently.

### [NEW] `src/tizenclaw/src/core/context_engine.rs` (Conceptual Plan)
- **Proactive Compaction**: Logic to reduce context size when nearing limits or encountering timeouts.

### [NEW] `src/tizenclaw/src/core/hooks.rs` (Conceptual Plan)
- **Lifecycle Hooks**: `on_tool_call`, `on_context_overflow`, etc.

## 3. Execution Mode Classification

1. **Reasoning Storage**: **One-shot Worker** (during process_prompt)
2. **Fallback Parser**: **One-shot Worker** (during process_prompt)
3. **Context Compaction**: **Daemon Sub-task** (background or inline recovery)
4. **Lane Management**: **Daemon Sub-task** (scheduler integration)

## 4. Tizen System API Integration
- No direct Tizen API changes for this core logic improvement, but improved reliability benefits all Tizen-specific tool executions.
