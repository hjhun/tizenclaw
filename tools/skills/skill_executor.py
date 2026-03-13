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
import tempfile
import threading

SOCKET_PATH = "/tmp/tizenclaw_skill.sock"
SKILLS_DIR = "/skills"
PYTHON_BIN = "/usr/bin/python3"
NODE_BIN = "/usr/bin/node"
MAX_PAYLOAD = 10 * 1024 * 1024  # 10 MB
EXEC_TIMEOUT = 30  # seconds
CODE_EXEC_TIMEOUT = 15  # seconds for dynamic code


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


def extract_json_output(stdout_text):
    """Extract the last JSON-like line from stdout."""
    output = stdout_text.rstrip()
    lines = output.split("\n")
    for line in reversed(lines):
        stripped = line.strip()
        if stripped and (
            stripped.startswith("{") or stripped.startswith("[")
        ):
            return stripped
    return output


def detect_runtime(skill_name):
    """Read manifest.json to determine runtime and entry point."""
    manifest_path = os.path.join(
        SKILLS_DIR, skill_name, "manifest.json"
    )
    runtime = "python"
    entry_point = f"{skill_name}.py"

    if os.path.isfile(manifest_path):
        try:
            with open(manifest_path) as f:
                manifest = json.load(f)
            runtime = manifest.get("runtime", "python")
            entry_point = manifest.get("entry_point", None)
            if not entry_point:
                ext_map = {
                    "python": ".py",
                    "node": ".js",
                    "native": "",
                }
                entry_point = (
                    skill_name + ext_map.get(runtime, ".py")
                )
        except (json.JSONDecodeError, IOError) as e:
            log(f"Failed to read manifest for "
                f"{skill_name}: {e}")

    return runtime, entry_point


