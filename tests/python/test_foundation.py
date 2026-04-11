import unittest
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from tizenclaw_py import (
    API_SURFACE,
    CLI_SURFACE,
    PLUGIN_SURFACE,
    RUNTIME_SURFACE,
    TOOL_SURFACE,
)


class FoundationTest(unittest.TestCase):
    def test_python_parity_surfaces_are_named(self) -> None:
        surfaces = [
            API_SURFACE,
            CLI_SURFACE,
            PLUGIN_SURFACE,
            RUNTIME_SURFACE,
            TOOL_SURFACE,
        ]
        self.assertEqual(
            [surface["name"] for surface in surfaces],
            ["api", "cli", "plugins", "runtime", "tools"],
        )

    def test_compatibility_package_exposes_real_inventory(self) -> None:
        self.assertIn("manifest", CLI_SURFACE["commands"])
        self.assertGreater(TOOL_SURFACE["tool_count"], 0)
        self.assertIn("src.main", RUNTIME_SURFACE["summary"]["python_modules"])
        self.assertGreater(len(PLUGIN_SURFACE["plugins"]), 0)
        self.assertIn("build_port_manifest", API_SURFACE["entrypoints"])


if __name__ == "__main__":
    unittest.main()
