# delete_pipeline

Delete a pipeline by its ID.

**Category**: pipeline

## Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| pipeline_id | string | yes | The pipeline ID to delete |

## Schema

```json
{
  "name": "delete_pipeline",
  "description": "Delete a pipeline by its ID.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "pipeline_id": {
        "type": "string",
        "description": "The pipeline ID to delete"
      }
    },
    "required": ["pipeline_id"]
  }
}
```
