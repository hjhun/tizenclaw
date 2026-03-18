#!/usr/bin/env python3
"""
generate_rag_web.py — Convert Tizen Web API docs to Markdown for RAG.

Reads from tizen-docs source and produces rag/web/ directory with:
  - Copied MD files (guides, tutorials, get-started)
  - Converted HTML→MD files (Doxygen API v10.0)
  - Generated index.md

Usage:
  python3 scripts/generate_rag_web.py [--source /path/to/tizen-docs/docs/application/web]

Prerequisites: Python 3.6+ (stdlib only, no pip packages needed)
"""

import argparse
import os
import re
import shutil
import sys
from html.parser import HTMLParser
from pathlib import Path

# ─────────────────────────────────────────────
# Tizen Doxygen HTML → Markdown converter
# ─────────────────────────────────────────────

class TizenHtmlToMd(HTMLParser):
    """Converts Tizen Doxygen-style HTML to readable Markdown."""

    # Tags whose text content we preserve
    BLOCK_TAGS = {"h1", "h2", "h3", "h4", "h5", "h6", "p", "div", "li", "dt", "dd", "pre", "td", "th", "tr"}
    # Paired tags that should be skipped entirely (have closing tags)
    SKIP_BLOCK_TAGS = {"script", "style", "head"}
    # Void/self-closing tags to silently ignore (no closing tag)
    VOID_TAGS = {"meta", "link", "img", "input", "area", "base", "col", "embed",
                 "param", "source", "track", "wbr"}

    def __init__(self):
        super().__init__()
        self.lines: list[str] = []
        self._buf: list[str] = []
        self._tag_stack: list[str] = []
        self._class_stack: list[str] = []
        self._in_pre = False
        self._in_skip = False
        self._skip_depth = 0
        self._list_depth = 0
        self._in_table = False
        self._table_row: list[str] = []
        self._table_header = False
        self._first_row = True

    def _current_class(self) -> str:
        return self._class_stack[-1] if self._class_stack else ""

    def handle_starttag(self, tag, attrs):
        tag = tag.lower()
        attrs_dict = dict(attrs)
        css_class = attrs_dict.get("class", "")

        # Void tags: silently ignore (no depth tracking needed)
        if tag in self.VOID_TAGS:
            return

        # Skip-block handling: track depth only for paired block tags
        if self._in_skip:
            if tag in self.SKIP_BLOCK_TAGS:
                self._skip_depth += 1
            return

        if tag in self.SKIP_BLOCK_TAGS:
            self._in_skip = True
            self._skip_depth = 1
            return

        self._tag_stack.append(tag)
        self._class_stack.append(css_class)

        if tag == "pre":
            self._flush()
            self._in_pre = True
            if "webidl" in css_class:
                self.lines.append("```webidl")
            elif "examplecode" in css_class or "signature" in css_class:
                self.lines.append("```javascript")
            else:
                self.lines.append("```")
        elif tag in ("h1", "h2", "h3", "h4", "h5", "h6"):
            self._flush()
            level = int(tag[1])
            self._buf.append("#" * level + " ")
        elif tag == "ul":
            self._flush()
            self._list_depth += 1
        elif tag == "ol":
            self._flush()
            self._list_depth += 1
        elif tag == "li":
            self._flush()
            indent = "  " * max(0, self._list_depth - 1)
            if "param" in css_class:
                self._buf.append(indent + "- **")
            else:
                self._buf.append(indent + "- ")
        elif tag == "table":
            self._flush()
            self._in_table = True
            self._first_row = True
        elif tag == "thead":
            self._table_header = True
        elif tag == "tr":
            self._table_row = []
        elif tag in ("td", "th"):
            pass
        elif tag == "em" or tag == "i":
            self._buf.append("*")
        elif tag == "b" or tag == "strong":
            self._buf.append("**")
        elif tag == "code":
            if not self._in_pre:
                self._buf.append("`")
        elif tag == "a":
            pass  # We'll handle text content normally
        elif tag == "br":
            self._buf.append("\n")
        elif tag == "hr":
            self._flush()
            self.lines.append("---")

    def handle_endtag(self, tag):
        tag = tag.lower()

        if self._in_skip:
            if tag in self.SKIP_BLOCK_TAGS:
                self._skip_depth -= 1
                if self._skip_depth <= 0:
                    self._in_skip = False
                    self._skip_depth = 0
            return

        if tag == "pre":
            self._flush_pre()
            self._in_pre = False
            self.lines.append("```")
            self.lines.append("")
        elif tag in ("h1", "h2", "h3", "h4", "h5", "h6"):
            self._flush()
            self.lines.append("")
        elif tag == "ul" or tag == "ol":
            self._list_depth = max(0, self._list_depth - 1)
            if self._list_depth == 0:
                self._flush()
        elif tag == "li":
            css_class = self._current_class()
            if "param" in css_class:
                self._buf.append("**")
            self._flush()
        elif tag == "p":
            self._flush()
            self.lines.append("")
        elif tag == "div":
            css_class = self._current_class()
            if css_class in ("brief", "description", "synopsis", "parameters",
                             "returntype", "exceptionlist", "example", "output"):
                self._flush()
                self.lines.append("")
        elif tag == "em" or tag == "i":
            self._buf.append("*")
        elif tag == "b" or tag == "strong":
            self._buf.append("**")
        elif tag == "code":
            if not self._in_pre:
                self._buf.append("`")
        elif tag == "table":
            self._in_table = False
            self.lines.append("")
        elif tag == "thead":
            self._table_header = False
        elif tag == "tr":
            if self._table_row:
                self.lines.append("| " + " | ".join(
                    cell.strip() for cell in self._table_row) + " |")
                if self._first_row:
                    self.lines.append("| " + " | ".join(
                        "---" for _ in self._table_row) + " |")
                    self._first_row = False
            self._table_row = []
        elif tag in ("td", "th"):
            cell_text = "".join(self._buf).strip()
            self._buf.clear()
            self._table_row.append(cell_text)
        elif tag == "dd":
            self._flush()
            self.lines.append("")

        if self._tag_stack and self._tag_stack[-1] == tag:
            self._tag_stack.pop()
        if self._class_stack:
            self._class_stack.pop()

    def handle_data(self, data):
        if self._in_skip:
            return
        if self._in_pre:
            self._buf.append(data)
        else:
            text = data.strip()
            if text:
                # Decode HTML entities
                text = text.replace("&gt;", ">").replace("&lt;", "<").replace("&amp;", "&")
                self._buf.append(text + " ")

    def handle_entityref(self, name):
        if self._in_skip:
            return
        entities = {"gt": ">", "lt": "<", "amp": "&", "quot": '"', "nbsp": " "}
        self._buf.append(entities.get(name, f"&{name};"))

    def handle_charref(self, name):
        if self._in_skip:
            return
        try:
            if name.startswith("x"):
                char = chr(int(name[1:], 16))
            else:
                char = chr(int(name))
            self._buf.append(char)
        except (ValueError, OverflowError):
            self._buf.append(f"&#{name};")

    def _flush(self):
        text = "".join(self._buf).strip()
        self._buf.clear()
        if text:
            self.lines.append(text)

    def _flush_pre(self):
        text = "".join(self._buf)
        self._buf.clear()
        # Remove leading/trailing blank lines but preserve internal whitespace
        stripped = text.strip("\n")
        if stripped:
            self.lines.append(stripped)

    def get_markdown(self) -> str:
        self._flush()
        result = "\n".join(self.lines)
        # Clean up excessive blank lines
        result = re.sub(r"\n{3,}", "\n\n", result)
        return result.strip() + "\n"


