#!/usr/bin/env python3
"""
TizenClaw Skill: Web Search (v2)
Multi-engine web search with Naver (default) and Google support.
Daily usage rate limiting via file-based counter.
"""
import json
import os
import sys
import urllib.parse
import urllib.request
from datetime import date

SKILLS_DIR = os.path.dirname(os.path.abspath(__file__))
CONFIG_PATH = os.path.join(
    SKILLS_DIR, "web_search_config.json"
)
DEFAULT_USAGE_FILE = "/data/web_search_usage.json"
DEFAULT_DAILY_LIMIT = 100


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
    limit = config.get("daily_limit", DEFAULT_DAILY_LIMIT)
    usage_file = config.get(
        "usage_file", DEFAULT_USAGE_FILE
    )
    today = date.today().isoformat()

    # Read current usage
    usage = {"date": today, "count": 0}
    try:
        with open(usage_file, "r") as f:
            usage = json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        pass

    # Reset if new day
    if usage.get("date") != today:
        usage = {"date": today, "count": 0}

    if usage["count"] >= limit:
        return False, 0, (
            f"Daily search limit reached "
            f"({limit}/{limit}). "
            f"Try again tomorrow."
        )

    # Increment and save
    usage["count"] += 1
    remaining = limit - usage["count"]
    try:
        parent = os.path.dirname(usage_file)
        if parent:
            os.makedirs(parent, exist_ok=True)
        with open(usage_file, "w") as f:
            json.dump(usage, f)
    except Exception as e:
        # Non-fatal: still allow search
        pass

    return True, remaining, None


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
        "webkw.json?query="
        + urllib.parse.quote(query)
        + "&display=5"
    )
    req = urllib.request.Request(url)
    req.add_header(
        "X-Naver-Client-Id", client_id
    )
    req.add_header(
        "X-Naver-Client-Secret", client_secret
    )

    try:
        with urllib.request.urlopen(
            req, timeout=10
        ) as resp:
            data = json.loads(
                resp.read().decode("utf-8")
            )
    except Exception as e:
        return {"error": f"Naver API error: {e}"}

    items = data.get("items", [])
    results = []
    for item in items[:5]:
        title = (
            item.get("title", "")
            .replace("<b>", "")
            .replace("</b>", "")
        )
        desc = (
            item.get("description", "")
            .replace("<b>", "")
            .replace("</b>", "")
        )
        link = item.get("link", "")
        results.append({
            "title": title,
            "snippet": desc,
            "url": link,
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
    req = urllib.request.Request(
        url,
        headers={"User-Agent": "TizenClaw/2.0"},
    )

    try:
        with urllib.request.urlopen(
            req, timeout=10
        ) as resp:
            data = json.loads(
                resp.read().decode("utf-8")
            )
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


def web_search(query, engine=None):
    """Perform web search with rate limiting."""
    config = load_config()

    # Check daily usage limit
    allowed, remaining, err = (
        check_and_increment_usage(config)
    )
    if not allowed:
        return {"error": err}

    # Determine engine
    if not engine:
        engine = config.get(
            "default_engine", "naver"
        )

    # Execute search
    if engine == "google":
        result = search_google(query, config)
    else:
        result = search_naver(query, config)

    result["daily_remaining"] = remaining
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
