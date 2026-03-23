"""
TizenClaw Web Dashboard HTTP Server.

Provides:
  - Static file serving for the dashboard UI (data/web/)
  - REST API endpoints (/api/*) for dashboard features
  - Bridge API for TizenClaw SDK (callTool, chat, events)

Uses only Python stdlib (http.server + asyncio) — zero dependencies.
"""
import asyncio
import json
import logging
import os
import time
import hashlib
import secrets
import mimetypes
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse, parse_qs
from typing import Dict, Any, Optional

logger = logging.getLogger(__name__)

# Constants
WEB_DIR = "/opt/usr/share/tizenclaw/web"
CONFIG_DIR = "/opt/usr/share/tizenclaw/config"
WORK_DIR = "/opt/usr/share/tizenclaw/work"
SESSIONS_DIR = os.path.join(WORK_DIR, "sessions")
LOGS_DIR = os.path.join(WORK_DIR, "audit")
DEFAULT_PORT = 8080
ADMIN_DEFAULT_PW = "tizenclaw"


class DashboardAPI:
    """Shared state and logic for the REST API."""

    def __init__(self, agent_core=None):
        self.agent = agent_core
        self.start_time = time.time()
        self._admin_pw_hash = hashlib.sha256(
            ADMIN_DEFAULT_PW.encode()
        ).hexdigest()
        self._tokens: Dict[str, float] = {}

        # Metrics counters
        self.counters = {
            "llm_calls": 0,
            "tool_calls": 0,
            "errors": 0,
        }

    def _auth_check(self, token: str) -> bool:
        if token in self._tokens:
            if time.time() - self._tokens[token] < 3600:
                return True
            del self._tokens[token]
        return False

    def login(self, password: str) -> Optional[str]:
        pw_hash = hashlib.sha256(password.encode()).hexdigest()
        if pw_hash == self._admin_pw_hash:
            token = secrets.token_hex(32)
            self._tokens[token] = time.time()
            return token
        return None

    def change_password(self, current: str, new_pw: str) -> bool:
        cur_hash = hashlib.sha256(current.encode()).hexdigest()
        if cur_hash == self._admin_pw_hash:
            self._admin_pw_hash = hashlib.sha256(new_pw.encode()).hexdigest()
            return True
        return False

    def get_metrics(self) -> dict:
        # Use HealthMonitor if available, fallback to local counters
        try:
            from tizenclaw.core.health_monitor import get_health_monitor
            hm = get_health_monitor()
            return hm.get_metrics_dict()
        except Exception:
            pass

        # Fallback: manual metrics
        uptime_sec = int(time.time() - self.start_time)
        hours = uptime_sec // 3600
        minutes = (uptime_sec % 3600) // 60
        secs = uptime_sec % 60
        return {
            "status": "running",
            "uptime": {
                "seconds": uptime_sec,
                "formatted": f"{hours}h {minutes}m {secs}s"
            },
            "counters": self.counters,
            "pid": os.getpid(),
        }

    def get_sessions(self) -> list:
        sessions = []
        os.makedirs(SESSIONS_DIR, exist_ok=True)
        try:
            for f in os.listdir(SESSIONS_DIR):
                path = os.path.join(SESSIONS_DIR, f)
                if os.path.isfile(path):
                    stat = os.stat(path)
                    date_str = time.strftime("%Y-%m-%d", time.localtime(stat.st_mtime))
                    sessions.append({
                        "id": f,
                        "date": date_str,
                        "size_bytes": stat.st_size,
                        "modified": int(stat.st_mtime)
                    })
        except Exception:
            pass
        return sorted(sessions, key=lambda x: x.get("modified", 0), reverse=True)

    def get_session_content(self, sid: str) -> Optional[str]:
        path = os.path.join(SESSIONS_DIR, sid)
        if os.path.isfile(path):
            with open(path, "r", encoding="utf-8", errors="replace") as f:
                return f.read()
        return None

    def get_tasks(self) -> list:
        tasks = []
        tasks_dir = os.path.join(WORK_DIR, "tasks")
        os.makedirs(tasks_dir, exist_ok=True)
        try:
            for f in os.listdir(tasks_dir):
                path = os.path.join(tasks_dir, f)
                if os.path.isfile(path):
                    stat = os.stat(path)
                    date_str = time.strftime("%Y-%m-%d", time.localtime(stat.st_mtime))
                    preview = ""
                    try:
                        with open(path, "r", encoding="utf-8") as fh:
                            preview = fh.read(200)
                    except Exception:
                        pass
                    tasks.append({
                        "file": f,
                        "date": date_str,
                        "modified": int(stat.st_mtime),
                        "content_preview": preview[:100]
                    })
        except Exception:
            pass
        return sorted(tasks, key=lambda x: x.get("modified", 0), reverse=True)

    def get_config_list(self) -> list:
        known = [
            "llm_config.json", "agent_roles.json", "telegram_config.json",
            "web_search_config.json", "tool_policy.json"
        ]
        configs = []
        for name in known:
            path = os.path.join(CONFIG_DIR, name)
            configs.append({"name": name, "exists": os.path.isfile(path)})
        return configs


