# create_pipeline

Create a multi-step pipeline for deterministic workflow execution. Each step can be a tool call, LLM prompt, or conditional branch. Steps execute sequentially, and output from each step is available to subsequent steps via {{variable}} interpolation.

**Category**: pipeline

## Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| name | string | yes | Pipeline name |
| description | string | no | Pipeline description |
| trigger | string | no | Trigger type: 'manual' or 'cron:daily HH:MM' etc. |
| steps | array | yes | Array of step objects (each with id, type, and optional tool_name, args, prompt, condition, then_step, else_step, output_var, skip_on_failure, max_retries) |

## Step Object

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| id | string | yes | Step identifier |
| type | string | yes | Step type: tool, prompt, or condition |
| tool_name | string | no | Tool to invoke (for type=tool) |
| args | object | no | Tool arguments (for type=tool) |
| prompt | string | no | LLM prompt text (for type=prompt) |
| condition | string | no | Condition expression (for type=condition) |
| then_step | string | no | Step ID if condition is true |
| else_step | string | no | Step ID if condition is false |
| output_var | string | no | Variable name for step output |
| skip_on_failure | boolean | no | Continue on error |
| max_retries | integer | no | Max retry count |

## Schema

```json
{
  "name": "create_pipeline",
  "description": "Create a multi-step pipeline for deterministic workflow execution with {{variable}} interpolation.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "name": {
        "type": "string",
        "description": "Pipeline name"
      },
      "description": {
        "type": "string",
        "description": "Pipeline description"
      },
      "trigger": {
        "type": "string",
        "description": "Trigger type: 'manual' or 'cron:daily HH:MM' etc."
      },
      "steps": {
        "type": "array",
        "description": "Array of step objects",
        "items": {
          "type": "object",
          "properties": {
            "id": { "type": "string", "description": "Step identifier" },
            "type": { "type": "string", "description": "Step type: tool, prompt, or condition" },
            "tool_name": { "type": "string", "description": "Tool to invoke" },
            "args": { "type": "object", "description": "Tool arguments" },
            "prompt": { "type": "string", "description": "LLM prompt text" },
            "condition": { "type": "string", "description": "Condition expression" },
            "then_step": { "type": "string", "description": "Step ID if true" },
            "else_step": { "type": "string", "description": "Step ID if false" },
            "output_var": { "type": "string", "description": "Variable name for step output" },
            "skip_on_failure": { "type": "boolean", "description": "Continue on error" },
            "max_retries": { "type": "integer", "description": "Max retry count" }
          },
          "required": ["id", "type"]
        }
      }
    },
    "required": ["name", "steps"]
  }
}
```
