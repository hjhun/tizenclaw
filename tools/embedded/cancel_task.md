# cancel_task

Cancel a scheduled task by its ID.

**Category**: task_scheduler

## Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| task_id | string | yes | The task ID to cancel |

## Schema

```json
{
  "name": "cancel_task",
  "description": "Cancel a scheduled task by its ID.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "task_id": {
        "type": "string",
        "description": "The task ID to cancel"
      }
    },
    "required": ["task_id"]
  }
}
```
