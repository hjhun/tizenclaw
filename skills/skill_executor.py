#!/usr/bin/env python3
"""
TizenClaw Skill Executor — UDS IPC server for secure container.

Listens on a Unix Domain Socket and executes skill scripts,
returning their stdout as JSON responses. Runs as PID 1 inside
the secure OCI container.

Protocol: Length-prefixed JSON (4-byte big-endian + UTF-8 JSON)
Request:  {"skill": "<name>", "args": "<json-string>"}
Response: {"status": "ok"|"error", "output": "<string>"}
"""
import json
import os
import signal
import socket
import struct
import subprocess
import sys
import threading

SOCKET_PATH = "/tmp/tizenclaw_skill.sock"
SKILLS_DIR = "/skills"
PYTHON_BIN = "/usr/bin/python3"
MAX_PAYLOAD = 10 * 1024 * 1024  # 10 MB
EXEC_TIMEOUT = 30  # seconds


def log(msg):
    """Simple stderr logger (visible via dlog bind-mount)."""
    print(f"[SkillExecutor] {msg}", file=sys.stderr, flush=True)


def recv_exact(sock, n):
    """Read exactly n bytes from socket."""
    buf = b""
    while len(buf) < n:
        chunk = sock.recv(n - len(buf))
        if not chunk:
            return None
        buf += chunk
    return buf


def send_response(sock, resp_dict):
    """Send a length-prefixed JSON response."""
    payload = json.dumps(resp_dict).encode("utf-8")
    header = struct.pack("!I", len(payload))
    try:
        sock.sendall(header + payload)
    except BrokenPipeError:
        pass


def execute_skill(skill_name, args_str):
    """Run a skill script and capture its output."""
    script = os.path.join(
        SKILLS_DIR, skill_name, f"{skill_name}.py"
    )
    if not os.path.isfile(script):
        return {
            "status": "error",
            "output": f"Skill not found: {script}",
        }

    env = os.environ.copy()
    env["CLAW_ARGS"] = args_str

    try:
        result = subprocess.run(
            [PYTHON_BIN, script],
            capture_output=True,
            text=True,
            timeout=EXEC_TIMEOUT,
            env=env,
        )
    except subprocess.TimeoutExpired:
        return {
            "status": "error",
            "output": f"Skill timed out after {EXEC_TIMEOUT}s",
        }
    except Exception as e:
        return {
            "status": "error",
            "output": f"Failed to run skill: {e}",
        }

    if result.returncode != 0:
        detail = (result.stderr or result.stdout or "")[:500]
        return {
            "status": "error",
            "output": (
                f"exit {result.returncode}: {detail}"
            ),
        }

    # Extract last non-empty line as JSON result
    output = result.stdout.rstrip()
    lines = output.split("\n")
    for line in reversed(lines):
        stripped = line.strip()
        if stripped and (
            stripped.startswith("{") or stripped.startswith("[")
        ):
            return {"status": "ok", "output": stripped}

    # Fallback: return raw stdout
    return {"status": "ok", "output": output}


def handle_client(conn):
    """Handle a single client connection."""
    try:
        while True:
            # Read 4-byte header
            header = recv_exact(conn, 4)
            if header is None:
                break

            length = struct.unpack("!I", header)[0]
            if length > MAX_PAYLOAD:
                log(f"Payload too large: {length}")
                send_response(conn, {
                    "status": "error",
                    "output": "Payload too large",
                })
                break

            raw = recv_exact(conn, length)
            if raw is None:
                break

            try:
                req = json.loads(raw.decode("utf-8"))
            except (json.JSONDecodeError, UnicodeDecodeError) as e:
                send_response(conn, {
                    "status": "error",
                    "output": f"Bad JSON: {e}",
                })
                continue

            skill = req.get("skill", "")
            args = req.get("args", "{}")
            log(f"Exec skill={skill}")

            resp = execute_skill(skill, args)
            send_response(conn, resp)

    except Exception as e:
        log(f"Client error: {e}")
    finally:
        conn.close()


def main():
    log("Starting...")

    # Clean up stale socket
    try:
        os.unlink(SOCKET_PATH)
    except FileNotFoundError:
        pass

    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(SOCKET_PATH)
    os.chmod(SOCKET_PATH, 0o666)
    srv.listen(5)
    log(f"Listening on {SOCKET_PATH}")

    # Graceful shutdown
    running = True

    def on_signal(signum, _):
        nonlocal running
        log(f"Signal {signum}, shutting down")
        running = False
        srv.close()

    signal.signal(signal.SIGTERM, on_signal)
    signal.signal(signal.SIGINT, on_signal)

    while running:
        try:
            conn, _ = srv.accept()
        except OSError:
            break
        t = threading.Thread(
            target=handle_client, args=(conn,), daemon=True
        )
        t.start()

    log("Stopped.")


if __name__ == "__main__":
    main()
