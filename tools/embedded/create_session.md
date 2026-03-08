# create_session

Create a new agent session with a custom system prompt. The new session operates independently with its own conversation history. Use this to delegate specialized tasks to a purpose-built agent.

**Category**: multi_agent

## Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| name | string | yes | Short name for the session (used as session_id prefix) |
| system_prompt | string | yes | Custom system prompt that defines the agent's role and behavior |

## Schema

```json
{
  "name": "create_session",
  "description": "Create a new agent session with a custom system prompt. The new session operates independently with its own conversation history.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "name": {
        "type": "string",
        "description": "Short name for the session (used as session_id prefix)"
      },
      "system_prompt": {
        "type": "string",
        "description": "Custom system prompt that defines the agent's role and behavior"
      }
    },
    "required": ["name", "system_prompt"]
  }
}
```
