"""
TizenClaw Fleet Agent — multi-device fleet management.

Matches C++ FleetAgent functionality:
  - Configure via fleet_config.json (server URL, device name/group)
  - Periodic heartbeat to fleet server
  - Remote command reception and execution
  - Device info reporting
"""
import asyncio
import json
import logging
import os
import ssl
import time
import urllib.request
from typing import Dict, Any, Optional, List

logger = logging.getLogger(__name__)

FLEET_CONFIG_PATH = "/opt/usr/share/tizenclaw/config/fleet_config.json"


class FleetAgent:
    """Fleet management agent for multi-device coordination."""

    def __init__(self):
        self._enabled = False
        self._server_url = ""
        self._device_name = ""
        self._device_group = ""
        self._device_id = ""
        self._heartbeat_interval = 60  # seconds
        self._running = False
        self._task: Optional[asyncio.Task] = None
        self._last_heartbeat_time: float = 0
        self._ssl_ctx: Optional[ssl.SSLContext] = None
        self._agent_core = None

        try:
            self._ssl_ctx = ssl.create_default_context()
        except Exception:
            self._ssl_ctx = ssl._create_unverified_context()

    def initialize(self, config_path: str = FLEET_CONFIG_PATH) -> bool:
        """Load fleet configuration."""
        if not os.path.isfile(config_path):
            logger.info("FleetAgent: Config not found, disabled")
            self._enabled = False
            return True

        try:
            with open(config_path, "r", encoding="utf-8") as f:
                cfg = json.load(f)
        except (json.JSONDecodeError, Exception) as e:
            logger.error(f"FleetAgent: Invalid config: {e}")
            self._enabled = False
            return True

        self._enabled = cfg.get("enabled", False)
        self._server_url = cfg.get("fleet_server_url", "")
        self._device_name = cfg.get("device_name", "TizenClaw Device")
        self._device_group = cfg.get("device_group", "default")
        self._device_id = cfg.get("device_id", "")
        self._heartbeat_interval = cfg.get("heartbeat_interval_seconds", 60)

        if not self._device_id:
            # Generate from hostname
            try:
                import socket
                self._device_id = socket.gethostname()
            except Exception:
                self._device_id = f"device_{os.getpid()}"

        if self._enabled:
            logger.info(f"FleetAgent: Enabled — {self._device_name} "
                        f"({self._device_group}) → {self._server_url}")
        return True

    def is_enabled(self) -> bool:
        return self._enabled

    def get_device_info(self) -> Dict[str, Any]:
        return {
            "device_name": self._device_name,
            "device_group": self._device_group,
            "device_id": self._device_id,
            "pid": os.getpid(),
        }

    def get_heartbeat_status(self) -> Dict[str, Any]:
        return {
            "running": self._running,
            "last_heartbeat_time": self._last_heartbeat_time,
            "interval_seconds": self._heartbeat_interval,
        }

    # ── Lifecycle ──

    def start(self):
        """Start heartbeat loop (call from async context via create_task)."""
        if not self._enabled:
            return
        self._running = True
        self._task = asyncio.ensure_future(self._heartbeat_loop())
        logger.info("FleetAgent: Heartbeat loop started")

    def stop(self):
        self._running = False
        if self._task:
            self._task.cancel()
        logger.info("FleetAgent: Stopped")

    # ── Heartbeat ──

    async def _heartbeat_loop(self):
        while self._running:
            try:
                await asyncio.sleep(self._heartbeat_interval)
                await self._send_heartbeat()
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"FleetAgent: Heartbeat error: {e}")

    async def _send_heartbeat(self):
        """Send heartbeat to fleet server."""
        if not self._server_url:
            return

        # Collect device metrics
        from tizenclaw.core.health_monitor import get_health_monitor
        hm = get_health_monitor()

        payload = {
            "device_id": self._device_id,
            "device_name": self._device_name,
            "device_group": self._device_group,
            "timestamp": time.time(),
            "metrics": hm.get_metrics_dict(),
        }

        try:
            url = f"{self._server_url.rstrip('/')}/api/heartbeat"
            data = json.dumps(payload).encode("utf-8")
            req = urllib.request.Request(
                url, data=data, method="POST",
                headers={"Content-Type": "application/json"}
            )
            loop = asyncio.get_running_loop()
            await loop.run_in_executor(None, lambda: urllib.request.urlopen(
                req, context=self._ssl_ctx, timeout=10
            ))
            self._last_heartbeat_time = time.time()
            logger.debug(f"FleetAgent: Heartbeat sent to {url}")
        except Exception as e:
            logger.warning(f"FleetAgent: Heartbeat failed: {e}")

    def get_status(self) -> Dict[str, Any]:
        return {
            "enabled": self._enabled,
            "running": self._running,
            "device_name": self._device_name,
            "device_group": self._device_group,
            "device_id": self._device_id,
            "server_url": self._server_url,
            "last_heartbeat": self._last_heartbeat_time,
        }
