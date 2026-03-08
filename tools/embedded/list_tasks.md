# list_tasks

List all scheduled tasks. Optionally filter by session_id.

**Category**: task_scheduler

## Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| session_id | string | no | Optional session ID to filter |

## Schema

```json
{
  "name": "list_tasks",
  "description": "List all scheduled tasks. Optionally filter by session_id.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "session_id": {
        "type": "string",
        "description": "Optional session ID to filter"
      }
    },
    "required": []
  }
}
```
