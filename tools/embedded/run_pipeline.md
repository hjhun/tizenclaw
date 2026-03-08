# run_pipeline

Execute a pipeline by its ID. Optionally provide input variables that can be referenced in steps via {{variable}} syntax.

**Category**: pipeline

## Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| pipeline_id | string | yes | The pipeline ID to execute |
| input_vars | object | no | Input variables (key-value pairs) available to all pipeline steps |

## Schema

```json
{
  "name": "run_pipeline",
  "description": "Execute a pipeline by its ID. Optionally provide input variables that can be referenced in steps via {{variable}} syntax.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "pipeline_id": {
        "type": "string",
        "description": "The pipeline ID to execute"
      },
      "input_vars": {
        "type": "object",
        "description": "Input variables (key-value pairs) available to all pipeline steps"
      }
    },
    "required": ["pipeline_id"]
  }
}
```
