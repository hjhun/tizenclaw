# tizen-web-search-cli
**Description**: Multi-engine web search supporting Naver, Google, Brave, Gemini, Grok, Kimi, and Perplexity.

## Usage
```
tizen-web-search-cli --query <QUERY> [--engine <ENGINE>]
```

## Arguments
| Argument | Required | Description |
|----------|----------|-------------|
| `--query` | Yes | Search query string |
| `--engine` | No | Search engine (default: from config) |

## Supported Engines
| Engine | Type | API Key Required |
|--------|------|-----------------|
| `naver` | Web search | client_id + client_secret |
| `google` | Custom Search | api_key + search_engine_id |
| `brave` | Web search | api_key |
| `gemini` | AI + Google grounding | api_key |
| `grok` | AI + web search | api_key |
| `kimi` | AI + web search | api_key |
| `perplexity` | AI search | api_key |

## Configuration
API keys are stored in `/opt/usr/share/tizenclaw/config/web_search_config.json`.

## Example Output
```json
{
  "engine": "naver",
  "query": "Tizen 10.0",
  "results": [
    {
      "title": "Tizen 10.0 Release Notes",
      "snippet": "Overview of new features...",
      "url": "https://example.com/article"
    }
  ]
}
```

## LLM Agent Instructions
**CRITICAL**: You MUST pass the search string using the `--query` flag. Do not pass the search string as a positional argument. 
Example: `--query "Tizen new features"`
If you want to specify an engine, use `--engine naver` etc.
