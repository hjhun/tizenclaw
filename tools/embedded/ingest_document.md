# ingest_document

Ingest a document into the knowledge base for semantic search. The text is split into chunks, embedded, and stored in the local vector database.

**Category**: rag

## Parameters

| Name | Type | Required | Description |
|------|------|----------|-------------|
| source | string | yes | Source identifier (filename, URL, or label) |
| text | string | yes | The document text to ingest |

## Schema

```json
{
  "name": "ingest_document",
  "description": "Ingest a document into the knowledge base for semantic search. The text is split into chunks, embedded, and stored in the local vector database.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "source": {
        "type": "string",
        "description": "Source identifier (filename, URL, or label)"
      },
      "text": {
        "type": "string",
        "description": "The document text to ingest"
      }
    },
    "required": ["source", "text"]
  }
}
```
