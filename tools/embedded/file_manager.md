# file_manager

Manage files on the Tizen device. Create, read, delete files or list directory contents. Paths MUST start with /skills/ or /data/ — other paths are rejected. Use /skills/ to save new skill scripts, /data/ for persistent data.

**Category**: file_system

## Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| operation | string | yes | The file operation to perform (write_file, read_file, delete_file, list_dir) |
| path | string | yes | File or directory path. Must start with /skills/ or /data/ |
| content | string | no | File content (for write_file only) |

## Schema

```json
{
  "name": "file_manager",
  "description": "Manage files on the Tizen device. Create, read, delete files or list directory contents. Paths MUST start with /skills/ or /data/.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "operation": {
        "type": "string",
        "enum": ["write_file", "read_file", "delete_file", "list_dir"],
        "description": "The file operation to perform"
      },
      "path": {
        "type": "string",
        "description": "File or directory path. Must start with /skills/ or /data/"
      },
      "content": {
        "type": "string",
        "description": "File content (for write_file only)"
      }
    },
    "required": ["operation", "path"]
  }
}
```
