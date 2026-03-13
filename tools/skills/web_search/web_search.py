#!/usr/bin/env python3
"""
TizenClaw Skill: Web Search (v3)
Multi-engine web search with support for:
  naver (default), google, brave, gemini, grok, kimi, perplexity.
Daily usage rate limiting via file-based counter.
"""
import json
import os
import ssl
import sys
import urllib.parse
import urllib.request
from datetime import date

SKILLS_DIR = os.path.dirname(os.path.abspath(__file__))
CONFIG_PATH = os.path.join(
    os.environ.get(
        "TIZENCLAW_CONFIG_DIR",
        "/opt/usr/share/tizenclaw/config"
    ),
    "web_search_config.json"
)
DEFAULT_USAGE_FILE = "/data/web_search_usage.json"
DEFAULT_DAILY_LIMIT = 100
DEFAULT_TIMEOUT = 15

BRAVE_ENDPOINT = (
    "https://api.search.brave.com"
    "/res/v1/web/search"
)
GEMINI_API_BASE = (
    "https://generativelanguage.googleapis.com"
    "/v1beta"
)
DEFAULT_GEMINI_MODEL = "gemini-2.5-flash"
XAI_ENDPOINT = "https://api.x.ai/v1/responses"
DEFAULT_GROK_MODEL = "grok-4-1-fast"
DEFAULT_KIMI_BASE_URL = (
    "https://api.moonshot.ai/v1"
)
DEFAULT_KIMI_MODEL = "moonshot-v1-128k"
KIMI_MAX_ROUNDS = 3
DEFAULT_PERPLEXITY_BASE_URL = (
    "https://api.perplexity.ai"
)
DEFAULT_PERPLEXITY_MODEL = "sonar-pro"


def _create_ssl_context():
    """Create SSL context, falling back to
    unverified if CA certs are unavailable
    (e.g. inside secure container)."""
    # Known CA bundle paths on Tizen / Linux
    ca_paths = [
        "/etc/ssl/certs/ca-certificates.crt",
        "/etc/pki/tls/certs/ca-bundle.crt",
        "/usr/share/ca-certificates/certs",
        "/etc/ssl/certs",
    ]

    # Try cafile first
    for p in ca_paths:
        if os.path.isfile(p):
            try:
                return ssl.create_default_context(
                    cafile=p
                )
            except Exception:
                continue

    # Try capath (directory with hashed certs)
    for p in ca_paths:
        if os.path.isdir(p):
            entries = os.listdir(p)
            if entries:
                try:
                    return ssl.create_default_context(
                        capath=p
                    )
                except Exception:
                    continue

    # Try certifi if installed
    try:
        import certifi
        return ssl.create_default_context(
            cafile=certifi.where()
        )
    except (ImportError, Exception):
        pass

    # Fallback: unverified context
    ctx = ssl.create_default_context()
    ctx.check_hostname = False
    ctx.verify_mode = ssl.CERT_NONE
    return ctx


SSL_CTX = _create_ssl_context()

SUPPORTED_ENGINES = [
    "naver", "google", "brave",
    "gemini", "grok", "kimi", "perplexity",
]


def load_config():
    """Load web search configuration."""
    try:
        with open(CONFIG_PATH, "r") as f:
            return json.load(f)
    except Exception:
        return {}


def check_and_increment_usage(config):
    """Check daily usage limit and increment counter.
    Returns (allowed, remaining, error_msg).
    """
    limit = config.get(
        "daily_limit", DEFAULT_DAILY_LIMIT
    )
    usage_file = config.get(
        "usage_file", DEFAULT_USAGE_FILE
    )
    today = date.today().isoformat()

    usage = {"date": today, "count": 0}
    try:
        with open(usage_file, "r") as f:
            usage = json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        pass

    if usage.get("date") != today:
        usage = {"date": today, "count": 0}

    if usage["count"] >= limit:
        return False, 0, (
            f"Daily search limit reached "
            f"({limit}/{limit}). "
            f"Try again tomorrow."
        )

    usage["count"] += 1
    remaining = limit - usage["count"]
    try:
        parent = os.path.dirname(usage_file)
        if parent:
            os.makedirs(parent, exist_ok=True)
        with open(usage_file, "w") as f:
            json.dump(usage, f)
    except Exception:
        pass

    return True, remaining, None


