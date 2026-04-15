# WORKFLOWS

## Task Classification

- Cycle type: runtime-layout live Tizen rerun and closure attempt
- Primary source: `.dev/REQUIREMENTS.md`
- Default shell assumption: direct Linux `bash`
- Active scope: close the remaining reviewer gap in `deploy.sh --test`, rerun
  a real non-dry-run Tizen deployment, inspect live runtime-layout behavior,
  and record the final verdict
- `testing-with-tizenclaw-tests`: not required beyond the scripted
  `./deploy.sh --test` validation for this deployment-focused cycle

## Active Stage Sequence

```text
refine -> plan -> design -> develop -> build/deploy -> test/review -> evaluate
```

## Completion Status

[O] Stage 0. Refine completed through `.dev/REQUIREMENTS.md`
[O] Stage 1. Plan completed and synchronized across `.dev/WORKFLOWS.md`,
  `.dev/PLAN.md`, and `.dev/DASHBOARD.md`
[O] Stage 2. Design completed
[O] Stage 3. Develop completed
[O] Stage 4. Build/Deploy completed
[O] Stage 5. Test/Review completed
[O] Stage 6. Evaluate completed

## Locked Planning Decisions

### Validation Target Policy

- Use direct `bash` and the reachable emulator target `emulator-26101`.
- Use the real non-dry-run `./deploy.sh -d emulator-26101` path for the
  authoritative Tizen rerun.
- Avoid destructive full-target reset unless the evidence remains ambiguous
  after the scripted sanitizer and fresh deploy pass.

### Scripted Validation Policy

- Fix the `./deploy.sh --test` sanitizer check so it asserts packaged owner and
  group normalization against the intended deployed defaults of `root:root`.
- Keep `./deploy.sh --test` as the required scripted gate before the live
  deployment rerun.

### Evidence Policy

- Identity evidence must come from target-side service metadata and the running
  process table.
- Mutable-state evidence must include fresh timestamps or freshly touched files
  under `/home/owner/.tizenclaw` after the deployment window.
- Packaged-asset evidence must include file ownership/mode plus an actual write
  probe or equivalent target-side proof for `/opt/usr/share/tizenclaw`.

### Verdict Policy

- The runtime-layout verdict can upgrade only if identity, mutable-state, and
  packaged-asset immutability all pass on fresh target-side evidence.
- If packaged assets are still writable in practice, the verdict remains
  blocked even when the scripted review gate passes.

## Stage Plan

### Stage 0. Refine
Status:
- Complete

Output:
- `.dev/REQUIREMENTS.md`

### Stage 1. Plan
Status:
- Complete

Outputs:
- `.dev/WORKFLOWS.md`
- `.dev/PLAN.md`
- `.dev/DASHBOARD.md`

### Stage 2. Design
Status:
- Complete

Recorded outcome:
- define the exact deploy and inspection command set for the live rerun, with
  enough freshness checks to distinguish new behavior from historical residue
- use `systemctl show`, `ps`, `find`, `stat`, and an `owner` write probe on
  the target as the authoritative evidence set

### Stage 3. Develop
Status:
- Complete

Recorded outcome:
- `deploy.sh --test` asserts the intended packaged ownership normalization in
  addition to the existing mode checks
- the ownership assertion runs under `fakeroot` so the scripted gate remains
  non-interactive on the host

### Stage 4. Build/Deploy
Status:
- Complete

Recorded outcome:
- run `./deploy.sh --test`
- run the real non-dry-run `./deploy.sh -d emulator-26101`
- `./deploy.sh --test` passed
- `./deploy.sh -d emulator-26101` completed successfully after `pkgcmd`
  returned `Operation not allowed [-4]` and the script correctly fell back to
  `rpm -Uvh`

### Stage 5. Test/Review
Status:
- Complete

Recorded outcome:
- capture target-side identity, mutable-state, and packaged-asset evidence
- determine whether the live runtime-layout contract is now fully satisfied
- identity passed: `systemctl show` and `ps` both proved `owner:users`
- mutable state passed: fresh files under `/home/owner/.tizenclaw` were created
  at `2026-04-15 21:03:51` through `21:04:01` KST after the service restart
- packaged assets passed in practice: `/opt/usr/share/tizenclaw` is
  `root:root`, the expected modes are restored, no non-root ownership remained,
  and an `owner` write probe failed with `Permission denied`

### Stage 6. Evaluate
Status:
- Complete

Recorded outcome:
- `.dev/DASHBOARD.md` and a new evaluator report record the final rerun result
  and state whether the runtime-layout verdict upgrades or remains blocked
- the runtime-layout verdict upgrades because all three live target checks now
  pass on the rerun
