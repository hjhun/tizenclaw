#!/usr/bin/env python3
"""TizenClaw Tool Executor — socket-activated service.

Listens on an abstract Unix domain socket and executes CLI tool commands
dispatched by the daemon's ContainerEngine via JSON-over-length-prefix IPC.

Protocol (request JSON):
    command   : str — full command (may contain spaces, e.g. "python3 /path/to/cli.py")
    arguments : str — CLI arguments as a single string (appended to command)
    args      : list[str] — (legacy) argument list
    timeout   : int — optional execution timeout in seconds (default 30)
"""
import asyncio
import json
import logging
import shlex
import subprocess

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] TOOL_EXECUTOR: %(message)s",
)
logger = logging.getLogger(__name__)

SOCKET_PATH = "\0tizenclaw-tool-executor.sock"
DEFAULT_TIMEOUT = 30


def _build_argv(req: dict) -> list[str]:
    """Build the argv list from a request dict.

    Supports two styles:
      1. command="python3 /path/to/script.py", arguments="battery"
         → ["python3", "/path/to/script.py", "battery"]
      2. command="/path/to/binary", args=["battery"]
         → ["/path/to/binary", "battery"]
    """
    command = req.get("command", "")
    # Split compound command (e.g. "python3 /path/to/cli.py") into parts
    argv = shlex.split(command) if command else []

    # Append arguments (string form — from ContainerEngine)
    arguments = req.get("arguments", "")
    if arguments and isinstance(arguments, str):
        argv.extend(shlex.split(arguments))

    # Append args (list form — legacy callers)
    args_list = req.get("args", [])
    if args_list and isinstance(args_list, list):
        argv.extend(str(a) for a in args_list)

    return argv


async def handle_client(reader, writer):
    try:
        data = await reader.read(4)
        if not data or len(data) < 4:
            writer.close()
            return

        length = int.from_bytes(data, byteorder="big")
        payload = await reader.read(length)

        req = json.loads(payload.decode("utf-8"))
        argv = _build_argv(req)
        timeout = req.get("timeout", DEFAULT_TIMEOUT)

        if not argv:
            resp = json.dumps({"status": "error", "error": "Empty command"})
        else:
            logger.info(f"Executing: {' '.join(argv)} (timeout={timeout}s)")
            process = await asyncio.create_subprocess_exec(
                *argv,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
            try:
                stdout, stderr = await asyncio.wait_for(
                    process.communicate(), timeout=timeout
                )
            except asyncio.TimeoutError:
                process.kill()
                await process.wait()
                resp = json.dumps({
                    "status": "error",
                    "error": f"Command timed out after {timeout}s",
                    "exit_code": -1,
                })
            else:
                resp = json.dumps({
                    "status": "success" if process.returncode == 0 else "error",
                    "stdout": stdout.decode("utf-8", errors="ignore"),
                    "stderr": stderr.decode("utf-8", errors="ignore"),
                    "exit_code": process.returncode,
                })
    except Exception as e:
        logger.error(f"Execution error: {e}")
        resp = json.dumps({"status": "error", "error": str(e)})

    resp_bytes = resp.encode("utf-8")
    writer.write(len(resp_bytes).to_bytes(4, byteorder="big"))
    writer.write(resp_bytes)
    await writer.drain()
    writer.close()


async def main():
    """Start the tool executor server.

    Supports two modes:
      1. systemd socket activation (LISTEN_FDS >= 1): use fd 3
      2. Standalone: create the abstract socket directly
    """
    import os
    import socket as _socket

    listen_fds = int(os.environ.get("LISTEN_FDS", "0"))

    if listen_fds >= 1:
        # systemd socket activation — fd 3 is the pre-opened listening socket
        sock = _socket.fromfd(3, _socket.AF_UNIX, _socket.SOCK_STREAM)
        sock.setblocking(False)
        server = await asyncio.start_unix_server(handle_client, sock=sock)
        logger.info("Python Tool Executor using systemd socket activation (fd=3)")
    else:
        # Standalone — create our own abstract socket
        server = await asyncio.start_unix_server(handle_client, path=SOCKET_PATH)
        logger.info(f"Python Tool Executor listening on {SOCKET_PATH!r}")

    async with server:
        await server.serve_forever()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        logger.info("Tool executor stopped.")
