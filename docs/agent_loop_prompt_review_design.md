# Agent Loop And System Prompt Review Design

## Review Method

The review uses first-party code or official project documentation for
each comparison target. The design goal is to extract transferable
architecture patterns without importing assumptions that conflict with
TizenClaw's Rust daemon runtime.

## Comparison Axes

### 1. Loop Topology

- Session serialization or concurrency model
- Tool-call iteration model
- Retry and termination behavior
- Sub-agent execution boundaries

### 2. System Prompt Topology

- Prompt ownership model
- Stable versus dynamic prompt layers
- Runtime context placement
- Role/sub-agent prompt minimization

### 3. Context And Memory Injection

- Bootstrap file injection
- Long-term memory placement
- Dynamic plugin or hook context injection
- Compaction and prompt-cache stability strategy

### 4. Safety And Operational Controls

- Prompt-injection defenses on local context files
- Tool-use enforcement guidance
- Approval and execution guidance
- Context budgeting and truncation behavior

## TizenClaw Compatibility Filter

Candidate ideas are accepted only when they satisfy all of the following:

1. They keep the core agent logic in pure Rust.
2. They do not require new Tizen FFI boundaries.
3. They preserve `Send + Sync` ownership expectations of current shared
   runtime state.
4. They fit a multi-backend LLM design rather than a single vendor SDK.
5. They can be introduced incrementally without destabilizing the daemon
   loop.

## Architectural Constraints

- No `libloading` strategy change is introduced in this review cycle.
- No new FFI bridge is proposed.
- Existing Rust ownership remains unchanged in this cycle because the
  deliverable is analysis documentation, not runtime code.
- Any future adoption should keep prompt-cache-friendly context outside
  the stable system prompt whenever possible.

## Expected Output Structure

The Korean report will be organized into:

1. TizenClaw current-state assessment
2. OpenClaw findings
3. NanoClaw findings
4. Hermes Agent findings
5. Transferability assessment
6. Recommended adoption order for TizenClaw
