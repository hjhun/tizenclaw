"""
TizenClaw Skill Watcher — monitors skill directories for changes using inotify.

When skill files (.tool.md, manifest.json, *.py) are modified, added, or deleted,
triggers a reload of the ToolIndexer to pick up changes without daemon restart.
"""
import asyncio
import ctypes
import ctypes.util
import logging
import os
import struct
import time
from typing import Optional, Callable

logger = logging.getLogger(__name__)

# inotify constants
IN_CREATE = 0x100
IN_DELETE = 0x200
IN_MODIFY = 0x02
IN_MOVED_TO = 0x80
IN_MOVED_FROM = 0x40
IN_CLOSE_WRITE = 0x08
IN_ISDIR = 0x40000000

WATCH_MASK = IN_CREATE | IN_DELETE | IN_MODIFY | IN_MOVED_TO | IN_CLOSE_WRITE

# File extensions to watch
WATCHED_EXTENSIONS = {".md", ".json", ".py", ".sh"}


class SkillWatcher:
    """Watches skill directories for changes using Linux inotify."""

    def __init__(self, watch_dirs: list = None,
                 reload_callback: Callable = None,
                 debounce_seconds: float = 2.0):
        self._watch_dirs = watch_dirs or [
            "/opt/usr/share/tizenclaw/tools/cli",
            "/opt/usr/share/tizenclaw/tools/skills",
        ]
        self._reload_cb = reload_callback
        self._debounce = debounce_seconds
        self._running = False
        self._task: Optional[asyncio.Task] = None
        self._inotify_fd = -1
        self._watch_descriptors = {}
        self._last_reload_time = 0.0
        self._reload_count = 0

    def _init_inotify(self) -> bool:
        """Initialize inotify file descriptor and add watches."""
        try:
            libc = ctypes.CDLL(ctypes.util.find_library("c"), use_errno=True)
            self._inotify_fd = libc.inotify_init1(0x800)  # IN_NONBLOCK
            if self._inotify_fd < 0:
                logger.error("SkillWatcher: inotify_init1 failed")
                return False

            for watch_dir in self._watch_dirs:
                if not os.path.isdir(watch_dir):
                    continue
                # Watch the directory itself
                self._add_watch(libc, watch_dir)
                # Watch subdirectories (one level)
                try:
                    for entry in os.listdir(watch_dir):
                        subdir = os.path.join(watch_dir, entry)
                        if os.path.isdir(subdir):
                            self._add_watch(libc, subdir)
                except Exception:
                    pass

            logger.info(f"SkillWatcher: Watching {len(self._watch_descriptors)} directories")
            return True
        except Exception as e:
            logger.error(f"SkillWatcher: Init failed: {e}")
            return False

    def _add_watch(self, libc, path: str):
        wd = libc.inotify_add_watch(
            self._inotify_fd,
            path.encode("utf-8"),
            WATCH_MASK
        )
        if wd >= 0:
            self._watch_descriptors[wd] = path

    async def start(self):
        """Start the file watcher."""
        if not self._init_inotify():
            logger.warning("SkillWatcher: Could not initialize, disabled")
            return

        self._running = True
        self._task = asyncio.create_task(self._watch_loop())
        logger.info("SkillWatcher: Started")

    async def stop(self):
        self._running = False
        if self._task:
            self._task.cancel()
            try:
                await self._task
            except asyncio.CancelledError:
                pass
        if self._inotify_fd >= 0:
            os.close(self._inotify_fd)
            self._inotify_fd = -1
        logger.info("SkillWatcher: Stopped")

    async def _watch_loop(self):
        """Main watch loop — reads inotify events."""
        loop = asyncio.get_running_loop()
        while self._running:
            try:
                # Read events (non-blocking)
                data = await loop.run_in_executor(None, self._read_events)
                if data and self._should_reload(data):
                    await self._trigger_reload()
                await asyncio.sleep(0.5)
            except asyncio.CancelledError:
                break
            except Exception as e:
                logger.error(f"SkillWatcher: Watch error: {e}")
                await asyncio.sleep(5)

    def _read_events(self) -> list:
        """Read pending inotify events (blocking with timeout)."""
        import select
        events = []
        try:
            ready, _, _ = select.select([self._inotify_fd], [], [], 1.0)
            if not ready:
                return events

            buf = os.read(self._inotify_fd, 4096)
            offset = 0
            while offset < len(buf):
                wd, mask, cookie, name_len = struct.unpack_from("iIII", buf, offset)
                offset += struct.calcsize("iIII")
                name = buf[offset:offset + name_len].rstrip(b"\x00").decode("utf-8", errors="replace")
                offset += name_len
                events.append((wd, mask, name))
        except Exception:
            pass
        return events

    def _should_reload(self, events: list) -> bool:
        """Check if any event matches watched file types."""
        for wd, mask, name in events:
            if not name:
                continue
            _, ext = os.path.splitext(name)
            if ext.lower() in WATCHED_EXTENSIONS:
                dir_path = self._watch_descriptors.get(wd, "?")
                logger.info(f"SkillWatcher: Change detected: {dir_path}/{name}")
                return True
        return False

    async def _trigger_reload(self):
        """Trigger skill reload with debounce."""
        now = time.time()
        if now - self._last_reload_time < self._debounce:
            return

        self._last_reload_time = now
        self._reload_count += 1
        logger.info(f"SkillWatcher: Triggering reload #{self._reload_count}")

        if self._reload_cb:
            try:
                if asyncio.iscoroutinefunction(self._reload_cb):
                    await self._reload_cb()
                else:
                    self._reload_cb()
            except Exception as e:
                logger.error(f"SkillWatcher: Reload callback failed: {e}")

    def get_status(self) -> dict:
        return {
            "running": self._running,
            "watched_dirs": len(self._watch_descriptors),
            "reload_count": self._reload_count,
            "last_reload": self._last_reload_time,
        }
