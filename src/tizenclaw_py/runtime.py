from __future__ import annotations

from pathlib import Path

from src.runtime import build_runtime_summary


RUNTIME_SURFACE = {
    "name": "runtime",
    "role": "audit and explanation mirror of the Rust runtime",
    "summary": build_runtime_summary(Path(__file__).resolve().parents[2]),
}
