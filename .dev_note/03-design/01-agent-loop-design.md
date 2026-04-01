# TizenClaw Agent Loop Enhancement: Architecture Blueprint

## 1. Objective
Enhance the TizenClaw agent loop with reasoning extraction, fallback tool parsing, and proactive context compaction.

## 2. Core Components

### 2.1 reasoning_text field
- **LlmResponse**: Add `reasoning_text: String`.
- **LlmMessage**: Add `reasoning_text: String`.
- **Extraction Logic**: 
    - Extract from LLM-specific fields (e.g., Gemini/OpenAI reasoning).
    - Regex fallback for `<think>...</think>` tags in `text`.

### 2.2 Fallback Tool Parser
- **Location**: `src/tizenclaw/src/core/fallback_parser.rs`
- **Responsibility**: Parse tool calls from plain text if `tool_calls` is empty.
- **Formats**:
    - `<tool_call>name(args)</tool_call>`
    - `{"tool": "name", "args": {...}}` (JSON block)

### 2.3 Context Engine
- **Location**: `src/tizenclaw/src/core/context_engine.rs`
- **Trait**: `ContextEngine`
- **Implementation**: `SimpleContextEngine` (Truncation/Summarization).
- **Integration**: `AgentCore::process_prompt` will check budget before calling LLM.

## 3. Data Flow & Sequential Logic

1.  **Preparation**:
    - Build `messages`.
    - Check context budget via `ContextEngine`.
    - If budget > 80%, perform **Proactive Compaction**.
2.  **LLM Call**:
    - `chat_with_fallback`.
3.  **Post-processing**:
    - Extract `reasoning_text`.
    - If `tool_calls` is empty, run `FallbackParser`.
    - If `tool_calls` exist, execute tools.
4.  **Error Recovery**:
    - If "Context Overflow" error occurs, perform **Reactive Compaction** and retry.

## 4. Concurrency & Safety
- **Tokio Loop**: The main loop in `process_prompt` remains the owner.
- **Thread Safety**: All new components must be `Send + Sync`.
- **No FFI usage**: All logic is implemented in pure Rust.

## 5. Implementation Path (Development Stage)
1.  Update `backend.rs` structs.
2.  Implement `fallback_parser.rs`.
3.  Implement `context_engine.rs`.
4.  Refactor `agent_core.rs` loop.
