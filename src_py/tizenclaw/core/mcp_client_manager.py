"""
TizenClaw MCP Client Manager — connects to external MCP servers and imports tools.

Matches C++ McpClientManager:
  - Load MCP server configs from mcp_config.json
  - Launch MCP server processes (stdio mode)
  - Discover tools via tools/list
  - Bridge tool calls from AgentCore to MCP servers
  - Lifecycle management (start/stop)
"""
import asyncio
import json
import logging
import os
import subprocess
from typing import Dict, List, Any, Optional

logger = logging.getLogger(__name__)

MCP_CONFIG_PATH = "/opt/usr/share/tizenclaw/config/mcp_config.json"


class McpServerConnection:
    """A connection to a single MCP server process."""

    def __init__(self, name: str, command: List[str], env: Dict[str, str] = None):
        self.name = name
        self.command = command
        self.env = env or {}
        self.tools: List[Dict[str, Any]] = []
        self._process: Optional[asyncio.subprocess.Process] = None
        self._request_id = 0

    async def start(self) -> bool:
        """Launch MCP server process in stdio mode."""
        try:
            env = {**os.environ, **self.env}
            self._process = await asyncio.create_subprocess_exec(
                *self.command,
                stdin=asyncio.subprocess.PIPE,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                env=env,
            )
            logger.info(f"MCP[{self.name}]: Process started (PID {self._process.pid})")

            # Initialize MCP protocol
            ok = await self._initialize()
            if not ok:
                await self.stop()
                return False

            # Discover tools
            self.tools = await self._list_tools()
            logger.info(f"MCP[{self.name}]: Discovered {len(self.tools)} tools")
            return True
        except Exception as e:
            logger.error(f"MCP[{self.name}]: Start failed: {e}")
            return False

    async def stop(self):
        if self._process and self._process.returncode is None:
            try:
                self._process.terminate()
                await asyncio.wait_for(self._process.wait(), timeout=5)
            except (asyncio.TimeoutError, ProcessLookupError):
                self._process.kill()
            logger.info(f"MCP[{self.name}]: Stopped")

    def is_running(self) -> bool:
        return self._process is not None and self._process.returncode is None

    async def _send_jsonrpc(self, method: str, params: Dict = None) -> Dict:
        """Send JSON-RPC request over stdio and read response."""
        if not self._process or not self._process.stdin or not self._process.stdout:
            return {"error": "Process not running"}

        self._request_id += 1
        request = {
            "jsonrpc": "2.0",
            "id": self._request_id,
            "method": method,
        }
        if params:
            request["params"] = params

        line = json.dumps(request) + "\n"
        self._process.stdin.write(line.encode("utf-8"))
        await self._process.stdin.drain()

        try:
            resp_line = await asyncio.wait_for(
                self._process.stdout.readline(), timeout=30
            )
            if resp_line:
                return json.loads(resp_line.decode("utf-8"))
        except (asyncio.TimeoutError, json.JSONDecodeError) as e:
            logger.error(f"MCP[{self.name}]: Response error: {e}")

        return {"error": "No response"}

    async def _initialize(self) -> bool:
        """Send MCP initialize handshake."""
        resp = await self._send_jsonrpc("initialize", {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "TizenClaw", "version": "1.0.0"},
        })
        if "error" in resp and isinstance(resp["error"], dict):
            logger.error(f"MCP[{self.name}]: Init error: {resp['error']}")
            return False

        # Send initialized notification
        notif = {"jsonrpc": "2.0", "method": "notifications/initialized"}
        self._process.stdin.write((json.dumps(notif) + "\n").encode("utf-8"))
        await self._process.stdin.drain()

        return True

    async def _list_tools(self) -> List[Dict[str, Any]]:
        resp = await self._send_jsonrpc("tools/list")
        result = resp.get("result", {})
        return result.get("tools", [])

    async def call_tool(self, tool_name: str, arguments: Dict) -> str:
        """Call a tool on this MCP server."""
        resp = await self._send_jsonrpc("tools/call", {
            "name": tool_name,
            "arguments": arguments,
        })
        result = resp.get("result", {})
        if "content" in result:
            parts = []
            for c in result["content"]:
                if c.get("type") == "text":
                    parts.append(c.get("text", ""))
            return "\n".join(parts) if parts else json.dumps(result)
        if "error" in resp:
            return json.dumps(resp["error"])
        return json.dumps(result)


class McpClientManager:
    """Manages multiple MCP server connections."""

    def __init__(self):
        self._servers: Dict[str, McpServerConnection] = {}
        self._enabled = False

    def load_config(self, path: str = MCP_CONFIG_PATH) -> bool:
        if not os.path.isfile(path):
            logger.info("McpClientManager: Config not found")
            return False
        try:
            with open(path, "r", encoding="utf-8") as f:
                cfg = json.load(f)
            self._enabled = cfg.get("enabled", False)
            for name, server_cfg in cfg.get("servers", {}).items():
                command = server_cfg.get("command", [])
                if isinstance(command, str):
                    command = command.split()
                env = server_cfg.get("env", {})
                self._servers[name] = McpServerConnection(name, command, env)
            logger.info(f"McpClientManager: Loaded {len(self._servers)} server configs")
            return True
        except Exception as e:
            logger.error(f"McpClientManager: Config error: {e}")
            return False

    async def start_all(self) -> int:
        """Start all configured MCP servers. Returns number of successes."""
        if not self._enabled:
            return 0
        success = 0
        for name, server in self._servers.items():
            if await server.start():
                success += 1
        return success

    async def stop_all(self):
        for server in self._servers.values():
            await server.stop()

    def get_all_tools(self) -> List[Dict[str, Any]]:
        """Get all tools from all connected MCP servers."""
        tools = []
        for name, server in self._servers.items():
            for tool in server.tools:
                tool_copy = dict(tool)
                tool_copy["_mcp_server"] = name
                tools.append(tool_copy)
        return tools

    async def call_tool(self, tool_name: str, arguments: Dict) -> Optional[str]:
        """Route a tool call to the appropriate MCP server."""
        for server in self._servers.values():
            for tool in server.tools:
                if tool.get("name") == tool_name:
                    return await server.call_tool(tool_name, arguments)
        return None

    def get_server_status(self) -> List[Dict[str, Any]]:
        return [
            {
                "name": name,
                "running": server.is_running(),
                "tools_count": len(server.tools),
                "command": " ".join(server.command[:3]),
            }
            for name, server in self._servers.items()
        ]

    def get_status(self) -> Dict[str, Any]:
        return {
            "enabled": self._enabled,
            "servers": self.get_server_status(),
            "total_tools": len(self.get_all_tools()),
        }
