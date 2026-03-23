"""
TizenClaw Multi-Agent System.

Provides:
  - AgentRegistry: Register/manage multiple specialized agents
  - SupervisorEngine: Route prompts to appropriate agents, aggregate results
  - A2A (Agent-to-Agent) protocol: Inter-agent message passing
"""
import asyncio
import json
import logging
import time
from typing import Dict, List, Any, Optional

logger = logging.getLogger(__name__)


class AgentRole:
    """Defines a role for an agent in the multi-agent system."""
    def __init__(self, name: str, description: str, system_prompt: str = "",
                 tools_filter: Optional[List[str]] = None):
        self.name = name
        self.description = description
        self.system_prompt = system_prompt
        self.tools_filter = tools_filter  # None = all tools


class AgentInstance:
    """A running agent instance with its own personality and capabilities."""
    def __init__(self, agent_id: str, role: AgentRole, core=None):
        self.agent_id = agent_id
        self.role = role
        self.core = core  # Reference to AgentCore
        self.status = "idle"
        self.last_active = time.time()
        self.task_count = 0

    async def process(self, session_id: str, prompt: str) -> str:
        if not self.core:
            return f"[Agent {self.agent_id}] No agent core assigned"
        self.status = "busy"
        self.last_active = time.time()
        try:
            result = await self.core.process_prompt(session_id, prompt)
            self.task_count += 1
            return result
        finally:
            self.status = "idle"


class AgentRegistry:
    """Registry for managing multiple agent instances."""

    def __init__(self):
        self._agents: Dict[str, AgentInstance] = {}
        self._lock = asyncio.Lock()

    async def register(self, agent: AgentInstance):
        async with self._lock:
            self._agents[agent.agent_id] = agent
            logger.info(f"Registered agent: {agent.agent_id} ({agent.role.name})")

    async def unregister(self, agent_id: str):
        async with self._lock:
            if agent_id in self._agents:
                del self._agents[agent_id]
                logger.info(f"Unregistered agent: {agent_id}")

    async def get(self, agent_id: str) -> Optional[AgentInstance]:
        return self._agents.get(agent_id)

    async def list_all(self) -> List[Dict[str, Any]]:
        return [
            {
                "agent_id": a.agent_id,
                "role": a.role.name,
                "description": a.role.description,
                "status": a.status,
                "task_count": a.task_count,
                "last_active": a.last_active,
            }
            for a in self._agents.values()
        ]

    async def find_by_role(self, role_name: str) -> Optional[AgentInstance]:
        for a in self._agents.values():
            if a.role.name == role_name and a.status == "idle":
                return a
        # If all busy, return first match
        for a in self._agents.values():
            if a.role.name == role_name:
                return a
        return None


class A2AMessage:
    """Agent-to-Agent message."""
    def __init__(self, sender: str, receiver: str, content: str,
                 msg_type: str = "request"):
        self.sender = sender
        self.receiver = receiver
        self.content = content
        self.msg_type = msg_type  # request, response, broadcast
        self.timestamp = time.time()
        self.id = f"{sender}_{receiver}_{int(self.timestamp * 1000)}"


class A2AProtocol:
    """Agent-to-Agent communication protocol."""

    def __init__(self, registry: AgentRegistry):
        self.registry = registry
        self._message_queue: asyncio.Queue = asyncio.Queue()
        self._handlers: Dict[str, List] = {}

    async def send(self, message: A2AMessage) -> Optional[str]:
        """Send a message to another agent and wait for response."""
        target = await self.registry.get(message.receiver)
        if not target:
            logger.warning(f"A2A: target agent {message.receiver} not found")
            return None

        logger.info(f"A2A: {message.sender} → {message.receiver}: {message.content[:80]}")
        result = await target.process(
            f"a2a_{message.sender}",
            f"[From agent '{message.sender}']: {message.content}"
        )
        return result

    async def broadcast(self, sender: str, content: str) -> Dict[str, str]:
        """Broadcast to all agents (except sender)."""
        results = {}
        agents = await self.registry.list_all()
        for a in agents:
            if a["agent_id"] != sender:
                msg = A2AMessage(sender, a["agent_id"], content, "broadcast")
                resp = await self.send(msg)
                if resp:
                    results[a["agent_id"]] = resp
        return results


