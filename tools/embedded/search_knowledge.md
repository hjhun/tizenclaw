# search_knowledge

Search the knowledge base using semantic similarity. Returns the most relevant document chunks.

**Category**: rag

## Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| query | string | yes | The search query |
| top_k | integer | no | Number of results (default 5) |

## Schema

```json
{
  "name": "search_knowledge",
  "description": "Search the knowledge base using semantic similarity. Returns the most relevant document chunks.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "query": {
        "type": "string",
        "description": "The search query"
      },
      "top_k": {
        "type": "integer",
        "description": "Number of results (default 5)"
      }
    },
    "required": ["query"]
  }
}
```
