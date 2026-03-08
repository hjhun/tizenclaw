# run_supervisor

Run a supervisor agent that decomposes a complex goal into sub-tasks and delegates them to specialized role agents. Each role agent has its own system prompt and tool restrictions. Results are aggregated into a single response. Requires agent_roles.json configuration.

**Category**: multi_agent

## Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| goal | string | yes | The high-level goal to decompose and delegate |
| strategy | string | no | Execution strategy: 'sequential' (default) or 'parallel' |

## Schema

```json
{
  "name": "run_supervisor",
  "description": "Run a supervisor agent that decomposes a complex goal into sub-tasks and delegates them to specialized role agents.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "goal": {
        "type": "string",
        "description": "The high-level goal to decompose and delegate"
      },
      "strategy": {
        "type": "string",
        "enum": ["sequential", "parallel"],
        "description": "Execution strategy: 'sequential' (default) or 'parallel'"
      }
    },
    "required": ["goal"]
  }
}
```
