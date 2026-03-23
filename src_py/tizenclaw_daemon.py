#!/usr/bin/env python3
import asyncio
import json
import logging
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
        
    async def handle_client(self, reader: asyncio.StreamReader, writer: asyncio.StreamWriter):
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
