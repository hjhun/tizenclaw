from __future__ import annotations

from pathlib import Path


def _plugin_inventory(root: Path) -> list[str]:
    return sorted(
        str(path.parent.relative_to(root))
        for path in (root / "src").glob("tizenclaw-metadata-*/Cargo.toml")
    )


PLUGIN_SURFACE = {
    "name": "plugins",
    "role": "plugin contract parity helpers",
    "plugins": _plugin_inventory(Path(__file__).resolve().parents[2]),
}
