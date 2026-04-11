from __future__ import annotations

from src.commands import build_command_inventory


CLI_SURFACE = {
    "name": "cli",
    "role": "operator-facing parity helpers",
    "commands": [command["name"] for command in build_command_inventory()],
}
