import unittest
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "src"
if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

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


if __name__ == "__main__":
    unittest.main()
