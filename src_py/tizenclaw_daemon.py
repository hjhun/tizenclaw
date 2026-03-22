#!/usr/bin/env python3
import asyncio
import json
import logging
import struct
import sys
from typing import Dict, Any

from tizenclaw.core.agent_core import AgentCore

# Configure logging
logging.basicConfig(level=logging.INFO, format='%(asctime)s [%(levelname)s] %(message)s')
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

        if method == "prompt":
            session_id = params.get("session_id", "default")
            prompt_text = params.get("text", "")
            
            # Simple synchronous call to agent for now
            result = await self.agent.process_prompt(session_id, prompt_text)
            return {"jsonrpc": "2.0", "id": req_id, "result": {"text": result}}
        
        elif method == "list_agents":
            return {"jsonrpc": "2.0", "id": req_id, "result": [{"name": "PythonAgent_Core"}]}
            
        else:
            return {"jsonrpc": "2.0", "id": req_id, "error": {"code": -32601, "message": "Method not found"}}

    async def run(self):
        await self.agent.initialize()
        
        server = await asyncio.start_unix_server(
            self.handle_client,
            path=self.SOCKET_PATH
        )
        logger.info(f"IPC Server listening on abstract namespace socket: {self.SOCKET_PATH}")
        
        async with server:
            await server.serve_forever()

if __name__ == "__main__":
    daemon = TizenClawDaemon()
    try:
        asyncio.run(daemon.run())
    except KeyboardInterrupt:
        logger.info("Daemon gracefully correctly manually.")
