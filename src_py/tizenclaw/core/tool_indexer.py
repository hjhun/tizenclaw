import os
import re
import json
import logging
from typing import List, Dict, Any

logger = logging.getLogger(__name__)

class ToolIndexer:
    """
    Parses .tool.md, .skill.md and MCP tools from the filesystem.
    Converts their YAML frontmatter into LLM JSON Schema parameters.
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
        if not os.path.exists(d):
            return
        
        for root, _, files in os.walk(d):
            for file in files:
                path = os.path.join(root, file)
                if file.endswith(".tool.md") or file.endswith(".skill.md"):
                    self._parse_markdown_tool(path)
                elif file.endswith(".mcp.json"):
                    self._parse_mcp_tool(path)

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

            # Note: For full implementations, we'd recursively parse properties.
            # Here we fake a catch-all 'arguments' input string if not detailed.
            self.tools[name] = {
                "name": name,
                "description": desc,
                "path": path,
                "type": "cli" if ".tool.md" in path else "skill",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "arguments": {
                            "type": "string",
                            "description": "Command line arguments or JSON string"
                        }
                    }
                }
            }
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
