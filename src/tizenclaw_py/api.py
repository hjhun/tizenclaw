from __future__ import annotations

from pathlib import Path

from src.port_manifest import build_port_manifest


API_SURFACE = {
    "name": "api",
    "role": "shared contracts mirrored from the canonical Rust workspace",
    "entrypoints": (
        "build_port_manifest",
        "run_parity_audit",
        "QueryEngine",
    ),
    "manifest_preview": build_port_manifest(Path(__file__).resolve().parents[2])[
        "purpose"
    ],
}
