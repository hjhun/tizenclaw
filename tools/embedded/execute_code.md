# execute_code

Execute arbitrary Python code on the Tizen device. Use this when no existing skill/tool can accomplish the task. The code MUST print a JSON result to stdout as the last line. Available: ctypes for Tizen C-API, os, subprocess, json, sys. Libraries at /tizen_libs or system path.

**Category**: code_execution

## Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| code | string | yes | Python code to execute on the Tizen device |

## Schema

```json
{
  "name": "execute_code",
  "description": "Execute arbitrary Python code on the Tizen device. Use this when no existing skill/tool can accomplish the task. The code MUST print a JSON result to stdout as the last line. Available: ctypes for Tizen C-API, os, subprocess, json, sys. Libraries at /tizen_libs or system path.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "code": {
        "type": "string",
        "description": "Python code to execute on the Tizen device"
      }
    },
    "required": ["code"]
  }
}
```
