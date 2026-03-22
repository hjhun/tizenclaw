import socket
import json
import logging
from typing import Dict, Any

logger = logging.getLogger(__name__)

class ContainerEngine:
    """
    Python implementation of TizenClaw ContainerEngine.
    Communicates with the secure crun/exec environment via abstract domain sockets.
    """
    TOOL_EXECUTOR_SOCKET = "\0tizenclaw-tool-executor.sock"

    def __init__(self):
        self.initialized = False

    async def initialize(self) -> bool:
        self.initialized = True
        logger.info("ContainerEngine initialized.")
        return True

    def _connect_tool_executor(self) -> socket.socket:
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.settimeout(30)
        sock.connect(self.TOOL_EXECUTOR_SOCKET)
        return sock

    def _execute_tool_command(self, req: Dict[str, Any], timeout_seconds: int = 30) -> str:
        try:
            sock = self._connect_tool_executor()
            sock.settimeout(timeout_seconds)
            sock.sendall(json.dumps(req).encode('utf-8') + b'\n')
            
            response_data = b""
            while True:
                chunk = sock.recv(4096)
                if not chunk:
                    break
                response_data += chunk
                if b'\n' in chunk:
                    break
                    
            sock.close()
            return response_data.decode('utf-8').strip()
        except Exception as e:
            logger.error(f"Failed to execute tool command: {e}")
            return json.dumps({"error": str(e)})

    async def execute_skill(self, skill_name: str, arg_str: str) -> str:
        req = {
            "command": "execute_skill",
            "skill_name": skill_name,
            "arguments": arg_str
        }
        return self._execute_tool_command(req)

    async def execute_cli_tool(self, tool_name: str, arguments: str, timeout_seconds: int) -> str:
        req = {
            "command": "execute_cli",
            "tool_name": tool_name,
            "arguments": arguments,
            "timeout": timeout_seconds
        }
        return self._execute_tool_command(req, timeout_seconds)
