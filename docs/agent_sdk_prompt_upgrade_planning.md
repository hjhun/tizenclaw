# Agent SDK Prompt Upgrade Planning

## Objective

Implement the next TizenClaw improvement cycle after the comparative
review. This cycle keeps the Claude Agent SDK adoption direction alive,
adds a practical compatibility path for OpenClaw-style skill hubs, and
refines prompt operation so backend-specific reasoning behavior no longer
depends on a single hard-coded `<think>/<final>` policy.

## Requested Scope

1. Preserve a Claude Agent SDK compatible direction rather than rejecting
   that integration path outright.
2. Make OpenClaw skill hubs reusable with minimal reshaping, ideally by
   mounting or registering an existing external skill root as-is.
3. Introduce prompt-mode control so the system prompt can run in
   `full` or `minimal` form.
4. Replace the unconditional `<think>/<final>` requirement with a
   backend-aware reasoning policy.
5. Verify the new behavior using `tizenclaw-cli` and the mandatory
   `./deploy.sh -a x86_64` cycle.

## Current Baseline

- TizenClaw already scans `workspace/skills/<name>/SKILL.md` and can
  register extra skill roots through `registered_paths.json`.
- The system prompt builder separates some dynamic context at runtime,
  but its reasoning section still forces `<think>` and `<final>` for all
  backends.
- `tizenclaw-cli` already exposes `register skill`, `list
  registrations`, and generic `config get/set/unset` commands over IPC.

## Planned Capabilities And Execution Modes

| Capability | Purpose | Execution Mode |
| --- | --- | --- |
| Prompt mode selection | Switch between full and minimal prompt layouts | One-shot Worker |
| Backend-aware reasoning policy | Tailor reasoning guidance to backend traits | One-shot Worker |
| Final text normalization | Strip `<think>` blocks when tags are absent | One-shot Worker |
| Skill hub intake path | Accept OpenClaw-style hub roots as-is | One-shot Worker |
| CLI-driven registration/config | Apply and validate runtime settings | One-shot Worker |

## Planning Notes

- Claude Agent SDK retention is treated as a future integration target,
  not a complete runtime swap in this cycle.
- OpenClaw compatibility should be additive and low-risk: prefer a hub
  mount point plus existing external path registration over a scanner
  rewrite.
- Prompt changes must preserve cache friendliness by keeping stable
  instructions in the system prompt and dynamic overlays in runtime
  messages.
