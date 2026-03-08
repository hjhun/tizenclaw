# create_task

Create a scheduled task that runs automatically. Supports: 'daily HH:MM' (every day), 'interval Ns/Nm/Nh' (repeating), 'once YYYY-MM-DD HH:MM' (one-shot), 'weekly DAY HH:MM' (every week). The prompt will be sent to the LLM at the scheduled time.

**Category**: task_scheduler

## Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| schedule | string | yes | Schedule expression, e.g. 'daily 09:00', 'interval 30m', 'once 2026-03-10 14:00', 'weekly mon 09:00' |
| prompt | string | yes | The prompt to execute at the scheduled time |

## Schema

```json
{
  "name": "create_task",
  "description": "Create a scheduled task that runs automatically. Supports: 'daily HH:MM', 'interval Ns/Nm/Nh', 'once YYYY-MM-DD HH:MM', 'weekly DAY HH:MM'.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "schedule": {
        "type": "string",
        "description": "Schedule expression, e.g. 'daily 09:00', 'interval 30m', 'once 2026-03-10 14:00', 'weekly mon 09:00'"
      },
      "prompt": {
        "type": "string",
        "description": "The prompt to execute at the scheduled time"
      }
    },
    "required": ["schedule", "prompt"]
  }
}
```
