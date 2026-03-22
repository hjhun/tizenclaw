import logging
import json
from typing import Dict, Any, Optional
from tizenclaw.core.tool_indexer import ToolIndexer
from tizenclaw.infra.container_engine import ContainerEngine

logger = logging.getLogger(__name__)

class ToolDispatcher:
    """
    Validates LLM tool calls against the ToolIndexer and executes them
    by dispatching to the secure ContainerEngine via abstract namespace IPC.
    """
    def __init__(self, indexer: ToolIndexer, container_engine: ContainerEngine):
        self.indexer = indexer
        self.container = container_engine

    async def execute_tool(self, name: str, args: Dict[str, Any]) -> str:
        metadata = self.indexer.get_tool_metadata(name)
        if not metadata:
            return f"Error: Tool '{name}' not found or not registered."

        tool_type = metadata.get("type", "cli")
        path = metadata.get("path", "")
        
        args_str = args.get("arguments", "")
        if isinstance(args_str, dict):
            args_str = json.dumps(args_str)

        logger.info(f"Dispatching tool '{name}' (Type: {tool_type})")

        try:
            if tool_type == "cli":
                return await self.container.execute_cli_tool(name, args_str)
            elif tool_type == "skill":
                return await self.container.execute_skill(path, args_str)
            elif tool_type == "mcp":
                return await self.container.execute_mcp_tool(name, args_str)
            else:
                return f"Error: Unknown tool type '{tool_type}'"
        except Exception as e:
            logger.error(f"Tool execution failed for '{name}': {e}")
            return f"Internal Execution Error: {e}"