def _api_request(url, headers=None, data=None,
                 timeout=DEFAULT_TIMEOUT):
    """Send HTTP request and return parsed JSON."""
    if data is not None:
        body = json.dumps(data).encode("utf-8")
        req = urllib.request.Request(
            url, data=body, method="POST"
        )
        req.add_header(
            "Content-Type", "application/json"
        )
    else:
        req = urllib.request.Request(url)

    req.add_header("User-Agent", "TizenClaw/3.0")
    if headers:
        for k, v in headers.items():
            req.add_header(k, v)

    with urllib.request.urlopen(
        req, timeout=timeout, context=SSL_CTX
    ) as resp:
        return json.loads(
            resp.read().decode("utf-8")
        )


def _strip_html(text):
    """Remove simple HTML bold tags."""
    return (
        text.replace("<b>", "")
        .replace("</b>", "")
    )


def search_naver(query, config):
    """Search using Naver Search API."""
    naver_cfg = config.get("naver", {})
    client_id = naver_cfg.get("client_id", "")
    client_secret = naver_cfg.get(
        "client_secret", ""
    )

    if not client_id or not client_secret:
        return {
            "error": "Naver API credentials not "
            "configured in web_search_config.json"
        }

    url = (
        "https://openapi.naver.com/v1/search/"
        "webkr.json?query="
        + urllib.parse.quote(query)
        + "&display=5"
    )

    try:
        data = _api_request(url, headers={
            "X-Naver-Client-Id": client_id,
            "X-Naver-Client-Secret": client_secret,
        })
    except Exception as e:
        return {"error": f"Naver API error: {e}"}

    items = data.get("items", [])
    results = []
    for item in items[:5]:
        results.append({
            "title": _strip_html(
                item.get("title", "")
            ),
            "snippet": _strip_html(
                item.get("description", "")
            ),
            "url": item.get("link", ""),
        })

    return {
        "engine": "naver",
        "query": query,
        "results": results,
    }


def search_google(query, config):
    """Search using Google Custom Search JSON API."""
    google_cfg = config.get("google", {})
    api_key = google_cfg.get("api_key", "")
    cx = google_cfg.get("search_engine_id", "")

    if not api_key or not cx:
        return {
            "error": "Google API credentials not "
            "configured in web_search_config.json"
        }

    url = (
        "https://www.googleapis.com/"
        "customsearch/v1?q="
        + urllib.parse.quote(query)
        + "&key=" + api_key
        + "&cx=" + cx
        + "&num=5"
    )

    try:
        data = _api_request(url)
    except Exception as e:
        return {"error": f"Google API error: {e}"}

    items = data.get("items", [])
    results = []
    for item in items[:5]:
        results.append({
            "title": item.get("title", ""),
            "snippet": item.get("snippet", ""),
            "url": item.get("link", ""),
        })

    return {
        "engine": "google",
        "query": query,
        "results": results,
    }


def search_brave(query, config):
    """Search using Brave Search API."""
    brave_cfg = config.get("brave", {})
    api_key = brave_cfg.get("api_key", "")

    if not api_key:
        return {
            "error": "Brave API key not "
            "configured in web_search_config.json"
        }

    url = (
        BRAVE_ENDPOINT
        + "?q=" + urllib.parse.quote(query)
        + "&count=5"
    )

    try:
        data = _api_request(url, headers={
            "Accept": "application/json",
            "X-Subscription-Token": api_key,
        })
    except Exception as e:
        return {"error": f"Brave API error: {e}"}

    web = data.get("web", {})
    items = web.get("results", [])
    results = []
    for item in items[:5]:
        results.append({
            "title": item.get("title", ""),
            "snippet": item.get(
                "description", ""
            ),
            "url": item.get("url", ""),
        })

    return {
        "engine": "brave",
        "query": query,
        "results": results,
    }


def search_gemini(query, config):
    """Search using Gemini with Google Search
    grounding."""
    gemini_cfg = config.get("gemini", {})
    api_key = gemini_cfg.get("api_key", "")
    model = gemini_cfg.get(
        "model", DEFAULT_GEMINI_MODEL
    )

    if not api_key:
        return {
            "error": "Gemini API key not "
            "configured in web_search_config.json"
        }

    url = (
        f"{GEMINI_API_BASE}/models/"
        f"{model}:generateContent"
        f"?key={api_key}"
    )
    body = {
        "contents": [{
            "parts": [{"text": query}],
        }],
        "tools": [{"google_search": {}}],
    }

    try:
        data = _api_request(url, data=body)
    except Exception as e:
        return {"error": f"Gemini API error: {e}"}

    if data.get("error"):
        err = data["error"]
        return {
            "error": (
                f"Gemini API error "
                f"({err.get('code', '?')}): "
                f"{err.get('message', 'unknown')}"
            )
        }

    candidate = (
        data.get("candidates", [{}])[0]
        if data.get("candidates") else {}
    )
    content = ""
    for part in (
        candidate.get("content", {})
        .get("parts", [])
    ):
        if part.get("text"):
            content += part["text"] + "\n"
    content = content.strip() or "No response"

    grounding = candidate.get(
        "groundingMetadata", {}
    )
    chunks = grounding.get(
        "groundingChunks", []
    )
    results = []
    for chunk in chunks[:5]:
        web = chunk.get("web", {})
        if web.get("uri"):
            results.append({
                "title": web.get("title", ""),
                "snippet": "",
                "url": web["uri"],
            })

    return {
        "engine": "gemini",
        "query": query,
        "content": content,
        "results": results,
    }


