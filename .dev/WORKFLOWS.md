# WORKFLOWS

## Workflow Class

- Request class: `full_workflow`
- Refinement mode: `normalize`
- Host strategy: host-first, script-first, foreground `bash`

## Stage Sequence

```text
refine -> plan -> design -> develop -> build/deploy -> test/review
-> commit -> evaluate
```

## Phase Completion Status

- [O] Stage 0. Refine — DONE
- [O] Stage 1. Plan — DONE
- [O] Stage 2. Design — DONE
- [O] Stage 3. Develop — DONE (rework pass 5: priority ordering + circuit-breaker status fixed)
- [O] Stage 4. Build/Deploy — DONE (`./deploy_host.sh -b` PASS)
- [O] Stage 5. Test/Review — DONE (`./deploy_host.sh --test` PASS: 597; 0 failed)
- [O] Stage 6. Commit — DONE (ce70f4b4)
- [O] Stage 7. Evaluate — DONE (see .dev/07-evaluator/20260416-tizenclaw-improve.md)

## Stage Contracts

### Stage 0. Refine

- Input: `.dev/REQUIREMENTS.md`
- Outcome: the roadmap targets, scope boundaries, validation path, risks, and
  open questions stay explicit enough for downstream work.
- Gate to continue: requirements remain explicit and usable.

### Stage 1. Plan

- Outputs: `.dev/WORKFLOWS.md`, `.dev/PLAN.md`, `.dev/TASKS.md`,
  `.dev/DASHBOARD.md`
- Outcome: downstream agents can follow the exact stage sequence without
  guessing skipped work, validation paths, or deliverables.
- Gate to continue: planning artifacts exist and match the current workflow
  policy.

### Stage 2. Design

- Inputs: `.dev/REQUIREMENTS.md`, existing design note
  `.dev/docs/runtime_flexibility_ooad_design_20260416.md`
- Outcome: resolve provider-routing policy, Telegram config precedence,
  ClawHub update semantics, and snapshot-cache ownership before coding.
- Gate to continue: implementation-critical ambiguity is removed or explicitly
  documented in `.dev/docs/` and `.dev/DASHBOARD.md`.

### Stage 3. Develop

- Skills: developing-code, testing-with-tizenclaw-tests
- Outcome: implement provider routing, Telegram model-list configuration,
  ClawHub update flow, snapshot caching, status output, and targeted tests.
- Gate to continue: intended files are updated and the runtime state stays
  synchronized with the plan.

### Stage 4. Build/Deploy

- Command path: `./deploy_host.sh`
- Outcome: scripted host build evidence is captured for the changed scope.
- Gate to continue: the scripted host path was executed and the result is
  recorded.

### Stage 5. Test/Review

- Command path: `./deploy_host.sh --test`
- Skills: reviewing-code, testing-with-tizenclaw-tests
- Outcome: validate routing, Telegram config loading, ClawHub update handling,
  snapshot cache invalidation, and operator-facing status visibility.
- Gate to continue: regression evidence and residual risks are recorded.

### Stage 6. Commit

- Commit contract: write `.tmp/commit_msg.txt`, then use
  `git commit -F .tmp/commit_msg.txt`
- Outcome: package only the approved diff scope.
- Gate to continue: commit scope and message format satisfy `AGENTS.md`.

### Stage 7. Evaluate

- Output: `.dev/07-evaluator/` report with an explicit verdict
- Outcome: final assessment of whether the roadmap slice met its intent and
  what follow-up remains.
- Gate to continue: evaluator report exists and the dashboard reflects the
  final verdict.

## Phase-Specific Focus

- Provider routing must move from one primary backend plus static fallbacks to
  a provider-selection layer with compatibility handling for legacy config.
- Telegram model choices must move out of Rust source and into operator-managed
  configuration with a documented precedence rule.
- ClawHub update must reuse `workspace/.clawhub/lock.json` and keep install
  safety guarantees intact.
- Skill snapshot caching must avoid redundant rescans and invalidate safely on
  root, registration, and capability-config changes.
- Validation evidence must come from the scripted host paths only.

## Skipped Phases

- None. `workflow_policy.skipped_phases` is empty for this `full_workflow`
  request.
