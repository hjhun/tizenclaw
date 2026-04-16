# WORKFLOWS

## Workflow Policy

- Request class: `full_workflow`
- Refinement mode: `normalize`
- Execution mode: host-default
- Required phases: `refine`, `plan`, `design`, `develop`, `build_deploy`,
  `test_review`, `commit`, `evaluate`
- Skipped phases: none
- Additional evaluator checkpoints: none

## Authoritative Stage Sequence

### Phase 0. Refine

- [O] Outcome: requirements, scope boundaries, constraints, and acceptance
  criteria are explicit.
- Inputs: task prompt, roadmap goal, repository rules.
- Outputs: `.dev/REQUIREMENTS.md`
- Completion signal: refine output is actionable without blocking questions.
- Status: complete

### Phase 1. Plan

- [O] Outcome: downstream execution order and validation gates are fixed for this
  roadmap slice.
- Inputs: `.dev/REQUIREMENTS.md`, `AGENTS.md`, `.dev/workflow_state.json`,
  current `.dev` state.
- Outputs: `.dev/WORKFLOWS.md`, `.dev/PLAN.md`, `.dev/TASKS.md`,
  `.dev/DASHBOARD.md`
- Completion signal: the next agent can start design without guessing scope,
  order, or validation expectations.
- Status: complete

### Phase 2. Design

- [O] Outcome: implementation-critical design choices are fixed for provider
  routing, Telegram config precedence, ClawHub update semantics, and snapshot
  cache ownership and invalidation.
- Inputs: `.dev/REQUIREMENTS.md`, runtime configuration and admin surface
  expectations, current module boundaries.
- Outputs: design notes under `.dev/docs/` when needed, refreshed
  `.dev/DASHBOARD.md`
- Completion signal: no implementation-critical ambiguity remains for routing,
  configuration precedence, update flow behavior, or cache invalidation.
- Status: complete

### Phase 3. Develop

- [O] Outcome: the selected design is implemented across runtime, config,
  ClawHub, cache, and CLI surfaces.
- Inputs: approved design decisions and prompt-derived task queue.
- Outputs: scoped code changes, required `tests/system/` updates,
  refreshed `.dev/DASHBOARD.md`
- Completion signal: intended files are updated coherently and daemon-visible
  behavior is aligned with the new routing, update, and caching paths.
- Required validation support: include `testing-with-tizenclaw-tests`
  coverage because daemon-visible behavior changes are in scope.
- Status: complete

### Phase 4. Build/Deploy

- [O] Outcome: host build and deploy sanity is validated through the scripted
  repository path.
- Inputs: completed implementation state.
- Outputs: host build or deploy evidence from `./deploy_host.sh`
- Completion signal: `./deploy_host.sh` succeeds for the modified scope.
- Status: complete

### Phase 5. Test/Review

- [O] Outcome: routing, Telegram config, ClawHub update, and snapshot cache
  behavior are validated and reviewed for regressions.
- Inputs: buildable implementation and new or updated tests.
- Outputs: executed validation evidence, review findings, refreshed
  `.dev/DASHBOARD.md`
- Completion signal: `./deploy_host.sh --test` succeeds and any residual gaps
  are recorded explicitly.
- Required validation support: include `testing-with-tizenclaw-tests`
  coverage where daemon-visible behavior changed.
- Status: complete

### Phase 6. Commit

- [O] Outcome: the change set is packaged for version control without pushing.
- Inputs: validated implementation and recorded residual risks.
- Outputs: `.tmp/commit_msg.txt`, final commit when requested
- Completion signal: diff scope matches the approved plan and commit message
  rules from `AGENTS.md` are satisfied.
- Status: complete

### Phase 7. Evaluate

- [O] Outcome: the final assessment records verdict, coverage, and residual risk.
- Inputs: post-validated repository state.
- Outputs: evaluator report under `.dev/07-evaluator/`
- Completion signal: an evaluator report records an explicit final verdict.
- Status: complete

## Phase Gates

- `refine -> plan`: requirements are explicit and actionable.
- `plan -> design`: workflow, plan, tasks, and dashboard are synchronized.
- `design -> develop`: no implementation-critical ambiguity remains.
- `develop -> build_deploy`: intended files changed and state is synchronized.
- `build_deploy -> test_review`: scripted host build or deploy path executed.
- `test_review -> commit`: executed validation evidence is recorded.
- `commit -> evaluate`: diff scope and commit format are correct.

## Validation Notes

- Use the host-first, script-first path only.
- `./deploy_host.sh` is the normal build or deploy gate.
- `./deploy_host.sh --test` is the regression and review gate.
- Do not substitute ad hoc raw `cargo` commands for the scripted cycle.
