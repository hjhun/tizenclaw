# Workflows

## Task: Address the remaining review findings for the PinchBench cycle

[x] Phase 0. Refine the current review-fix scope - `refining-requirements`
[x] Phase 1. Plan the remediation sequence and supervisor gates -
    `planning-project`
[x] Phase 2. Record the implementation-ready design for the remaining review
    findings - `designing-architecture`
[x] Phase 3. Patch the contradictory unit test and narrow the benchmark
    cleanup logic - `developing-code`
[x] Phase 4. Execute `./deploy_host.sh` in the foreground on the host path -
    `building-deploying`
[x] Phase 5. Review the resulting evidence, confirm the fixes, and update
    `.dev` state - `reviewing-code`
[x] Phase 6. Record the final evaluator verdict and residual risks -
    `evaluating-outcomes`

## Supervisor Gates

- Refine -> `.dev/REQUIREMENTS.md` reflects the current review findings and the
  existing `95.0%` score context from `.dev/SCORE.md`.
- Plan -> `.dev/WORKFLOWS.md`, `.dev/PLAN.md`, and `.dev/DASHBOARD.md` agree on
  the active review-fix scope and next action.
- Design -> implementation boundaries are explicit:
  - file completion still requires successful current-run file-management
    activity
  - output cleanup only removes prior result JSON files from the results
    directory
- Develop -> only the intended test, runner, and `.dev` files changed for this
  slice.
- Build/Deploy -> `./deploy_host.sh` completes in the foreground.
- Test/Review -> validation evidence confirms both review findings are fixed
  and `.dev` state is synchronized.
- Evaluate -> the final report states whether the review-fix slice is accepted
  and notes any residual risk.

## Notes

- Execution class: `host-default`
- Shell path: direct `bash`
- Auth mode: existing OpenAI OAuth only
- Model injection: `disabled`
- Validation gate: host scripted deploy for this review-fix slice
- Commit expectation: no commit work unless the user explicitly requests it