def convert_html_to_md(html_content: str) -> str:
    """Convert Tizen Doxygen HTML to Markdown."""
    parser = TizenHtmlToMd()
    parser.feed(html_content)
    return parser.get_markdown()


# ─────────────────────────────────────────────
# File processing
# ─────────────────────────────────────────────

def copy_guide_md_files(src_dir: Path, dest_dir: Path) -> list[str]:
    """Copy .md files from guides/ only. Returns list of relative paths."""
    copied = []
    guides_dir = src_dir / "guides"
    if not guides_dir.is_dir():
        return copied
    for md_file in sorted(guides_dir.rglob("*.md")):
        rel = md_file.relative_to(src_dir)
        # Skip media directories
        if "media" in rel.parts:
            continue
        dest = dest_dir / rel
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(md_file, dest)
        copied.append(str(rel))
    return copied


def convert_api_html(src_dir: Path, dest_dir: Path, api_version: str = "10.0") -> list[str]:
    """Convert Doxygen HTML/HTM files to MD for a specific API version."""
    converted = []
    api_base = src_dir / "api" / api_version

    # Walk all HTML and HTM files under the API version directory
    html_files = sorted(
        list(api_base.rglob("*.html")) + list(api_base.rglob("*.htm"))
    )
    for html_file in html_files:
        rel = html_file.relative_to(src_dir)
        # Skip non-content directories
        if any(part in ("scripts", "css", "images") for part in rel.parts):
            continue

        md_rel = rel.with_suffix(".md")
        dest = dest_dir / md_rel
        dest.parent.mkdir(parents=True, exist_ok=True)

        try:
            html_content = html_file.read_text(encoding="utf-8", errors="replace")
            md_content = convert_html_to_md(html_content)
            # Skip near-empty conversions
            if len(md_content.strip()) < 50:
                continue
            dest.write_text(md_content, encoding="utf-8")
            converted.append(str(md_rel))
        except Exception as e:
            print(f"  WARN: Failed to convert {rel}: {e}", file=sys.stderr)

    return converted





