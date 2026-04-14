---
description: TizenClaw Development Agent Operation Rules
---

# TizenClaw Development Agent Operation Rules

This file mirrors `.agent/rules/AGENTS.md` so root-level auto-detection can
load the repository workflow rules.

## Purpose

TizenClaw is a Rust-based autonomous agent daemon that is developed on host
Linux by default and optionally validated on Tizen when the user explicitly
requests that path.

This bundle has three layers:

- reusable workflow templates under `.agent/workflows/`
- reusable local skills under `.agent/skills/`
- repository rules under `.agent/rules/`

Use `supervising-workflow` whenever the next workflow is not obvious.

## Language Rule

- Interactive planning, progress updates, and final reports must match the
  user's language.
- Stable repository artifacts may remain in English unless the user requests a
  different language.

## TizenClaw Core Rules

- Host-first by default: use `./deploy_host.sh` unless the user explicitly
  requests Tizen, emulator, or device validation.
- Script-first by default: do not substitute raw `cargo build`, `cargo test`,
  `cargo check`, `cargo clippy`, or ad-hoc `cmake` commands for the normal
  development cycle.
- Explicit Tizen override: use `./deploy.sh` only when the user asks for a
  Tizen packaging or deployment cycle.
- Keep the default rapid-development focus on `x86_64`.
- Run commands synchronously in the foreground. Do not use `nohup`, `&`, or
  detached subprocesses.

## Environment Entry Rule

Before executing repository commands, follow
`.agent/rules/shell-detection.md` and use
`.agent/skills/managing-environment/SKILL.md`.

The normal case in this repository is a WSL Ubuntu `bash` shell that should
run commands directly without a `wsl.exe` wrapper.

## Workflow Entry Points

Start with the workflow that matches the task:

- `.agent/workflows/refine-plan.md`
- `.agent/workflows/planning-design.md`
- `.agent/workflows/develop-system-tests.md`
- `.agent/workflows/build-deploy-review.md`
- `.agent/workflows/cleanup-commit.md`

## Pipeline Overview

All non-trivial tasks begin with requirement refinement. Planning then
generates the task-specific `.dev/WORKFLOWS.md` that drives the rest of the
cycle.

```text
refine -> plan -> design -> develop -> build/deploy -> test/review
-> commit -> evaluate
             ^          ^                ^
      supervisor gates between phases; optional mid-pipeline evaluator checks
```

`testing-with-tizenclaw-tests` is part of development and validation whenever
daemon-visible behavior changes.

## Stage Outputs

### Stage 0. Refine
- Skill: `.agent/skills/refining-requirements/SKILL.md`
- Output: `.dev/REQUIREMENTS.md`

### Stage 1. Plan
- Skill: `.agent/skills/planning-project/SKILL.md`
- Outputs:
  - `.dev/WORKFLOWS.md`
  - `.dev/PLAN.md`
  - `.dev/DASHBOARD.md`

### Stage 2. Design
- Skill: `.agent/skills/designing-architecture/SKILL.md`
- Outputs:
  - design notes under `.dev/docs/` when needed
  - refreshed `.dev/DASHBOARD.md`

### Stage 3. Develop
- Skills:
  - `.agent/skills/developing-code/SKILL.md`
  - `.agent/skills/testing-with-tizenclaw-tests/SKILL.md` when applicable
- Outputs:
  - scoped implementation changes
  - any required `tests/system/` scenario updates
  - refreshed `.dev/DASHBOARD.md`

### Stage 4. Build/Deploy
- Skill: `.agent/skills/building-deploying/SKILL.md`
- Output:
  - scripted host or Tizen build/deploy evidence

### Stage 5. Test/Review
- Skills:
  - `.agent/skills/reviewing-code/SKILL.md`
  - `.agent/skills/testing-with-tizenclaw-tests/SKILL.md` when applicable
- Output:
  - executed validation results
  - findings and residual risks in `.dev/DASHBOARD.md`

### Stage 6. Commit
- Skill: `.agent/skills/managing-versions/SKILL.md`
- Output:
  - `.tmp/commit_msg.txt`
  - final commit

### Stage 7. Evaluate
- Skill: `.agent/skills/evaluating-outcomes/SKILL.md`
- Output:
  - reports under `.dev/07-evaluator/`

The final evaluator stage is mandatory. Planning may also insert additional
mid-pipeline evaluator checkpoints for high-risk or ambiguous work.

## Supervisor Gate Rules

The supervisor validates each stage transition before work advances:

- refine -> requirements are explicit
- plan -> workflow, plan, and dashboard exist
- design -> implementation no longer needs invented structure
- develop -> intended files changed and state is synchronized
- build/deploy -> scripted path executed
- test/review -> executed evidence recorded
- commit -> diff scope and commit format are correct
- evaluate -> final assessment is recorded with an explicit verdict

If a gate fails:

1. Record the reason in `.dev/DASHBOARD.md`
2. Route back to the earliest incomplete stage
3. Re-run the corrected stage

Escalate to the user after three failed retries on the same gate.

## Commit Rules

- Write the message to `.tmp/commit_msg.txt`
- Use `git commit -F .tmp/commit_msg.txt`
- Never use `git commit -m`
- Use English for the commit message
- Keep the title at 50 characters or fewer
- Keep every body line at 80 characters or fewer
- Push only when the user explicitly requests it

## `.dev` State Rules

- Keep `.dev/DASHBOARD.md` concise and current
- Keep `.dev/PLAN.md` as a prompt-derived checklist
- Keep `.dev/WORKFLOWS.md` as the task-specific process map
- Put new design or review documents under `.dev/docs/`
- Preserve historical `.dev` records unless the user explicitly asks to clean
  them up