def execute_skill(skill_name, args_str):
    """Run a skill script and capture its output."""
    runtime, entry_point = detect_runtime(skill_name)

    script = os.path.join(
        SKILLS_DIR, skill_name, entry_point
    )
    if not os.path.exists(script):
        return {
            "status": "error",
            "output": f"Entry point not found: {script}",
        }

    env = os.environ.copy()
    env["CLAW_ARGS"] = args_str

    # Dispatch by runtime
    if runtime == "python":
        cmd = [PYTHON_BIN, script]
    elif runtime == "node":
        cmd = [NODE_BIN, script]
    elif runtime == "native":
        cmd = [script]
    else:
        return {
            "status": "error",
            "output": f"Unknown runtime: {runtime}",
        }

    log(f"Exec skill={skill_name} "
        f"runtime={runtime} cmd={cmd[0]}")

    try:
        result = subprocess.run(
            cmd,
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

    return {
        "status": "ok",
        "output": extract_json_output(result.stdout),
    }


def execute_dynamic_code(code_str, timeout=None):
    """Execute LLM-generated Python code in a temp file."""
    if timeout is None:
        timeout = CODE_EXEC_TIMEOUT

    log(f"execute_code: {len(code_str)} chars, "
        f"timeout={timeout}s")

    # Write code to a temp file
    tmp_fd, tmp_path = tempfile.mkstemp(
        suffix=".py", prefix="tizenclaw_dynamic_",
        dir="/tmp"
    )
    try:
        with os.fdopen(tmp_fd, "w") as f:
            f.write(code_str)

        env = os.environ.copy()
        result = subprocess.run(
            [PYTHON_BIN, tmp_path],
            capture_output=True,
            text=True,
            timeout=timeout,
            env=env,
        )
    except subprocess.TimeoutExpired:
        return {
            "status": "error",
            "output": f"Code timed out after {timeout}s",
        }
    except Exception as e:
        return {
            "status": "error",
            "output": f"Failed to run code: {e}",
        }
    finally:
        try:
            os.unlink(tmp_path)
        except OSError:
            pass

    if result.returncode != 0:
        detail = (result.stderr or result.stdout or "")[:500]
        return {
            "status": "error",
            "output": (
                f"exit {result.returncode}: {detail}"
            ),
        }

    return {
        "status": "ok",
        "output": extract_json_output(result.stdout),
    }


ALLOWED_PATHS = ["/skills", "/data"]
MAX_READ_SIZE = 1024 * 1024  # 1 MB


def validate_path(path):
    """Validate that path is under allowed directories."""
    real = os.path.realpath(path)
    for allowed in ALLOWED_PATHS:
        allowed_real = os.path.realpath(allowed)
        if real == allowed_real or real.startswith(
            allowed_real + "/"
        ):
            return real, None
    return None, (
        f"Path '{path}' is outside allowed directories: "
        f"{ALLOWED_PATHS}"
    )


def handle_file_manager(req):
    """Handle file management operations."""
    operation = req.get("operation", "")
    path = req.get("path", "")

    if not path:
        return {
            "status": "error",
            "output": "No path provided",
        }

    real_path, err = validate_path(path)
    if err:
        return {"status": "error", "output": err}

    log(f"file_manager: op={operation} path={real_path}")

    try:
        if operation == "write_file":
            content = req.get("content", "")
            parent = os.path.dirname(real_path)
            os.makedirs(parent, exist_ok=True)
            with open(real_path, "w") as f:
                f.write(content)
            return {
                "status": "ok",
                "output": json.dumps({
                    "result": "file_written",
                    "path": path,
                    "size": len(content),
                }),
            }

        elif operation == "read_file":
            if not os.path.isfile(real_path):
                return {
                    "status": "error",
                    "output": f"File not found: {path}",
                }
            size = os.path.getsize(real_path)
            if size > MAX_READ_SIZE:
                return {
                    "status": "error",
                    "output": (
                        f"File too large: {size} bytes "
                        f"(max {MAX_READ_SIZE})"
                    ),
                }
            with open(real_path, "r") as f:
                content = f.read()
            return {
                "status": "ok",
                "output": json.dumps({
                    "result": "file_read",
                    "path": path,
                    "content": content,
                    "size": len(content),
                }),
            }

        elif operation == "delete_file":
            if not os.path.exists(real_path):
                return {
                    "status": "error",
                    "output": f"Path not found: {path}",
                }
            if os.path.isdir(real_path):
                import shutil
                shutil.rmtree(real_path)
            else:
                os.unlink(real_path)
            return {
                "status": "ok",
                "output": json.dumps({
                    "result": "deleted",
                    "path": path,
                }),
            }

        elif operation == "list_dir":
            target = real_path
            if not os.path.isdir(target):
                return {
                    "status": "error",
                    "output": f"Not a directory: {path}",
                }
            entries = []
            for name in sorted(os.listdir(target)):
                full = os.path.join(target, name)
                entries.append({
                    "name": name,
                    "type": (
                        "dir" if os.path.isdir(full)
                        else "file"
                    ),
                    "size": (
                        os.path.getsize(full)
                        if os.path.isfile(full) else 0
                    ),
                })
            return {
                "status": "ok",
                "output": json.dumps({
                    "result": "listing",
                    "path": path,
                    "entries": entries,
                }),
            }

        else:
            return {
                "status": "error",
                "output": (
                    f"Unknown operation: {operation}. "
                    "Supported: write_file, read_file, "
                    "delete_file, list_dir"
                ),
            }

    except Exception as e:
        return {
            "status": "error",
            "output": f"file_manager error: {e}",
        }


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

            # Route by command type
            command = req.get("command", "")
            if command == "execute_code":
                code = req.get("code", "")
                timeout = req.get("timeout",
                                  CODE_EXEC_TIMEOUT)
                if not code:
                    send_response(conn, {
                        "status": "error",
                        "output": "No code provided",
                    })
                    continue
                log(f"execute_code request")
                resp = execute_dynamic_code(code, timeout)
            elif command == "file_manager":
                resp = handle_file_manager(req)
            else:
                # Legacy skill execution
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
