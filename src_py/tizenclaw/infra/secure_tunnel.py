"""
TizenClaw Secure Tunnel — reverse SSH tunnel for remote device access.

Provides:
  - Reverse SSH tunnel (device → remote server)
  - Forward tunnel (remote server → device service)
  - Auto-reconnect with exponential backoff
  - SSH key-based authentication (no password)
  - Configurable via secure_tunnel.json
  - Health monitoring integration

Use case: a Tizen device behind NAT can establish a reverse tunnel
to a public server, allowing remote access to the dashboard or
IPC socket from anywhere.
"""
import asyncio
import json
import logging
import os
import signal
import time
from typing import Dict, Any, Optional, List

logger = logging.getLogger(__name__)

TUNNEL_CONFIG_PATH = "/opt/usr/share/tizenclaw/config/secure_tunnel.json"
SSH_KEY_PATH = "/opt/usr/share/tizenclaw/work/.ssh/id_ed25519"


class TunnelSpec:
    """A single tunnel specification."""
    def __init__(self, name: str, mode: str,
                 local_port: int, remote_port: int,
                 remote_host: str = "localhost"):
        self.name = name
        self.mode = mode  # "reverse" (-R) or "forward" (-L)
        self.local_port = local_port
        self.remote_port = remote_port
        self.remote_host = remote_host

    def to_ssh_arg(self) -> str:
        if self.mode == "reverse":
            return f"-R {self.remote_port}:{self.remote_host}:{self.local_port}"
        else:
            return f"-L {self.local_port}:{self.remote_host}:{self.remote_port}"

    def to_dict(self) -> Dict[str, Any]:
        return {
            "name": self.name,
            "mode": self.mode,
            "local_port": self.local_port,
            "remote_port": self.remote_port,
            "remote_host": self.remote_host,
        }


