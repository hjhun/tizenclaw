---
name: tizen-web-search-cli
description: "Multi-engine web search supporting Naver, Google, Brave, Gemini, Grok, Kimi, and Perplexity"
type: cli
command: "python3 /opt/usr/share/tizenclaw/tools/cli_py/tizen_web_search_cli.py"
---
# tizen-web-search-cli

**Description**: Multi-engine web search supporting Naver, Google, Brave, Gemini, Grok, Kimi, and Perplexity.

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

## Usage
```
tizen-web-search-cli --query "Tizen 10.0 release" --engine naver
tizen-web-search-cli --query "weather today"
```

## Output
```json
{
  "engine": "naver",
  "query": "Tizen 10.0",
  "results": [
    {"title": "Tizen 10.0 Release Notes", "snippet": "Overview...", "url": "https://example.com"}
  ]
}
```
