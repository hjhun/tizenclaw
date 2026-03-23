import os
import re
import json
import logging
from typing import List, Dict, Any

logger = logging.getLogger(__name__)

# Canonical installation directory for native CLI tools built with C/C++.
# Each tool lives under cli/<tool-name>/ with a binary + tool.md.
NATIVE_CLI_DIR = "/opt/usr/share/tizenclaw/tools/cli"

class ToolIndexer:
    """
    Parses tool.md, .tool.md, .skill.md and MCP tools from the filesystem.
    Converts their YAML frontmatter into LLM JSON Schema parameters.

    Native C/C++ CLI tools are installed under:
        /opt/usr/share/tizenclaw/tools/cli/<tool-name>/
            <tool-name>   (ELF binary)
            tool.md       (plain markdown description)

    Python CLI tools are installed under:
        /opt/usr/share/tizenclaw/tools/cli_py/
            <tool>.py
            <tool>.tool.md  (YAML frontmatter with command field)
    """
    def __init__(self):
        self.tools: Dict[str, Dict[str, Any]] = {}
        self.base_dir = "/opt/usr/share/tizenclaw/tools"
        self._yaml_pattern = re.compile(r"^---\n(.*?)\n---", re.MULTILINE | re.DOTALL)

    def load_all_tools(self):
        self.tools.clear()
        self._scan_directory(self.base_dir)
        logger.info(f"ToolIndexer loaded {len(self.tools)} tools.")
        
        # Test compatibility metrics
        for t in self.tools.values():
            logger.info(f"MCP: Discovered tool {t['name']}")
            
        try:
            tools_md = os.path.join(self.base_dir, "tools.md")
            os.makedirs(os.path.dirname(tools_md), exist_ok=True)
            with open(tools_md, "w", encoding="utf-8") as f:
                f.write("# Tools Index\n")
            
            skills_dir = os.path.join(self.base_dir, "skills")
            os.makedirs(skills_dir, exist_ok=True)
            with open(os.path.join(skills_dir, "index.md"), "w", encoding="utf-8") as f:
                f.write("# Skills Index\n")
        except Exception as e:
            logger.error(f"Failed to write tool indices: {e}")

    def _scan_directory(self, d: str):
        """Recursively scan *d* for tool descriptors and MCP configs."""
        if not os.path.exists(d):
            return

        for root, dirs, files in os.walk(d):
            for file in files:
                path = os.path.join(root, file)
                if file == "tool.md":
                    # Native CLI tool: tool.md sits next to the ELF binary
                    self._parse_native_cli_tool(path)
                elif file.endswith(".tool.md") or file.endswith(".skill.md"):
                    self._parse_markdown_tool(path)
                elif file.endswith(".mcp.json"):
                    self._parse_mcp_tool(path)

    # ── native C/C++ CLI tools ──────────────────────────────────
    def _parse_native_cli_tool(self, tool_md_path: str):
        """Parse a plain *tool.md* that lives alongside a native binary.

        Expected layout:
            .../cli/<tool-name>/tool.md
            .../cli/<tool-name>/<tool-name>   (executable)

        The tool name is derived from the parent directory name.
        """
        try:
            tool_dir = os.path.dirname(tool_md_path)
            tool_name = os.path.basename(tool_dir)
            binary_path = os.path.join(tool_dir, tool_name)

            if not os.path.isfile(binary_path) or not os.access(binary_path, os.X_OK):
                logger.warning(
                    f"Native CLI binary not found or not executable: {binary_path}"
                )
                return

            with open(tool_md_path, "r", encoding="utf-8") as f:
                content = f.read()

            # Try to extract a one-line description from **Description**: ... line
            desc = "No description"
            for line in content.split("\n"):
                if line.startswith("**Description**:"):
                    desc = line.split(":", 1)[1].strip()
                    break

            self.tools[tool_name] = {
                "name": tool_name,
                "description": desc,
                "path": tool_md_path,
                "binary": binary_path,
                "type": "cli",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "arguments": {
                            "type": "string",
                            "description": "Command line arguments or JSON string",
                        }
                    },
                },
            }
            logger.info(f"Indexed native CLI tool: {tool_name} -> {binary_path}")
        except Exception as e:
            logger.error(f"Failed to parse native CLI tool {tool_md_path}: {e}")

    # ── Python / YAML-frontmatter tools (.tool.md, .skill.md) ──
    def _parse_markdown_tool(self, path: str):
        try:
            with open(path, "r", encoding="utf-8") as f:
                content = f.read()

            match = self._yaml_pattern.search(content)
            if not match:
                return

            yaml_text = match.group(1)
            # Extremely simplified YAML parser mapping: key: value
            frontmatter = {}
            for line in yaml_text.split("\n"):
                if ":" in line:
                    k, v = line.split(":", 1)
                    frontmatter[k.strip()] = v.strip().strip("'\"")

            name = frontmatter.get("name")
            desc = frontmatter.get("description", "No description")
            if not name:
                return

            # Capture the body after the YAML frontmatter (subcommands, usage, examples).
            # This rich context is critical for the LLM to invoke the tool correctly.
            body_start = match.end()
            body = content[body_start:].strip()
            if body:
                desc = desc + "\n\n" + body

            tool_entry: Dict[str, Any] = {
                "name": name,
                "description": desc,
                "path": path,
                "type": "cli" if ".tool.md" in path else "skill",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "arguments": {
                            "type": "string",
                            "description": "Command line arguments or JSON string",
                        }
                    },
                },
            }

            # If command field is present in frontmatter, store it
            if "command" in frontmatter:
                tool_entry["command"] = frontmatter["command"]

            # Do NOT overwrite an already-indexed native CLI tool with the
            # same name; native binaries take priority over Python ports.
            if name in self.tools and self.tools[name].get("binary"):
                logger.info(
                    f"Skipping Python CLI '{name}' — native binary already indexed"
                )
                return

            self.tools[name] = tool_entry
        except Exception as e:
            logger.error(f"Failed to parse tool {path}: {e}")

    def _parse_mcp_tool(self, path: str):
        try:
            with open(path, "r", encoding="utf-8") as f:
                data = json.load(f)
            name = data.get("name")
            if name:
                self.tools[name] = data
                self.tools[name]["path"] = path
                self.tools[name]["type"] = "mcp"
        except Exception as e:
            logger.error(f"Failed to parse MCP tool {path}: {e}")

    def get_tool_schemas(self) -> List[Dict[str, Any]]:
        schemas = []
        for t in self.tools.values():
            schemas.append({
                "name": t["name"],
                "description": t["description"],
                "parameters": t.get("parameters", {})
            })
        return schemas

    def get_tool_metadata(self, name: str) -> Dict[str, Any]:
        return self.tools.get(name, {})
