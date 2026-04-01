# Planning 1: Remove execute_code core logic

## Objective
Remove the deprecated `execute_code` capability from the TizenClaw core module completely, as the underlying Python/Bash Sandbox has been removed in the previous `48aad293` task.

## Target Changes
1. `src/core/tool_declaration_builder.rs`: Remove `execute_code` from the LLM tool schemas list.
2. `src/core/agent_role.rs` & tests: Remove `execute_code` dependency from allowed tools arrays.
3. `src/core/pipeline_executor.rs` & tests: Migrate `execute_code` dependencies to a generic test tool.
4. `src/core/tool_policy.rs` & tests: Change `execute_code` references to a dummy test tool.
5. `src/core/workflow_engine.rs` & tests: Update test references from `execute_code` to another base tool.
6. JSON configs & docs: Purge any mention of `execute_code`.

## Execution Mode
- Rust Native Modification (Daemon Core code cleanup)
- Built-in embedded definitions removal.
