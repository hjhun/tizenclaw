# Planning: Routing Guide Removal and E2E Tool Evaluation

## 1. Cognitive Requirements & Target Analysis
The AI agent operates via dynamic tool mappings. The `routing_guide.md` is to be entirely excised from the source tree to allow the agent uninhibited, native autonomous discovery. Instead of statically routing prompts, the agent must leverage the Tizen C-API bridging inherently.

## 2. Agent Capabilities Context
- Extraneous hardcoded instructions (`routing_guide.md`) are deleted. This frees context space and forces reliance on dynamic `<tool_descriptions>`.
- Testing evaluates absolute state stability across 10+ core device vectors sequentially.

## 3. Agent System Integration Planning
- **Module Convention:** `CMakeLists.txt` and `packaging/tizenclaw.spec` (removal of data file targets).
- **Execution Mode:** 1. File Deletion 2. E2E Agent Prompt Execution.
- **Environmental Context:** Testing must interpret failure modes across the embedded bounds asynchronously.