class SecureTunnel:
    """
    Manages SSH tunnels between the Tizen device and a remote server.

    Configuration (secure_tunnel.json):
    {
        "enabled": true,
        "ssh_server": "tunnel.example.com",
        "ssh_port": 22,
        "ssh_user": "tizenclaw",
        "ssh_key_path": "/opt/usr/share/tizenclaw/work/.ssh/id_ed25519",
        "server_alive_interval": 30,
        "server_alive_count_max": 3,
        "reconnect_delay_initial": 5,
        "reconnect_delay_max": 300,
        "tunnels": [
            {
                "name": "dashboard",
                "mode": "reverse",
                "local_port": 8080,
                "remote_port": 18080
            },
            {
                "name": "ipc",
                "mode": "reverse",
                "local_port": 9090,
                "remote_port": 19090
            }
        ]
    }
    """

    def __init__(self):
        self._enabled = False
        self._ssh_server = ""
        self._ssh_port = 22
        self._ssh_user = ""
        self._ssh_key_path = SSH_KEY_PATH
        self._server_alive_interval = 30
        self._server_alive_count_max = 3
        self._reconnect_delay_initial = 5
        self._reconnect_delay_max = 300
        self._tunnels: List[TunnelSpec] = []
        self._process: Optional[asyncio.subprocess.Process] = None
        self._running = False
        self._task: Optional[asyncio.Task] = None
        self._connected = False
        self._connect_count = 0
        self._last_connect_time: float = 0
        self._last_disconnect_time: float = 0
        self._total_uptime: float = 0

    # ── Configuration ──

    def load_config(self, path: str = TUNNEL_CONFIG_PATH) -> bool:
        if not os.path.isfile(path):
            logger.info("SecureTunnel: Config not found, disabled")
            return False

        try:
            with open(path, "r", encoding="utf-8") as f:
                cfg = json.load(f)
        except Exception as e:
            logger.error(f"SecureTunnel: Config error: {e}")
            return False

        self._enabled = cfg.get("enabled", False)
        self._ssh_server = cfg.get("ssh_server", "")
        self._ssh_port = cfg.get("ssh_port", 22)
        self._ssh_user = cfg.get("ssh_user", "")
        self._ssh_key_path = cfg.get("ssh_key_path", SSH_KEY_PATH)
        self._server_alive_interval = cfg.get("server_alive_interval", 30)
        self._server_alive_count_max = cfg.get("server_alive_count_max", 3)
        self._reconnect_delay_initial = cfg.get("reconnect_delay_initial", 5)
        self._reconnect_delay_max = cfg.get("reconnect_delay_max", 300)

        self._tunnels = []
        for t in cfg.get("tunnels", []):
            self._tunnels.append(TunnelSpec(
                name=t.get("name", ""),
                mode=t.get("mode", "reverse"),
                local_port=t.get("local_port", 0),
                remote_port=t.get("remote_port", 0),
                remote_host=t.get("remote_host", "localhost"),
            ))

        if self._enabled:
            logger.info(f"SecureTunnel: Config loaded — {self._ssh_user}@{self._ssh_server}:"
                        f"{self._ssh_port}, {len(self._tunnels)} tunnel(s)")
        return True

    def is_enabled(self) -> bool:
        return self._enabled

    # ── SSH Key Management ──

    def generate_key_pair(self) -> bool:
        """Generate ED25519 key pair if none exists."""
        if os.path.isfile(self._ssh_key_path):
            return True

        key_dir = os.path.dirname(self._ssh_key_path)
        os.makedirs(key_dir, mode=0o700, exist_ok=True)

        try:
            import subprocess
            result = subprocess.run(
                ["ssh-keygen", "-t", "ed25519", "-f", self._ssh_key_path,
                 "-N", "", "-C", "tizenclaw@device"],
                capture_output=True, text=True, timeout=10
            )
            if result.returncode == 0:
                os.chmod(self._ssh_key_path, 0o600)
                logger.info(f"SecureTunnel: Generated key pair at {self._ssh_key_path}")
                return True
            else:
                logger.error(f"SecureTunnel: ssh-keygen failed: {result.stderr}")
        except Exception as e:
            logger.error(f"SecureTunnel: Key generation error: {e}")
        return False

    def get_public_key(self) -> str:
        """Read public key for authorized_keys setup."""
        pub_path = f"{self._ssh_key_path}.pub"
        if os.path.isfile(pub_path):
            with open(pub_path, "r") as f:
                return f.read().strip()
        return ""

    # ── Tunnel Lifecycle ──

    def _build_ssh_command(self) -> List[str]:
        """Build the SSH command with all tunnel specs."""
        cmd = [
            "ssh",
            "-N",  # No remote command
            "-o", "StrictHostKeyChecking=no",
            "-o", "UserKnownHostsFile=/dev/null",
            "-o", f"ServerAliveInterval={self._server_alive_interval}",
            "-o", f"ServerAliveCountMax={self._server_alive_count_max}",
            "-o", "ExitOnForwardFailure=yes",
            "-o", "BatchMode=yes",  # No password prompt
            "-p", str(self._ssh_port),
            "-i", self._ssh_key_path,
        ]

        # Add tunnel specifications
        for tunnel in self._tunnels:
            arg = tunnel.to_ssh_arg()
            cmd.extend(arg.split())

        cmd.append(f"{self._ssh_user}@{self._ssh_server}")
        return cmd

    async def start(self):
        """Start the tunnel with auto-reconnect."""
        if not self._enabled:
            logger.info("SecureTunnel: Disabled, not starting")
            return

        if not self._ssh_server or not self._ssh_user:
            logger.error("SecureTunnel: Missing ssh_server or ssh_user")
            return

        if not os.path.isfile(self._ssh_key_path):
            logger.info("SecureTunnel: No SSH key, generating...")
            if not self.generate_key_pair():
                logger.error("SecureTunnel: Cannot start without SSH key")
                return

        self._running = True
        self._task = asyncio.create_task(self._reconnect_loop())
        logger.info("SecureTunnel: Started (auto-reconnect enabled)")

    async def stop(self):
        """Stop the tunnel and SSH process."""
        self._running = False

        if self._process and self._process.returncode is None:
            try:
                self._process.terminate()
                await asyncio.wait_for(self._process.wait(), timeout=5)
            except (asyncio.TimeoutError, ProcessLookupError):
                try:
                    self._process.kill()
                except Exception:
                    pass

        if self._task:
            self._task.cancel()
            try:
                await self._task
            except asyncio.CancelledError:
                pass

        if self._connected:
            now = time.time()
            self._total_uptime += now - self._last_connect_time
            self._connected = False
            self._last_disconnect_time = now

        logger.info("SecureTunnel: Stopped")

    async def _reconnect_loop(self):
        """Main reconnection loop with exponential backoff."""
        delay = self._reconnect_delay_initial

        while self._running:
            try:
                cmd = self._build_ssh_command()
                logger.info(f"SecureTunnel: Connecting to {self._ssh_user}@"
                            f"{self._ssh_server}:{self._ssh_port}")

                self._process = await asyncio.create_subprocess_exec(
                    *cmd,
                    stdout=asyncio.subprocess.PIPE,
                    stderr=asyncio.subprocess.PIPE,
                )

                self._connected = True
                self._connect_count += 1
                self._last_connect_time = time.time()
                delay = self._reconnect_delay_initial  # Reset backoff

                logger.info(f"SecureTunnel: Connected (PID {self._process.pid})")

                # Wait for SSH process to exit
                returncode = await self._process.wait()

                # Disconnected
                now = time.time()
                session_time = now - self._last_connect_time
                self._total_uptime += session_time
                self._connected = False
                self._last_disconnect_time = now

                if self._running:
                    stderr = ""
                    try:
                        stderr_data = await asyncio.wait_for(
                            self._process.stderr.read(1024), timeout=1
                        )
                        stderr = stderr_data.decode("utf-8", errors="replace").strip()
                    except Exception:
                        pass

                    logger.warning(f"SecureTunnel: Disconnected (rc={returncode}, "
                                   f"session={session_time:.0f}s) {stderr}")

            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"SecureTunnel: Connection error: {e}")

            if not self._running:
                break

            # Exponential backoff
            logger.info(f"SecureTunnel: Reconnecting in {delay}s...")
            await asyncio.sleep(delay)
            delay = min(delay * 2, self._reconnect_delay_max)

    def is_connected(self) -> bool:
        return self._connected and self._process is not None and self._process.returncode is None

    def get_tunnel_list(self) -> List[Dict[str, Any]]:
        return [t.to_dict() for t in self._tunnels]

    def get_status(self) -> Dict[str, Any]:
        uptime = self._total_uptime
        if self._connected:
            uptime += time.time() - self._last_connect_time

        return {
            "enabled": self._enabled,
            "running": self._running,
            "connected": self.is_connected(),
            "ssh_server": f"{self._ssh_user}@{self._ssh_server}:{self._ssh_port}" if self._ssh_server else "",
            "tunnels": self.get_tunnel_list(),
            "connect_count": self._connect_count,
            "total_uptime_seconds": round(uptime, 1),
            "pid": self._process.pid if self._process and self._process.returncode is None else None,
        }
