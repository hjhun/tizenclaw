"""
TizenClaw Skill Plugin Manager — manages skill plugins from RPK packages.

Matches C++ SkillPluginManager functionality:
  - Parse pipe-delimited skill names from manifest metadata
  - Create/remove symlinks from package skill dirs to the skills directory
  - Singleton pattern for process-wide access
"""
import logging
import os
import shutil
from typing import List, Optional

logger = logging.getLogger(__name__)

SKILLS_DIR = "/opt/usr/share/tizenclaw/tools/skills"


class SkillPluginManager:
    """Manages skill plugin installation from RPK packages."""

    _instance: Optional['SkillPluginManager'] = None

    def __init__(self, skills_dir: str = SKILLS_DIR):
        self._skills_dir = skills_dir

    @classmethod
    def get_instance(cls) -> 'SkillPluginManager':
        if cls._instance is None:
            cls._instance = SkillPluginManager()
        return cls._instance

    # ── Skill name parsing ──

    @staticmethod
    def parse_skill_names(value: str) -> List[str]:
        """Parse pipe-delimited skill names (e.g. 'skill_a|skill_b|skill_c')."""
        if not value or not value.strip():
            return []
        names = []
        for part in value.split("|"):
            name = part.strip()
            if name:
                names.append(name)
        return names

    # ── Symlink management ──

    def link_skill_dir(self, source: str, target: str) -> bool:
        """Link a skill source directory to the target path.

        If target already exists, it is removed first.
        Uses symlink if possible, falls back to copy.
        """
        if not os.path.isdir(source):
            logger.error(f"SkillPluginManager: Source not found: {source}")
            return False

        try:
            # Remove existing target
            if os.path.exists(target) or os.path.islink(target):
                if os.path.islink(target):
                    os.unlink(target)
                elif os.path.isdir(target):
                    shutil.rmtree(target)

            # Try symlink first
            try:
                os.symlink(source, target)
            except OSError:
                # Fallback to copy (e.g. cross-filesystem)
                shutil.copytree(source, target)

            logger.info(f"SkillPluginManager: Linked {source} → {target}")
            return True
        except Exception as e:
            logger.error(f"SkillPluginManager: Link failed: {e}")
            return False

    def remove_skill_dir(self, target: str):
        """Remove a skill directory or symlink."""
        try:
            if os.path.islink(target):
                os.unlink(target)
            elif os.path.isdir(target):
                shutil.rmtree(target)
            elif os.path.exists(target):
                os.unlink(target)
            logger.info(f"SkillPluginManager: Removed {target}")
        except Exception as e:
            logger.error(f"SkillPluginManager: Remove failed: {e}")

    # ── Package-level operations ──

    def install_package_skills(self, package_id: str,
                               skill_names: str, lib_dir: str) -> int:
        """Install skills from a package.

        Args:
            package_id: RPK package ID
            skill_names: Pipe-delimited skill names from manifest
            lib_dir: Package lib directory containing skill subdirs

        Returns:
            Number of skills successfully installed
        """
        names = self.parse_skill_names(skill_names)
        installed = 0
        for name in names:
            source = os.path.join(lib_dir, name)
            target = os.path.join(self._skills_dir, f"{package_id}__{name}")
            if self.link_skill_dir(source, target):
                installed += 1
        logger.info(f"SkillPluginManager: Installed {installed}/{len(names)} "
                    f"skills from package '{package_id}'")
        return installed

    def uninstall_package_skills(self, package_id: str):
        """Remove all skills belonging to a package."""
        prefix = f"{package_id}__"
        removed = 0
        if os.path.isdir(self._skills_dir):
            for entry in os.listdir(self._skills_dir):
                if entry.startswith(prefix):
                    target = os.path.join(self._skills_dir, entry)
                    self.remove_skill_dir(target)
                    removed += 1
        logger.info(f"SkillPluginManager: Removed {removed} skills "
                    f"from package '{package_id}'")

    def list_installed_skills(self) -> List[dict]:
        """List all installed skill plugins."""
        skills = []
        if not os.path.isdir(self._skills_dir):
            return skills
        for entry in sorted(os.listdir(self._skills_dir)):
            path = os.path.join(self._skills_dir, entry)
            manifest_path = os.path.join(path, "manifest.json")
            info = {
                "name": entry,
                "path": path,
                "is_symlink": os.path.islink(path),
                "has_manifest": os.path.isfile(manifest_path),
            }
            if info["has_manifest"]:
                try:
                    import json
                    with open(manifest_path, "r", encoding="utf-8") as f:
                        m = json.load(f)
                    info["version"] = m.get("version", "unknown")
                    info["description"] = m.get("description", "")
                except Exception:
                    pass
            skills.append(info)
        return skills
