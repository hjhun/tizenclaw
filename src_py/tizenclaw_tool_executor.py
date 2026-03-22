#!/usr/bin/env python3
import asyncio
import json
import logging
import subprocess

logging.basicConfig(level=logging.INFO, format='%(asctime)s [%(levelname)s] TOOL_EXECUTOR: %(message)s')
logger = logging.getLogger(__name__)

SOCKET_PATH = "\0tizenclaw_tool_executor.sock"

async def handle_client(reader, writer):
    data = await reader.read(4)
    if not data or len(data) < 4:
        writer.close()
        return
        
    length = int.from_bytes(data, byteorder='big')
    payload = await reader.read(length)
    
    try:
        req = json.loads(payload.decode('utf-8'))
        command = req.get("command", "")
        args = req.get("args", [])
        
        logger.info(f"Executing: {command} {args}")
        # Actually execute tools in container
        process = await asyncio.create_subprocess_exec(
            command, *args,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE
        )
        stdout, stderr = await process.communicate()
        
        resp = json.dumps({
            "status": "success" if process.returncode == 0 else "error",
            "stdout": stdout.decode('utf-8', errors='ignore'),
            "stderr": stderr.decode('utf-8', errors='ignore'),
            "exit_code": process.returncode
        })
    except Exception as e:
        logger.error(f"Execution error: {e}")
        resp = json.dumps({"status": "error", "error": str(e)})

    resp_bytes = resp.encode('utf-8')
    writer.write(len(resp_bytes).to_bytes(4, byteorder='big'))
    writer.write(resp_bytes)
    await writer.drain()
    writer.close()

async def main():
    server = await asyncio.start_unix_server(handle_client, path=SOCKET_PATH)
    logger.info("Python Tool Executor Socket listening..")
    async with server:
        await server.serve_forever()

if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        logger.info("Tool executor stopped.")
