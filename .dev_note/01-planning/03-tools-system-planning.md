# 03-tools-system-planning.md (Tool System Overhaul)

## Planning Progress (Autonomous Core):
- [x] Step 1: Analyze project cognitive requirements and map them to Embedded Tizen System APIs
- [x] Step 2: List persistent agent daemon states, logic models, and fallback capabilities (docs/)
- [x] Step 3: Draft Rust workspace module integration objectives and subsystem logic paths

## Step 1: Cognitive Requirements & Target Analysis
The TizenClaw tools ecosystem must align with the Anthropic MCP (Model Context Protocol) and Skills standard, eliminating the fragmented 'custom_skill' logic. The ActionBridge needs to flexibly parse arbitrary Tizen Action Schema versions (v1, v2) dynamically reading as raw JSON without struct-based panics. Additionally, as tools/skills are updated by the system in `/opt/usr/share/tizen-tools/`, the agent must dynamically inject updated tool structures (Markdown) into the LLM context (the region is standard RW, enabling real-time prompt updates).

## Step 2: Agent Capabilities Listing and Resource Context

### Capability 1: Dynamic Action Schema Parsing (ActionBridge)
- **Goal:** Allow the `ActionBridge` to ingest any Tizen Action schema (v1 or v2) safely by parsing abstract JSON trees, passing them natively to Anthropic LLM tools.
- **Input:** JSON files generated in the Tizen Action Framework directory.
- **Output:** Unified `LlmToolDecl` structures.
- **Power Mitigation:** By parsing raw `Value` directly via `serde_json` without intensive reflection or schema mismatches, parsing overhead is minimized locally on device memory.

### Capability 2: Live Tool Registry Injection
- **Goal:** Periodically or event-driven updates of the Agent's systemic LLM prompts reflecting changes inside `/opt/usr/share/tizen-tools/`. Instead of only generating static markdown files, the Agent dynamically reads available skills/indexes and injects them as active System Prompts.
- **Input:** File changes in RW `/opt/usr/share/tizen-tools/` and its subdirectories.
- **Output:** Updated internal LLM System Context for the current Active Session.
- **Power Mitigation:** Avoids continuous disk polling where possible, utilizing standard prompt-state variables inside memory, reloading context only upon requested context bounds.

### Capability 3: Tool Catalog Clean-up (`custom_skill` Removal)
- **Goal:** Remove redundant `custom_skill` roles and mappings. All AI-generated tools run uniformly mapped inside `/opt/usr/share/tizen-tools/skills`.
- **Input:** Clean workspace deletion rules applied over existing `tizenclaw_secure_container.sh` and `agent_roles.json`.
- **Output:** Simplified single-source directory bindings.
- **Power Mitigation:** Prevents duplicate container mounting ops, speeding up the Tizenclaw daemon context initialization by processing single `skills/` instead of multiple abstract trees.

## Step 3: Agent System Integration Planning

### Mandatory Execution Mode Classification:
1. **Dynamic Action Schema Parsing (ActionBridge):** `Daemon Sub-task` (Initializes async memory map, periodically syncs missing endpoints).
2. **Live Tool Registry Injection:** `One-shot Worker` (Updates memory cache of System Prompt strings dynamically prior to LLM submission).
3. **Tool Catalog Clean-up:** Conceptual Refactoring (No new execution thread, just initialization adjustments).

### Module Convention
- Modifications target `tizenclaw-core` specifically `ActionBridge` and prompt build logic (`tizenclaw/src/core/prompt_builder.rs` / `context_engine.rs`) and system configuration files.

## Completion Status
Planning stage artifacts fully completed tracking unified `skills` integration, flexible Action Schemas parsing, and runtime prompt injections.
