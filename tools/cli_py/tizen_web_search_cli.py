#!/usr/bin/env python3
"""tizen-web-search-cli — Python port (urllib)"""
import json, sys, urllib.request, urllib.parse, ssl, os

CONFIG_PATHS = ["/opt/usr/share/tizenclaw/config/web_search_config.json", "/opt/usr/share/tizenclaw/data/devel/web_search_config.json"]

def _load_config():
    for p in CONFIG_PATHS:
        if os.path.isfile(p):
            with open(p) as f: return json.load(f)
    return {}

def _get_arg(key, default=""):
    for i in range(1, len(sys.argv)-1):
        if sys.argv[i] == key: return sys.argv[i+1]
    return default

def _ssl_ctx():
    ctx = ssl.create_default_context()
    for ca in ["/etc/ssl/ca-bundle.pem", "/etc/ssl/certs/ca-certificates.crt", "/usr/share/ca-certificates"]:
        if os.path.exists(ca):
            try:
                if os.path.isfile(ca): ctx.load_verify_locations(ca)
                else: ctx.load_verify_locations(capath=ca)
                break
            except: pass
    return ctx

def search(query, engine="google", num=5):
    config = _load_config()
    ctx = _ssl_ctx()

    if engine == "google" and config.get("google_api_key") and config.get("google_cx"):
        url = f"https://www.googleapis.com/customsearch/v1?q={urllib.parse.quote(query)}&key={config['google_api_key']}&cx={config['google_cx']}&num={num}"
        try:
            req = urllib.request.Request(url)
            resp = urllib.request.urlopen(req, context=ctx, timeout=15)
            data = json.loads(resp.read().decode())
            items = [{"title":it.get("title",""),"link":it.get("link",""),"snippet":it.get("snippet","")} for it in data.get("items",[])]
            return json.dumps({"status":"success","engine":"google","query":query,"results":items})
        except Exception as e:
            return json.dumps({"error":str(e),"engine":"google"})

    if config.get("brave_api_key"):
        url = f"https://api.search.brave.com/res/v1/web/search?q={urllib.parse.quote(query)}&count={num}"
        try:
            req = urllib.request.Request(url, headers={"X-Subscription-Token": config["brave_api_key"], "Accept": "application/json"})
            resp = urllib.request.urlopen(req, context=ctx, timeout=15)
            data = json.loads(resp.read().decode())
            items = [{"title":it.get("title",""),"url":it.get("url",""),"description":it.get("description","")} for it in data.get("web",{}).get("results",[])]
            return json.dumps({"status":"success","engine":"brave","query":query,"results":items})
        except Exception as e:
            return json.dumps({"error":str(e),"engine":"brave"})

    return json.dumps({"error":"no search API configured","suggestion":"set google_api_key+google_cx or brave_api_key in web_search_config.json"})

if __name__ == "__main__":
    q = _get_arg("--query") or (_get_arg("search") if len(sys.argv) >= 3 and sys.argv[1] == "search" else "")
    if not q and len(sys.argv) >= 3: q = sys.argv[2]
    if not q:
        print("Usage: tizen-web-search-cli search --query <QUERY> [--engine google|brave] [--num N]", file=sys.stderr); sys.exit(1)
    print(search(q, _get_arg("--engine", "google"), int(_get_arg("--num", "5"))))