def search_grok(query, config):
    """Search using xAI Grok with web search."""
    grok_cfg = config.get("grok", {})
    api_key = grok_cfg.get("api_key", "")
    model = grok_cfg.get(
        "model", DEFAULT_GROK_MODEL
    )

    if not api_key:
        return {
            "error": "Grok (xAI) API key not "
            "configured in web_search_config.json"
        }

    body = {
        "model": model,
        "input": [{
            "role": "user",
            "content": query,
        }],
        "tools": [{"type": "web_search"}],
    }

    try:
        data = _api_request(
            XAI_ENDPOINT,
            headers={
                "Authorization": f"Bearer {api_key}",
            },
            data=body,
        )
    except Exception as e:
        return {"error": f"Grok API error: {e}"}

    # Extract content from Responses API format
    content = ""
    citations = []
    for output in data.get("output", []):
        if output.get("type") == "message":
            for block in output.get("content", []):
                if (block.get("type") ==
                        "output_text" and
                        block.get("text")):
                    content = block["text"]
                    for ann in block.get(
                        "annotations", []
                    ):
                        if (ann.get("type") ==
                                "url_citation" and
                                ann.get("url")):
                            citations.append(
                                ann["url"]
                            )
        elif (output.get("type") ==
              "output_text" and
              output.get("text")):
            content = output["text"]
            for ann in output.get(
                "annotations", []
            ):
                if (ann.get("type") ==
                        "url_citation" and
                        ann.get("url")):
                    citations.append(ann["url"])

    if not content:
        content = data.get(
            "output_text", "No response"
        )

    # Use top-level citations if available
    top_citations = data.get("citations", [])
    if top_citations:
        citations = top_citations

    # Deduplicate
    seen = set()
    unique_citations = []
    for url in citations:
        if url not in seen:
            seen.add(url)
            unique_citations.append(url)

    results = []
    for url in unique_citations[:5]:
        results.append({
            "title": "",
            "snippet": "",
            "url": url,
        })

    return {
        "engine": "grok",
        "query": query,
        "content": content,
        "results": results,
    }


def search_kimi(query, config):
    """Search using Kimi (Moonshot) with built-in
    web search tool."""
    kimi_cfg = config.get("kimi", {})
    api_key = kimi_cfg.get("api_key", "")
    base_url = kimi_cfg.get(
        "base_url", DEFAULT_KIMI_BASE_URL
    ).rstrip("/")
    model = kimi_cfg.get(
        "model", DEFAULT_KIMI_MODEL
    )

    if not api_key:
        return {
            "error": "Kimi API key not "
            "configured in web_search_config.json"
        }

    endpoint = f"{base_url}/chat/completions"
    headers = {
        "Authorization": f"Bearer {api_key}",
    }
    messages = [
        {"role": "user", "content": query},
    ]
    kimi_tool = {
        "type": "builtin_function",
        "function": {"name": "$web_search"},
    }
    all_citations = []

    for _ in range(KIMI_MAX_ROUNDS):
        body = {
            "model": model,
            "messages": messages,
            "tools": [kimi_tool],
        }

        try:
            data = _api_request(
                endpoint, headers=headers,
                data=body,
            )
        except Exception as e:
            return {
                "error": f"Kimi API error: {e}"
            }

        # Collect citations from search_results
        for sr in data.get(
            "search_results", []
        ):
            url = (sr.get("url") or "").strip()
            if url:
                all_citations.append(url)

        choice = (
            data.get("choices", [{}])[0]
            if data.get("choices") else {}
        )
        message = choice.get("message", {})
        text = (
            message.get("content", "").strip()
            or message.get(
                "reasoning_content", ""
            ).strip()
        )
        tool_calls = message.get(
            "tool_calls", []
        )
        finish = choice.get("finish_reason", "")

        if finish != "tool_calls" or not tool_calls:
            # Final response
            content = text or "No response"
            break

        # Feed tool results back
        messages.append({
            "role": "assistant",
            "content": message.get("content", ""),
            "tool_calls": tool_calls,
        })
        tool_content = json.dumps({
            "search_results": [
                {
                    "title": sr.get("title", ""),
                    "url": sr.get("url", ""),
                    "content": sr.get(
                        "content", ""
                    ),
                }
                for sr in data.get(
                    "search_results", []
                )
            ]
        })
        for tc in tool_calls:
            tc_id = (tc.get("id") or "").strip()
            if tc_id:
                messages.append({
                    "role": "tool",
                    "tool_call_id": tc_id,
                    "content": tool_content,
                })
    else:
        content = (
            "Search completed but no final "
            "answer was produced."
        )

    # Deduplicate citations
    seen = set()
    unique = []
    for url in all_citations:
        if url not in seen:
            seen.add(url)
            unique.append(url)

    results = []
    for url in unique[:5]:
        results.append({
            "title": "",
            "snippet": "",
            "url": url,
        })

    return {
        "engine": "kimi",
        "query": query,
        "content": content,
        "results": results,
    }


