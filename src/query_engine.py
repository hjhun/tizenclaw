"""Query helpers for parity inventory and summary views."""

from __future__ import annotations

from pathlib import Path

from .port_manifest import build_port_manifest


class QueryEngine:
    """Searches the parity manifest and emits compact result sets."""

    def __init__(self, root: Path):
        self.root = root
        self.manifest = build_port_manifest(root)

    def _records(self, domain: str) -> list[dict[str, object]]:
        runtime = self.manifest["runtime_summary"]
        docs = self.manifest["document_references"]

        records = {
            "commands": self.manifest["command_inventory"],
            "tools": self.manifest["tool_pool"]["tools"],
            "modules": [
                {"name": name, "category": "python_module"}
                for name in runtime["python_modules"]
            ],
            "docs": docs,
            "crates": [
                {"name": name, "category": "rust_workspace"}
                for name in runtime["rust_workspace_crates"]
            ]
            + [
                {"name": name, "category": "legacy_workspace"}
                for name in runtime["legacy_workspace_crates"]
            ],
        }
        if domain == "all":
            merged: list[dict[str, object]] = []
            for items in records.values():
                merged.extend(items)
            return merged
        return list(records[domain])

    def search(self, term: str, domain: str = "all") -> dict[str, object]:
        lowered = term.lower()
        matches = []
        for record in self._records(domain):
            haystack = " ".join(str(value) for value in record.values()).lower()
            if lowered in haystack:
                matches.append(record)

        return {
            "term": term,
            "domain": domain,
            "match_count": len(matches),
            "matches": matches,
        }

    def summary(self) -> dict[str, object]:
        return {
            "commands": len(self.manifest["command_inventory"]),
            "tools": self.manifest["tool_pool"]["tool_count"],
            "python_modules": len(self.manifest["runtime_summary"]["python_modules"]),
            "document_references": len(self.manifest["document_references"]),
        }
