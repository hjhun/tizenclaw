# Agent Loop And System Prompt Review Planning

## Goal

Review TizenClaw's current `AgentLoop` and system prompt design against
three reference projects:

- OpenClaw
- NanoClaw
- Hermes Agent

The output of this cycle is a comparison report that identifies which
loop-control, prompt-layering, memory-injection, and sub-agent patterns
can be adopted safely in TizenClaw.

## Scope

This review is documentation-first work. No FFI, Tizen device API, or
daemon runtime behavior changes are planned in this cycle.

The review covers:

1. TizenClaw current implementation in `agent_core`, `agent_loop_state`,
   `prompt_builder`, `context_engine`, and `agent_roles`.
2. OpenClaw's documented agent loop, system prompt assembly, prompt modes,
   and hook points.
3. NanoClaw's host orchestrator loop, container runner loop, Claude Agent
   SDK usage, and `CLAUDE.md`-based context model.
4. Hermes Agent's cached system prompt strategy, loop budget/compression
   handling, plugin context injection, and tool-use enforcement.

## Planned Deliverables

1. An English planning note for workflow traceability.
2. An English design note defining the comparison axes and adoption
   filters.
3. A Korean review report summarizing findings and concrete
   TizenClaw adoption candidates.
4. `.dev_note/DASHBOARD.md` stage tracking entries for the full cycle.

## Execution Mode Classification

- Review artifact curation: One-shot Worker
- Comparison matrix design: One-shot Worker
- Adoption recommendation report: One-shot Worker

No Streaming Event Listener or Daemon Sub-task is introduced by this
cycle.

## Evaluation Questions

1. Which project separates stable prompt content from per-turn dynamic
   context most effectively?
2. Which loop design is best aligned with TizenClaw's embedded daemon
   constraints and multi-backend LLM architecture?
3. Which sub-agent and prompt-minimization patterns reduce token cost
   without weakening safety or observability?
4. Which ideas require minimal-risk documentation/runtime changes first,
   and which should be deferred until later refactors?
