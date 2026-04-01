# Design 1: Safely Remove execute_code module coupling

## Objective
To safely omit `execute_code` from all testing blocks without breaking existing unit tests evaluating `ToolPolicy`, `AgentRole`, `PipelineExecutor`, and `WorkflowEngine`.

## Execution Strategy
- Since `execute_code` has been removed, the core Rust components that hardcode `execute_code` for unit tests will be dynamically switched to `dummy_tool` or `test_tool`.
- Any JSON config defining `allowed_tools` containing `execute_code` will be pruned to prevent the LLM backend from assuming `execute_code` availability.
- The `tool_declaration_builder` method will simply omit pushing the tool.

## Memory / Performance Implication
- Removes the schema definition from LLM inference input, saving context tokens.
- No memory overhead.