class DashboardHandler(BaseHTTPRequestHandler):
    """HTTP request handler for TizenClaw dashboard."""

    api: DashboardAPI = None
    agent_core = None
    loop: asyncio.AbstractEventLoop = None

    def log_message(self, format, *args):
        logger.debug(f"HTTP: {args[0] if args else ''}")

    def do_GET(self):
        parsed = urlparse(self.path)
        path = parsed.path
        query = parse_qs(parsed.query)

        if path.startswith("/api/"):
            self._handle_api_get(path[5:], query)
        else:
            self._serve_static(path)

    def do_POST(self):
        parsed = urlparse(self.path)
        path = parsed.path

        content_length = int(self.headers.get("Content-Length", 0))
        body = self.rfile.read(content_length) if content_length > 0 else b""

        try:
            data = json.loads(body) if body else {}
        except json.JSONDecodeError:
            data = {}

        if path.startswith("/api/"):
            self._handle_api_post(path[5:], data)
        else:
            self._send_json({"error": "Not found"}, 404)

    def _handle_api_get(self, endpoint: str, query: dict):
        api = self.__class__.api

        if endpoint == "metrics":
            self._send_json(api.get_metrics())
        elif endpoint == "sessions":
            self._send_json(api.get_sessions())
        elif endpoint.startswith("sessions/"):
            sid = endpoint[9:]
            content = api.get_session_content(sid)
            if content is not None:
                self._send_json({"content": content})
            else:
                self._send_json({"error": "Not found"}, 404)
        elif endpoint == "tasks":
            self._send_json(api.get_tasks())
        elif endpoint.startswith("tasks/"):
            fname = endpoint[6:]
            path = os.path.join(WORK_DIR, "tasks", fname)
            if os.path.isfile(path):
                with open(path, "r", encoding="utf-8") as f:
                    self._send_json({"content": f.read()})
            else:
                self._send_json({"error": "Not found"}, 404)
        elif endpoint == "logs" or endpoint == "logs/dates":
            self._handle_logs_get(endpoint, query)
        elif endpoint.startswith("config/"):
            self._handle_config_get(endpoint[7:])
        elif endpoint == "bridge/tools":
            if self.__class__.agent_core and self.__class__.agent_core.indexer:
                schemas = self.__class__.agent_core.indexer.get_tool_schemas()
                self._send_json({"tools": schemas})
            else:
                self._send_json({"tools": []})
        elif endpoint == "perception":
            # Return perception engine status + last insight
            agent = self.__class__.agent_core
            if agent and hasattr(agent, 'event_bus') and agent.event_bus:
                try:
                    from tizenclaw.core.perception_engine import PerceptionEngine
                    # Try to get from daemon's perception engine reference
                    self._send_json({"status": "running", "insight": {}})
                except Exception:
                    self._send_json({"status": "unavailable"})
            else:
                self._send_json({"status": "unavailable"})
        elif endpoint == "eventbus":
            agent = self.__class__.agent_core
            if agent and hasattr(agent, 'event_bus') and agent.event_bus:
                limit = int(query.get("limit", [50])[0])
                topic = query.get("topic", [None])[0]
                history = agent.event_bus.get_history(topic=topic, limit=limit)
                self._send_json({"events": history})
            else:
                self._send_json({"events": []})
        else:
            self._send_json({"error": "Unknown endpoint"}, 404)

    def _handle_api_post(self, endpoint: str, data: dict):
        api = self.__class__.api

        if endpoint == "chat":
            self._handle_chat(data)
        elif endpoint == "auth/login":
            token = api.login(data.get("password", ""))
            if token:
                self._send_json({"status": "ok", "token": token})
            else:
                self._send_json({"error": "Invalid password"}, 401)
        elif endpoint == "auth/change_password":
            auth_header = self.headers.get("Authorization", "")
            token = auth_header.replace("Bearer ", "")
            if not api._auth_check(token):
                self._send_json({"error": "Unauthorized"}, 401)
                return
            ok = api.change_password(
                data.get("current_password", ""),
                data.get("new_password", "")
            )
            if ok:
                self._send_json({"status": "ok"})
            else:
                self._send_json({"error": "Current password incorrect"}, 400)
        elif endpoint.startswith("config/"):
            self._handle_config_post(endpoint[7:], data)
        elif endpoint == "bridge/tool":
            self._handle_bridge_tool(data)
        elif endpoint == "bridge/chat":
            self._handle_chat(data)
        elif endpoint == "ota/check":
            try:
                from tizenclaw.core.ota_updater import OtaUpdater
                updater = OtaUpdater()
                updater.load_config()
                result = updater.check_for_updates()
                self._send_json(json.loads(result))
            except Exception as e:
                self._send_json({"error": str(e)})
        elif endpoint == "ota/update":
            skill = data.get("skill", "")
            if not skill:
                self._send_json({"error": "No skill name specified"}, 400)
                return
            try:
                from tizenclaw.core.ota_updater import OtaUpdater
                updater = OtaUpdater()
                updater.load_config()
                result = updater.update_skill(skill)
                self._send_json(json.loads(result))
            except Exception as e:
                self._send_json({"error": str(e)})
        elif endpoint == "ota/rollback":
            skill = data.get("skill", "")
            if not skill:
                self._send_json({"error": "No skill name specified"}, 400)
                return
            try:
                from tizenclaw.core.ota_updater import OtaUpdater
                updater = OtaUpdater()
                result = updater.rollback_skill(skill)
                self._send_json(json.loads(result))
            except Exception as e:
                self._send_json({"error": str(e)})
        else:
            self._send_json({"error": "Unknown endpoint"}, 404)

    def _handle_chat(self, data: dict):
        prompt = data.get("prompt", data.get("text", ""))
        session_id = data.get("session_id", "web_dashboard")
        agent = self.__class__.agent_core

        if not agent or not prompt:
            self._send_json({"response": "Error: Agent not available or empty prompt"})
            return

        # Run async agent call from sync handler
        loop = self.__class__.loop
        future = asyncio.run_coroutine_threadsafe(
            agent.process_prompt(session_id, prompt),
            loop
        )
        try:
            result = future.result(timeout=120)
            self.__class__.api.counters["llm_calls"] += 1
            self._send_json({"response": result})
        except Exception as e:
            self.__class__.api.counters["errors"] += 1
            logger.error(f"Chat error: {e}")
            self._send_json({"response": f"Error: {e}"})

    def _handle_bridge_tool(self, data: dict):
        tool_name = data.get("tool_name", "")
        args = data.get("arguments", {})
        agent = self.__class__.agent_core

        if not agent or not tool_name:
            self._send_json({"status": "error", "error": "No tool specified"}, 400)
            return

        loop = self.__class__.loop
        future = asyncio.run_coroutine_threadsafe(
            agent.dispatcher.execute_tool(tool_name, args),
            loop
        )
        try:
            result = future.result(timeout=30)
            self.__class__.api.counters["tool_calls"] += 1
            try:
                parsed = json.loads(result)
                self._send_json({"status": "ok", "result": parsed})
            except (json.JSONDecodeError, TypeError):
                self._send_json({"status": "ok", "result": result})
        except Exception as e:
            self.__class__.api.counters["errors"] += 1
            self._send_json({"status": "error", "error": str(e)})

    def _handle_logs_get(self, endpoint: str, query: dict):
        os.makedirs(LOGS_DIR, exist_ok=True)
        if endpoint == "logs/dates":
            dates = set()
            try:
                for f in os.listdir(LOGS_DIR):
                    if f.endswith(".log"):
                        dates.add(f.replace(".log", ""))
            except Exception:
                pass
            self._send_json({"dates": sorted(dates, reverse=True)})
        else:
            date = query.get("date", [None])[0]
            logs = []
            try:
                for f in sorted(os.listdir(LOGS_DIR), reverse=True):
                    if f.endswith(".log"):
                        if date and not f.startswith(date):
                            continue
                        path = os.path.join(LOGS_DIR, f)
                        with open(path, "r", encoding="utf-8", errors="replace") as fh:
                            logs.append({"file": f, "content": fh.read()})
                        if len(logs) >= 10:
                            break
            except Exception:
                pass
            self._send_json(logs)

    def _handle_config_get(self, name: str):
        if name == "list":
            api = self.__class__.api
            self._send_json({"configs": api.get_config_list()})
            return

        path = os.path.join(CONFIG_DIR, name)
        if os.path.isfile(path):
            with open(path, "r", encoding="utf-8") as f:
                self._send_json({"status": "ok", "content": f.read()})
        else:
            # Check data/devel for sample
            sample_path = os.path.join("/opt/usr/share/tizenclaw/data/devel", name)
            if os.path.isfile(sample_path):
                with open(sample_path, "r", encoding="utf-8") as f:
                    self._send_json({"status": "not_found", "sample": f.read()})
            else:
                self._send_json({"error": "Config not found"}, 404)

    def _handle_config_post(self, name: str, data: dict):
        auth_header = self.headers.get("Authorization", "")
        token = auth_header.replace("Bearer ", "")
        api = self.__class__.api
        if not api._auth_check(token):
            self._send_json({"error": "Unauthorized"}, 401)
            return

        content = data.get("content", "")
        os.makedirs(CONFIG_DIR, exist_ok=True)
        path = os.path.join(CONFIG_DIR, name)
        try:
            with open(path, "w", encoding="utf-8") as f:
                f.write(content)
            self._send_json({"status": "ok"})
        except Exception as e:
            self._send_json({"error": str(e)}, 500)

    def _serve_static(self, path: str):
        if path == "/" or path == "":
            path = "/index.html"

        file_path = os.path.join(WEB_DIR, path.lstrip("/"))

        if not os.path.isfile(file_path):
            # Try with index.html for SPA
            index_path = os.path.join(file_path, "index.html")
            if os.path.isfile(index_path):
                file_path = index_path
            else:
                self.send_error(404, "Not Found")
                return

        # Security: prevent path traversal
        real_web = os.path.realpath(WEB_DIR)
        real_file = os.path.realpath(file_path)
        if not real_file.startswith(real_web):
            self.send_error(403, "Forbidden")
            return

        # Determine content type
        mime_type, _ = mimetypes.guess_type(file_path)
        if mime_type is None:
            mime_type = "application/octet-stream"

        try:
            with open(file_path, "rb") as f:
                content = f.read()
            self.send_response(200)
            self.send_header("Content-Type", mime_type)
            self.send_header("Content-Length", str(len(content)))
            self.send_header("Cache-Control", "no-cache")
            self.end_headers()
            self.wfile.write(content)
        except Exception:
            self.send_error(500, "Internal Server Error")

    def _send_json(self, data: Any, status: int = 200):
        body = json.dumps(data, ensure_ascii=False).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.end_headers()
        self.wfile.write(body)


def start_dashboard_server(agent_core, loop: asyncio.AbstractEventLoop, port: int = DEFAULT_PORT):
    """Start the dashboard HTTP server in a background thread."""
    import threading

    DashboardHandler.api = DashboardAPI(agent_core)
    DashboardHandler.agent_core = agent_core
    DashboardHandler.loop = loop

    server = HTTPServer(("0.0.0.0", port), DashboardHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    logger.info(f"Dashboard HTTP server started on port {port}")
    return server
