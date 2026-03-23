"""
TizenClaw OTA Updater — Over-the-Air skill update system.

Matches C++ OtaUpdater functionality:
  - Version comparison (semver-style)
  - Remote manifest parsing (check for available updates)
  - Skill download, verify (SHA-256), install with backup
  - Rollback to previous version
  - Auto-check on configurable interval
"""
import asyncio
import hashlib
import json
import logging
import os
import shutil
import ssl
import tarfile
import tempfile
import time
import urllib.request
from typing import Dict, List, Any, Optional
from dataclasses import dataclass, field

logger = logging.getLogger(__name__)

SKILLS_DIR = "/opt/usr/share/tizenclaw/tools/skills"
OTA_CONFIG_PATH = "/opt/usr/share/tizenclaw/config/ota_config.json"
BACKUP_DIR = "/opt/usr/share/tizenclaw/work/ota_backups"


@dataclass
class SkillUpdateInfo:
    name: str = ""
    local_version: str = "0.0.0"
    remote_version: str = "0.0.0"
    url: str = ""
    sha256: str = ""
    update_available: bool = False
    description: str = ""


class OtaUpdater:
    """Over-the-Air skill update manager."""

    def __init__(self, skills_dir: str = SKILLS_DIR,
                 reload_callback=None):
        self._skills_dir = skills_dir
        self._reload_cb = reload_callback
        self._manifest_url = ""
        self._auto_check_hours = 24
        self._auto_update = False
        self._ssl_ctx: Optional[ssl.SSLContext] = None
        self._running = False
        self._task: Optional[asyncio.Task] = None

        try:
            self._ssl_ctx = ssl.create_default_context()
        except Exception:
            self._ssl_ctx = ssl._create_unverified_context()

    # ── Version comparison ──

    @staticmethod
    def is_newer_version(current: str, remote: str) -> bool:
        """Compare semver-style version strings."""
        def parse(v):
            parts = []
            for p in v.split("."):
                try:
                    parts.append(int(p))
                except ValueError:
                    parts.append(0)
            return parts

        cur = parse(current)
        rem = parse(remote)
        max_len = max(len(cur), len(rem))
        cur.extend([0] * (max_len - len(cur)))
        rem.extend([0] * (max_len - len(rem)))
        return rem > cur

    # ── Config ──

    def load_config(self, path: str = OTA_CONFIG_PATH) -> bool:
        if not os.path.isfile(path):
            logger.info(f"OtaUpdater: Config not found at {path}")
            return False
        try:
            with open(path, "r", encoding="utf-8") as f:
                cfg = json.load(f)
            self._manifest_url = cfg.get("manifest_url", "")
            self._auto_check_hours = cfg.get("auto_check_interval_hours", 24)
            self._auto_update = cfg.get("auto_update", False)
            logger.info(f"OtaUpdater: Config loaded, manifest_url={self._manifest_url}")
            return True
        except Exception as e:
            logger.error(f"OtaUpdater: Config load error: {e}")
            return False

    def get_manifest_url(self) -> str:
        return self._manifest_url

    # ── Manifest parsing ──

    def _get_local_version(self, skill_name: str) -> str:
        """Read version from local skill's manifest.json."""
        manifest_path = os.path.join(self._skills_dir, skill_name, "manifest.json")
        if not os.path.isfile(manifest_path):
            return "0.0.0"
        try:
            with open(manifest_path, "r", encoding="utf-8") as f:
                m = json.load(f)
            return m.get("version", "0.0.0")
        except Exception:
            return "0.0.0"

    def parse_manifest(self, manifest_json: str,
                       skills_dir: str = "") -> List[SkillUpdateInfo]:
        """Parse a remote manifest JSON into update info list."""
        if not skills_dir:
            skills_dir = self._skills_dir
        try:
            data = json.loads(manifest_json)
        except Exception:
            return []

        skills = data.get("skills", [])
        updates = []
        for s in skills:
            name = s.get("name", "")
            if not name:
                continue
            remote_ver = s.get("version", "0.0.0")
            local_ver = self._get_local_version(name)
            info = SkillUpdateInfo(
                name=name,
                local_version=local_ver,
                remote_version=remote_ver,
                url=s.get("url", ""),
                sha256=s.get("sha256", ""),
                update_available=self.is_newer_version(local_ver, remote_ver),
                description=s.get("description", ""),
            )
            updates.append(info)
        return updates

    # ── Check for updates ──

    def check_for_updates(self) -> str:
        """Fetch remote manifest and return JSON with available updates."""
        if not self._manifest_url:
            return json.dumps({"error": "No manifest URL configured"})

        try:
            req = urllib.request.Request(self._manifest_url)
            with urllib.request.urlopen(req, context=self._ssl_ctx, timeout=30) as resp:
                manifest_json = resp.read().decode("utf-8")
        except Exception as e:
            return json.dumps({"error": f"Failed to fetch manifest: {e}"})

        updates = self.parse_manifest(manifest_json)
        available = [u for u in updates if u.update_available]

        return json.dumps({
            "available_count": len(available),
            "updates": [
                {
                    "name": u.name,
                    "local_version": u.local_version,
                    "remote_version": u.remote_version,
                    "description": u.description,
                }
                for u in available
            ],
            "total_skills": len(updates),
        })

    # ── Download & Install ──

    def _download_file(self, url: str, dest: str) -> bool:
        try:
            req = urllib.request.Request(url)
            with urllib.request.urlopen(req, context=self._ssl_ctx, timeout=120) as resp:
                with open(dest, "wb") as f:
                    f.write(resp.read())
            return True
        except Exception as e:
            logger.error(f"OtaUpdater: Download failed: {e}")
            return False

    @staticmethod
    def _verify_sha256(filepath: str, expected: str) -> bool:
        if not expected:
            return True  # No checksum to verify
        h = hashlib.sha256()
        with open(filepath, "rb") as f:
            for chunk in iter(lambda: f.read(8192), b""):
                h.update(chunk)
        return h.hexdigest() == expected

    def _backup_skill(self, skill_name: str) -> bool:
        src = os.path.join(self._skills_dir, skill_name)
        if not os.path.isdir(src):
            return True
        os.makedirs(BACKUP_DIR, exist_ok=True)
        dst = os.path.join(BACKUP_DIR, skill_name)
        try:
            if os.path.exists(dst):
                shutil.rmtree(dst)
            shutil.copytree(src, dst)
            return True
        except Exception as e:
            logger.error(f"OtaUpdater: Backup failed for {skill_name}: {e}")
            return False

    def update_skill(self, skill_name: str) -> str:
        """Download and install a specific skill update."""
        if not self._manifest_url:
            return json.dumps({"error": "No manifest URL configured"})

        # Fetch manifest
        try:
            req = urllib.request.Request(self._manifest_url)
            with urllib.request.urlopen(req, context=self._ssl_ctx, timeout=30) as resp:
                manifest_json = resp.read().decode("utf-8")
        except Exception as e:
            return json.dumps({"error": f"Fetch manifest failed: {e}"})

        updates = self.parse_manifest(manifest_json)
        target = None
        for u in updates:
            if u.name == skill_name and u.update_available:
                target = u
                break
        if not target:
            return json.dumps({"error": f"No update available for '{skill_name}'"})

        if not target.url:
            return json.dumps({"error": "No download URL in manifest"})

        # Backup current version
        self._backup_skill(skill_name)

        # Download
        with tempfile.NamedTemporaryFile(suffix=".tar.gz", delete=False) as tmp:
            tmp_path = tmp.name

        try:
            if not self._download_file(target.url, tmp_path):
                return json.dumps({"error": "Download failed"})

            # Verify SHA-256
            if not self._verify_sha256(tmp_path, target.sha256):
                return json.dumps({"error": "SHA-256 verification failed"})

            # Extract
            dest = os.path.join(self._skills_dir, skill_name)
            if os.path.exists(dest):
                shutil.rmtree(dest)
            os.makedirs(dest, exist_ok=True)
            with tarfile.open(tmp_path, "r:gz") as tar:
                tar.extractall(dest)

            # Trigger reload
            if self._reload_cb:
                self._reload_cb()

            return json.dumps({
                "status": "updated",
                "skill": skill_name,
                "from_version": target.local_version,
                "to_version": target.remote_version,
            })
        except Exception as e:
            # Rollback on failure
            self.rollback_skill(skill_name)
            return json.dumps({"error": f"Install failed: {e}"})
        finally:
            if os.path.exists(tmp_path):
                os.unlink(tmp_path)

    # ── Rollback ──

    def rollback_skill(self, skill_name: str) -> str:
        backup = os.path.join(BACKUP_DIR, skill_name)
        if not os.path.isdir(backup):
            return json.dumps({"error": f"No backup found for '{skill_name}'"})

        dest = os.path.join(self._skills_dir, skill_name)
        try:
            if os.path.exists(dest):
                shutil.rmtree(dest)
            shutil.copytree(backup, dest)
            if self._reload_cb:
                self._reload_cb()
            return json.dumps({"status": "rolled_back", "skill": skill_name})
        except Exception as e:
            return json.dumps({"error": f"Rollback failed: {e}"})

    # ── Auto-check loop ──

    async def start_auto_check(self):
        """Start periodic auto-check loop."""
        if not self._manifest_url:
            logger.info("OtaUpdater: No manifest URL, auto-check disabled")
            return
        self._running = True
        self._task = asyncio.create_task(self._auto_check_loop())
        logger.info(f"OtaUpdater: Auto-check started ({self._auto_check_hours}h interval)")

    async def stop(self):
        self._running = False
        if self._task:
            self._task.cancel()
            try:
                await self._task
            except asyncio.CancelledError:
                pass

    async def _auto_check_loop(self):
        while self._running:
            try:
                await asyncio.sleep(self._auto_check_hours * 3600)
                result = self.check_for_updates()
                data = json.loads(result)
                if data.get("available_count", 0) > 0:
                    logger.info(f"OtaUpdater: {data['available_count']} update(s) available")
                    if self._auto_update:
                        for u in data.get("updates", []):
                            self.update_skill(u["name"])
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"OtaUpdater: Auto-check error: {e}")

    def get_status(self) -> Dict[str, Any]:
        return {
            "manifest_url": self._manifest_url,
            "auto_check_hours": self._auto_check_hours,
            "auto_update": self._auto_update,
            "running": self._running,
        }
