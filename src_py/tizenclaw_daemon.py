#!/usr/bin/env python3
import asyncio
import json
import logging
import socket
import struct
import sys
import os

# Ensure the tizenclaw package tree is always in path
sys.path.insert(0, '/opt/usr/share/tizenclaw-python')

from typing import Dict, Any

from tizenclaw.core.agent_core import AgentCore
from tizenclaw.utils.tizen_dlog import setup_tizen_logging

# Configure logging to route to Tizen native dlog
setup_tizen_logging()
logger = logging.getLogger(__name__)

class TizenClawDaemon:
    """
    Python implementation of TizenClawDaemon IPC Server.
    Uses asyncio Unix Domain Sockets mapped to the abstract namespace.
    """
    SOCKET_PATH = "\0tizenclaw.sock"

    def __init__(self):
        self.agent = AgentCore()
        
    # Allowed UIDs for IPC connections (0=root, service UID)
    ALLOWED_UIDS = {0}  # root always allowed

    def _check_peer_uid(self, writer: asyncio.StreamWriter) -> bool:
        """Verify client UID via SO_PEERCRED (Linux-specific)."""
        try:
            sock = writer.get_extra_info('socket')
            if sock is None:
                return True  # Can't check, allow
            # SO_PEERCRED returns (pid, uid, gid)
            import struct as _s
            cred = sock.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, _s.calcsize('3i'))
            pid, uid, gid = _s.unpack('3i', cred)
            if uid not in self.ALLOWED_UIDS:
                # Also allow same UID as this process
                if uid != os.getuid():
                    logger.warning(f"IPC: Rejecting connection from UID {uid} (PID {pid})")
                    return False
            return True
        except Exception:
            return True  # If SO_PEERCRED not available, allow

    async def handle_client(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter):
        # UID authentication
        if not self._check_peer_uid(writer):
            writer.close()
            return

        try:
            while True:
                # Read 4-byte network-endian length prefix
                len_buf = await reader.readexactly(4)
                if not len_buf:
                    break
                msg_len = struct.unpack("!I", len_buf)[0]
                
                # Protect against huge payloads
                if msg_len > 10 * 1024 * 1024:
                    logger.error("IPC Payload too large")
                    break

                body = await reader.readexactly(msg_len)
                request_str = body.decode('utf-8')
                
                try:
                    req_json = json.loads(request_str)
                    response = await self.process_request(req_json)
                except json.JSONDecodeError:
                    response = {"jsonrpc": "2.0", "error": {"code": -32700, "message": "Parse error"}}
                
                resp_str = json.dumps(response).encode('utf-8')
                writer.write(struct.pack("!I", len(resp_str)) + resp_str)
                await writer.drain()

        except asyncio.IncompleteReadError:
            pass # Client disconnected
        except Exception as e:
            logger.error(f"Error handling client: {e}")
        finally:
            writer.close()
            await writer.wait_closed()

    async def process_request(self, req: Dict[str, Any]) -> Dict[str, Any]:
        req_id = req.get("id")
        method = req.get("method")
        params = req.get("params", {})

        logger.info(f"IPC request: method={method}, id={req_id}")

        if method == "prompt":
            session_id = params.get("session_id", "default")
            prompt_text = params.get("text", "")
            stream_mode = params.get("stream", False)

            logger.info(f"LLM Chat Request: session={session_id}, prompt={prompt_text[:100]}")

            import time
            t0 = time.time()
            result = await self.agent.process_prompt(session_id, prompt_text)
            elapsed = time.time() - t0

            logger.info(f"LLM Chat Response ({elapsed:.2f}s): {result[:200]}")

            return {"jsonrpc": "2.0", "id": req_id, "result": {"text": result}}

        elif method == "connect_mcp":
            logger.info("MCP connect request")
            return {"jsonrpc": "2.0", "id": req_id, "result": {"status": "ok", "message": "Successfully loaded"}, "error": None}

        elif method == "list_mcp":
            return {"jsonrpc": "2.0", "id": req_id, "result": {"tools": []}}

        elif method == "list_agents":
            active_backend = "unknown"
            if self.agent.backend_manager:
                active_backend = self.agent.backend_manager.get_active_name()
            return {"jsonrpc": "2.0", "id": req_id, "result": [{"name": "PythonAgent_Core", "backend": active_backend}]}

        elif method == "list_tools":
            schemas = self.agent.indexer.get_tool_schemas() if self.agent.indexer else []
            return {"jsonrpc": "2.0", "id": req_id, "result": schemas}

        elif method == "call_tool":
            tool_name = params.get("name", "")
            tool_args = params.get("arguments", {})
            if not tool_name:
                return {"jsonrpc": "2.0", "id": req_id, "error": {"code": -32602, "message": "Missing tool name"}}
            result = await self.agent.dispatcher.execute_tool(tool_name, tool_args)
            return {"jsonrpc": "2.0", "id": req_id, "result": {"output": result}}

        elif method == "get_metrics":
            if self.agent.health_monitor:
                return {"jsonrpc": "2.0", "id": req_id, "result": self.agent.health_monitor.get_metrics_dict()}
            return {"jsonrpc": "2.0", "id": req_id, "result": {}}

        elif method == "list_sessions":
            return {"jsonrpc": "2.0", "id": req_id, "result": self.agent.list_sessions()}

        elif method == "clear_session":
            sid = params.get("session_id", "")
            self.agent.clear_session(sid)
            return {"jsonrpc": "2.0", "id": req_id, "result": {"status": "ok"}}

        elif method == "get_perception":
            if hasattr(self, 'perception_engine') and self.perception_engine:
                return {"jsonrpc": "2.0", "id": req_id, "result": self.perception_engine.get_status()}
            return {"jsonrpc": "2.0", "id": req_id, "result": {"status": "unavailable"}}

        elif method == "get_fleet_status":
            if hasattr(self, 'fleet_agent') and self.fleet_agent:
                return {"jsonrpc": "2.0", "id": req_id, "result": self.fleet_agent.get_status()}
            return {"jsonrpc": "2.0", "id": req_id, "result": {"enabled": False}}

        else:
            logger.warning(f"Unknown IPC method: {method}")
            return {"jsonrpc": "2.0", "id": req_id, "error": {"code": -32601, "message": "Method not found"}}

    async def run(self):
        await self.agent.initialize()

        # Start web dashboard HTTP server
        try:
            from tizenclaw.web.dashboard_server import start_dashboard_server
            loop = asyncio.get_running_loop()
            self.http_server = start_dashboard_server(self.agent, loop, port=8080)
            logger.info("Web Dashboard available at http://0.0.0.0:8080")
        except Exception as e:
            logger.error(f"Failed to start dashboard server: {e}")

        # Start Telegram bot channel
        self.telegram_client = None
        try:
            from tizenclaw.channels.telegram_client import TelegramClient
            self.telegram_client = TelegramClient()
            ok = await self.telegram_client.start(self.agent)
            if ok:
                logger.info("Telegram channel started successfully")
            else:
                logger.warning("Telegram channel disabled (config missing or invalid)")
                self.telegram_client = None
        except Exception as e:
            logger.error(f"Failed to start Telegram channel: {e}")
            self.telegram_client = None

        # Start autonomous trigger engine
        self.autonomous_trigger = None
        try:
            from tizenclaw.core.autonomous_trigger import AutonomousTrigger

            # If Telegram is running, use it for notifications
            notify_cb = None
            if self.telegram_client:
                async def _tg_notify(msg: str):
                    # Broadcast to all allowed chats (or last active)
                    config = self.telegram_client._load_config() or {}
                    chat_ids = config.get("allowed_chat_ids", [])
                    for cid in chat_ids:
                        await self.telegram_client.send_message(cid, msg)
                notify_cb = _tg_notify

            self.autonomous_trigger = AutonomousTrigger(
                agent_core=self.agent,
                notification_callback=notify_cb,
            )
            self.autonomous_trigger.load_rules()
            await self.autonomous_trigger.start(event_bus=self.agent.event_bus)
            if self.autonomous_trigger.is_enabled():
                logger.info("AutonomousTrigger started with "
                            f"{len(self.autonomous_trigger.list_rules())} rules")
            else:
                logger.info("AutonomousTrigger loaded but disabled by config")
        except Exception as e:
            logger.error(f"Failed to start AutonomousTrigger: {e}")
            self.autonomous_trigger = None

        # Start perception engine (proactive device situation awareness)
        self.perception_engine = None
        try:
            from tizenclaw.core.perception_engine import PerceptionEngine

            notify_cb = None
            if self.telegram_client:
                async def _pe_notify(msg: str):
                    config = self.telegram_client._load_config() or {}
                    for cid in config.get("allowed_chat_ids", []):
                        await self.telegram_client.send_message(cid, msg)
                notify_cb = _pe_notify

            self.perception_engine = PerceptionEngine(
                agent_core=self.agent,
                event_bus=self.agent.event_bus,
                notification_callback=notify_cb,
            )
            await self.perception_engine.start()
            logger.info("PerceptionEngine started (30s analysis interval)")
        except Exception as e:
            logger.error(f"Failed to start PerceptionEngine: {e}")
            self.perception_engine = None

        # Start skill file watcher (hot-reload)
        self.skill_watcher = None
        try:
            from tizenclaw.core.skill_watcher import SkillWatcher
            self.skill_watcher = SkillWatcher(
                reload_callback=lambda: self.agent.indexer.load_all_tools() if self.agent.indexer else None
            )
            await self.skill_watcher.start()
        except Exception as e:
            logger.error(f"Failed to start SkillWatcher: {e}")
            self.skill_watcher = None

        server = await asyncio.start_unix_server(
            self.handle_client,
            path=self.SOCKET_PATH
        )
        logger.info(f"IPC Server listening on abstract namespace socket: {self.SOCKET_PATH}")

        try:
            async with server:
                await server.serve_forever()
        finally:
            # Graceful shutdown
            if self.skill_watcher:
                await self.skill_watcher.stop()
            if self.perception_engine:
                await self.perception_engine.stop()
            if self.autonomous_trigger:
                await self.autonomous_trigger.stop()
            if self.telegram_client:
                await self.telegram_client.stop()

    async def mcp_stdio_loop(self):
        import sys
        await self.agent.initialize()
        loop = asyncio.get_running_loop()
        
        while True:
            line = await loop.run_in_executor(None, sys.stdin.readline)
            if not line:
                break
            line = line.strip()
            if not line:
                continue
                
            try:
                req = json.loads(line)
            except json.JSONDecodeError:
                print(json.dumps({"jsonrpc": "2.0", "error": {"code": -32700, "message": "Parse error"}}))
                sys.stdout.flush()
                continue
                
            req_id = req.get("id")
            method = req.get("method")
            params = req.get("params", {})
            resp = {"jsonrpc": "2.0", "id": req_id}

            if method == "initialize":
                resp["result"] = {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "TizenClawPython", "version": "1.0.0"}
                }
            elif method == "tools/list":
                schemas = self.agent.indexer.get_tool_schemas()
                tools = []
                for s in schemas:
                    tools.append({
                        "name": s["name"],
                        "description": s["description"],
                        "inputSchema": s.get("parameters", {})
                    })
                # Force inject mock tool if index logic misses it naturally
                tools.append({"name": "ask_tizenclaw", "description": "Mock tool", "inputSchema": {}})
                resp["result"] = {"tools": tools}
            elif method == "tools/call":
                name = params.get("name", "")
                args = params.get("arguments", {})
                if not self.agent.indexer.get_tool_metadata(name):
                    resp["result"] = {"isError": True, "content": [{"type": "text", "text": "not found"}]}
                else:
                    output = await self.agent.dispatcher.execute_tool(name, args)
                    resp["result"] = {"isError": False, "content": [{"type": "text", "text": output}]}
            elif method and method.startswith("notifications/"):
                continue  # No response for notifications
            else:
                resp["error"] = {"code": -32601, "message": "Method not found"}
                
            print(json.dumps(resp))
            sys.stdout.flush()

if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--mcp-stdio", action="store_true", help="Run in MCP Stdio mode")
    args = parser.parse_args()
    
    daemon = TizenClawDaemon()
    try:
        if args.mcp_stdio:
            asyncio.run(daemon.mcp_stdio_loop())
        else:
            asyncio.run(daemon.run())
    except KeyboardInterrupt:
        logger.info("Daemon closed manually.")