def search_perplexity(query, config):
    """Search using Perplexity AI."""
    pplx_cfg = config.get("perplexity", {})
    api_key = pplx_cfg.get("api_key", "")
    base_url = pplx_cfg.get(
        "base_url", DEFAULT_PERPLEXITY_BASE_URL
    ).rstrip("/")
    model = pplx_cfg.get(
        "model", DEFAULT_PERPLEXITY_MODEL
    )

    if not api_key:
        return {
            "error": "Perplexity API key not "
            "configured in web_search_config.json"
        }

    endpoint = f"{base_url}/chat/completions"
    body = {
        "model": model,
        "messages": [{
            "role": "user",
            "content": query,
        }],
    }

    try:
        data = _api_request(
            endpoint,
            headers={
                "Authorization": f"Bearer {api_key}",
            },
            data=body,
        )
    except Exception as e:
        return {
            "error": f"Perplexity API error: {e}"
        }

    content = "No response"
    citations = []
    choices = data.get("choices", [])
    if choices:
        msg = choices[0].get("message", {})
        content = (
            msg.get("content", "").strip()
            or "No response"
        )

        # Extract citations from annotations
        for ann in msg.get("annotations", []):
            if ann.get("type") == "url_citation":
                url = (
                    ann.get("url_citation", {})
                    .get("url")
                    or ann.get("url")
                )
                if url:
                    citations.append(url)

    # Also check top-level citations
    top = data.get("citations", [])
    if top:
        citations = top

    # Deduplicate
    seen = set()
    unique = []
    for url in citations:
        if isinstance(url, str) and url.strip():
            u = url.strip()
            if u not in seen:
                seen.add(u)
                unique.append(u)

    results = []
    for url in unique[:5]:
        results.append({
            "title": "",
            "snippet": "",
            "url": url,
        })

    return {
        "engine": "perplexity",
        "query": query,
        "content": content,
        "results": results,
    }


ENGINE_DISPATCH = {
    "naver": search_naver,
    "google": search_google,
    "brave": search_brave,
    "gemini": search_gemini,
    "grok": search_grok,
    "kimi": search_kimi,
    "perplexity": search_perplexity,
}


def web_search(query, engine=None):
    """Perform web search with rate limiting."""
    config = load_config()

    if not engine:
        engine = config.get(
            "default_engine", "naver"
        )

    if engine not in ENGINE_DISPATCH:
        return {
            "error": (
                f"Unknown engine: {engine}. "
                f"Supported: "
                f"{', '.join(SUPPORTED_ENGINES)}"
            )
        }

    # Apply daily usage limit for API-key engines
    if engine not in ("naver",):
        allowed, remaining, err = (
            check_and_increment_usage(config)
        )
        if not allowed:
            return {"error": err}
        result = ENGINE_DISPATCH[engine](
            query, config
        )
        result["daily_remaining"] = remaining
    else:
        result = ENGINE_DISPATCH[engine](
            query, config
        )

    return result


if __name__ == "__main__":
    claw_args = os.environ.get("CLAW_ARGS")
    if claw_args:
        try:
            parsed = json.loads(claw_args)
            query = parsed.get("query", "")
            engine = parsed.get("engine")
            if query:
                print(json.dumps(
                    web_search(query, engine)
                ))
                sys.exit(0)
        except Exception as e:
            print(json.dumps({
                "error": f"Parse error: {e}"
            }))
            sys.exit(1)

    if len(sys.argv) < 2:
        print(json.dumps({
            "error": "No query provided"
        }))
        sys.exit(1)

    query = " ".join(sys.argv[1:])
    print(json.dumps(web_search(query)))
