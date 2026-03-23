import socket
import struct
import json
import logging
from typing import Dict, Any

logger = logging.getLogger(__name__)

class ContainerEngine:
    """
    Python implementation of TizenClaw ContainerEngine.
    Communicates with the tool-executor via abstract Unix domain sockets
    using a 4-byte network-endian length-prefix protocol.
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
        """Send a request to the tool-executor and return the response.

        Uses 4-byte big-endian length prefix, matching the tool-executor's
        protocol: [4-byte length][JSON payload]
        """
        try:
            sock = self._connect_tool_executor()
            sock.settimeout(timeout_seconds + 5)  # extra margin

            payload = json.dumps(req).encode('utf-8')
            sock.sendall(struct.pack("!I", len(payload)) + payload)

            # Read 4-byte length prefix response
            len_buf = b""
            while len(len_buf) < 4:
                chunk = sock.recv(4 - len(len_buf))
                if not chunk:
                    break
                len_buf += chunk

            if len(len_buf) < 4:
                sock.close()
                return json.dumps({"error": "No response from tool executor"})

            resp_len = struct.unpack("!I", len_buf)[0]
            response_data = b""
            while len(response_data) < resp_len:
                chunk = sock.recv(min(4096, resp_len - len(response_data)))
                if not chunk:
                    break
                response_data += chunk

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

    async def execute_cli_tool(self, command: str, arguments: str, timeout_seconds: int = 30) -> str:
        """Execute a CLI tool via the tool-executor socket.

        Args:
            command: Full path to the native CLI binary, or a shell command
                     string (e.g. 'python3 /path/to/tool.py').
            arguments: Command-line arguments as a single string.
            timeout_seconds: Maximum execution time.
        """
        req = {
            "command": command,
            "arguments": arguments,
            "timeout": timeout_seconds
        }
        return self._execute_tool_command(req, timeout_seconds)