# Default agent roles
DEFAULT_ROLES = {
    "assistant": AgentRole(
        name="assistant",
        description="General purpose conversational assistant",
        system_prompt="You are TizenClaw, a helpful AI assistant for Tizen devices. "
                      "Answer user questions naturally and accurately."
    ),
    "tool_specialist": AgentRole(
        name="tool_specialist",
        description="Specialist in executing device tools and system operations",
        system_prompt="You are a Tizen device tool specialist. When the user asks about "
                      "device status, settings, or operations, use the appropriate tools "
                      "to get real data. Always prefer tool calls over generic answers."
    ),
    "researcher": AgentRole(
        name="researcher",
        description="Information gathering and web search specialist",
        system_prompt="You are a research specialist. Use web search and knowledge base "
                      "tools to find accurate, up-to-date information."
    ),
    "codegen": AgentRole(
        name="codegen",
        description="Code generation and web app creation specialist",
        system_prompt="You are a code generation specialist for Tizen. Generate high-quality "
                      "Python, JavaScript, and HTML code. Use generate_web_app when asked "
                      "to create web applications."
    ),
}


class SupervisorEngine:
    """
    Supervisor that routes user requests to the most appropriate agent.
    Implements the 'run_supervisor' embedded tool.
    """

    def __init__(self, registry: AgentRegistry, a2a: A2AProtocol):
        self.registry = registry
        self.a2a = a2a
        self._routing_rules = {
            # Keywords → role mapping
            "device": "tool_specialist",
            "battery": "tool_specialist",
            "wifi": "tool_specialist",
            "bluetooth": "tool_specialist",
            "app": "tool_specialist",
            "install": "tool_specialist",
            "brightness": "tool_specialist",
            "volume": "tool_specialist",
            "sensor": "tool_specialist",
            "vconf": "tool_specialist",
            "search": "researcher",
            "find": "researcher",
            "research": "researcher",
            "code": "codegen",
            "generate": "codegen",
            "create": "codegen",
            "web app": "codegen",
            "webapp": "codegen",
        }

    def _classify_intent(self, prompt: str) -> str:
        """Classify user intent to determine which agent should handle it."""
        lower = prompt.lower()
        for keyword, role in self._routing_rules.items():
            if keyword in lower:
                return role
        return "assistant"  # Default

    async def process(self, session_id: str, prompt: str) -> str:
        """Route prompt to the best agent."""
        role_name = self._classify_intent(prompt)
        agent = await self.registry.find_by_role(role_name)

        if not agent:
            # Fallback to assistant
            agent = await self.registry.find_by_role("assistant")

        if not agent:
            return "No agents available to handle this request."

        logger.info(f"Supervisor: routing to {agent.agent_id} (role={agent.role.name})")
        return await agent.process(session_id, prompt)

    async def multi_agent_task(self, session_id: str, prompt: str,
                                agents: List[str]) -> Dict[str, str]:
        """Execute a task across multiple agents in parallel."""
        tasks = []
        for agent_id in agents:
            agent = await self.registry.get(agent_id)
            if agent:
                tasks.append((agent_id, agent.process(session_id, prompt)))

        results = {}
        for agent_id, coro in tasks:
            try:
                results[agent_id] = await asyncio.wait_for(coro, timeout=120)
            except asyncio.TimeoutError:
                results[agent_id] = f"[Timeout] Agent {agent_id} did not respond"
            except Exception as e:
                results[agent_id] = f"[Error] {e}"

        return results

    def get_status(self) -> Dict[str, Any]:
        return {
            "routing_rules_count": len(self._routing_rules),
            "default_role": "assistant",
        }