# ─────────────────────────────────────────────
# Index generation
# ─────────────────────────────────────────────

def generate_index(dest_dir: Path, guide_files: list[str], api_files: list[str]):
    """Generate index.md with categorized links to all RAG docs."""
    lines = [
        "# Tizen Web API Reference — RAG Index",
        "",
        "This index lists all available Tizen Web documentation for LLM reference.",
        "All files are in Markdown format converted from the official Tizen documentation.",
        "",
        "## API Usage Guides",
        "",
    ]

    # Group guides by category subdirectory
    guides_by_cat: dict[str, list[str]] = {}
    for f in sorted(guide_files):
        parts = Path(f).parts
        # e.g. guides/alarm/alarms.md → category = "alarm"
        cat = parts[1] if len(parts) > 2 else "general"
        guides_by_cat.setdefault(cat, []).append(f)

    for cat in sorted(guides_by_cat.keys()):
        cat_title = cat.replace("-", " ").replace("_", " ").title()
        lines.append(f"### {cat_title}")
        lines.append("")
        for f in sorted(guides_by_cat[cat]):
            name = Path(f).stem.replace("-", " ").replace("_", " ").title()
            lines.append(f"- [{name}]({f})")
        lines.append("")

    lines.extend(["## API Reference (Tizen 10.0)", ""])

    # Group API files by profile (mobile, wearable, tv)
    api_by_profile: dict[str, list[str]] = {}
    for f in sorted(api_files):
        parts = Path(f).parts
        profile = "general"
        if "device_api" in parts:
            idx = list(parts).index("device_api")
            if idx + 1 < len(parts):
                profile = parts[idx + 1]
        elif "w3c_api" in parts:
            profile = "w3c"
        elif "ui_fw_api" in parts:
            profile = "ui_framework"
        elif "wearable_widget" in parts:
            profile = "wearable_widget"
        api_by_profile.setdefault(profile, []).append(f)

    for profile in sorted(api_by_profile.keys()):
        profile_title = profile.replace("_", " ").title()
        lines.append(f"### {profile_title}")
        lines.append("")
        for f in sorted(api_by_profile[profile]):
            name = Path(f).stem.replace("_", " ").title()
            lines.append(f"- [{name}]({f})")
        lines.append("")

    index_path = dest_dir / "index.md"
    index_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"  Generated: index.md")


# ─────────────────────────────────────────────
# Main
# ─────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="Generate RAG web docs from tizen-docs")
    parser.add_argument(
        "--source",
        default="/home/hjhun/samba/github/tizen-docs/docs/application/web",
        help="Path to tizen-docs web directory",
    )
    parser.add_argument(
        "--output",
        default=None,
        help="Output directory (default: <project_root>/rag/web)",
    )
    parser.add_argument(
        "--api-version",
        default="10.0",
        help="API version to convert (default: 10.0)",
    )
    args = parser.parse_args()

    src_dir = Path(args.source).resolve()
    if not src_dir.is_dir():
        print(f"ERROR: Source directory not found: {src_dir}", file=sys.stderr)
        sys.exit(1)

    # Determine project root (script is in scripts/)
    project_root = Path(__file__).resolve().parent.parent
    dest_dir = Path(args.output).resolve() if args.output else project_root / "rag" / "web"

    print(f"Source: {src_dir}")
    print(f"Output: {dest_dir}")
    print(f"API version: {args.api_version}")
    print()

    # Clean output directory
    if dest_dir.exists():
        shutil.rmtree(dest_dir)
    dest_dir.mkdir(parents=True, exist_ok=True)

    # Step 1: Copy guide MD files (API usage guides only)
    print("Step 1: Copying API usage guides...")
    guide_files = copy_guide_md_files(src_dir, dest_dir)
    print(f"  Copied {len(guide_files)} guide MD files")

    # Step 2: Convert Doxygen HTML/HTM to MD (API version only)
    print(f"Step 2: Converting API HTML/HTM (v{args.api_version})...")
    api_files = convert_api_html(src_dir, dest_dir, args.api_version)
    print(f"  Converted {len(api_files)} API files")

    # Step 3: Generate index
    print("Step 3: Generating index.md...")
    generate_index(dest_dir, guide_files, api_files)

    total = len(guide_files) + len(api_files) + 1  # +1 for index
    print(f"\nDone! {total} files in {dest_dir}")


if __name__ == "__main__":
    main()
