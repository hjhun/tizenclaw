import os
import json
import logging
import asyncio
from enum import Enum
from typing import List, Dict, Any, Optional

logger = logging.getLogger(__name__)

class WorkflowStepType(Enum):
    PROMPT = "prompt"
    TOOL = "tool"

class WorkflowStep:
    def __init__(self, step_id: str):
        self.id = step_id
        self.description = ""
        self.type = WorkflowStepType.PROMPT
        self.instruction = ""
        self.tool_name = ""
        self.args: Dict[str, Any] = {}
        self.output_var = ""
        self.skip_on_failure = False
        self.max_retries = 0

class Workflow:
    def __init__(self, wf_id: str):
        self.id = wf_id
        self.name = ""
        self.description = ""
        self.trigger = "manual"
        self.steps: List[WorkflowStep] = []
        self.raw_markdown = ""

class WorkflowRunResult:
    def __init__(self, wf_id: str):
        self.workflow_id = wf_id
        self.status = "success"
        self.step_results: List[Dict[str, str]] = []
        self.variables: Dict[str, Any] = {}
        self.duration_ms = 0

class WorkflowEngine:
    """
    Python implementation of TizenClaw WorkflowEngine.
    Executes markdown-based sequences of LLM instructions and Tool calls async.
    """
    def __init__(self, agent=None):
        self.agent = agent
        self.workflows: Dict[str, Workflow] = {}
        self.workflows_lock = asyncio.Lock()
        self.data_dir = "/opt/usr/share/tizenclaw/workflows"
        os.makedirs(self.data_dir, exist_ok=True)

    async def load_workflows(self):
        # Load from self.data_dir matching *.md
        pass

    async def create_workflow(self, markdown: str) -> str:
        wf = self._parse_markdown(markdown)
        async with self.workflows_lock:
            self.workflows[wf.id] = wf
        self._save_workflow(wf)
        return wf.id

    async def run_workflow(self, workflow_id: str, input_vars: Dict[str, Any] = None) -> WorkflowRunResult:
        result = WorkflowRunResult(workflow_id)
        result.variables = input_vars or {}
        
        async with self.workflows_lock:
            if workflow_id not in self.workflows:
                result.status = "failed"
                return result
            wf = self.workflows[workflow_id]

        for step in wf.steps:
            try:
                # Interpolate variables first
                # Execute step via AgentCore loosely coupled
                step_res = await self._execute_step(step, result.variables)
                result.step_results.append({"step_id": step.id, "result": step_res})
            except Exception as e:
                logger.error(f"Workflow {workflow_id} step {step.id} failed: {e}")
                if not step.skip_on_failure:
                    result.status = "failed"
                    break

        return result

    async def _execute_step(self, step: WorkflowStep, variables: Dict[str, Any]) -> str:
        # Interpolate variables in instructions and args
        inst = step.instruction
        for k, v in variables.items():
            inst = inst.replace(f"{{{{{k}}}}}", str(v))
            
        if step.type == WorkflowStepType.PROMPT and self.agent:
            res = await self.agent.process_prompt("workflow_session", inst)
            if step.output_var:
                variables[step.output_var] = res
            return res
        elif step.type == WorkflowStepType.TOOL and self.agent and self.agent.dispatcher:
            # Interpolate dict args
            exec_args = {}
            for k, v in step.args.items():
                val_str = str(v)
                for vk, vv in variables.items():
                    val_str = val_str.replace(f"{{{{{vk}}}}}", str(vv))
                exec_args[k] = val_str
            res = await self.agent.dispatcher.execute_tool(step.tool_name, exec_args)
            if step.output_var:
                variables[step.output_var] = res
            return res
        return "Simulated success execution"

    def _parse_markdown(self, markdown: str) -> Workflow:
        import re, uuid
        wf = Workflow(str(uuid.uuid4())[:8])
        wf.raw_markdown = markdown
        
        # Super simple frontmatter/YAML parsing
        match = re.search(r"^---\n(.*?)\n---", markdown, re.MULTILINE | re.DOTALL)
        if match:
            for line in match.group(1).split("\n"):
                if ":" in line:
                    k, v = line.split(":", 1)
                    k, v = k.strip(), v.strip().strip("'\"")
                    if k == "id": wf.id = v
                    elif k == "name": wf.name = v
                    elif k == "description": wf.description = v

        # Extract steps from markdown ordered lists or headings
        lines = markdown.split("\n")
        in_step = False
        current_step = None
        for line in lines:
            if line.startswith("## Step"):
                if current_step: wf.steps.append(current_step)
                current_step = WorkflowStep(f"step_{len(wf.steps)}")
                in_step = True
            elif in_step and current_step:
                if line.startswith("Tool:"):
                    current_step.type = WorkflowStepType.TOOL
                    current_step.tool_name = line.split("Tool:")[1].strip()
                elif line.startswith("Output:"):
                    current_step.output_var = line.split("Output:")[1].strip()
                elif line.startswith("Prompt:"):
                    current_step.type = WorkflowStepType.PROMPT
                    current_step.instruction = line.split("Prompt:")[1].strip()
                elif line.strip() and not line.startswith("-"):
                    current_step.instruction += " " + line.strip()
                    
        if current_step: wf.steps.append(current_step)
        return wf

    def _save_workflow(self, wf: Workflow):
        path = os.path.join(self.data_dir, f"{wf.id}.md")
        try:
            with open(path, "w", encoding="utf-8") as f:
                f.write(wf.raw_markdown)
        except Exception as e:
            logger.error(f"Failed to save workflow {wf.id}: {e}")
