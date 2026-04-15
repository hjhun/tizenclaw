# WORKFLOWS

## Task Classification

- Scope: live Tizen runtime-layout validation
- Cycle type: explicit deploy-and-verify closure run
- Primary source: `.dev/REQUIREMENTS.md`
- Required starting gate: `refine -> plan`
- Deployment policy for this cycle: execute the real non-dry-run
  `./deploy.sh` path against a reachable Tizen target

## Active Stage Sequence

```text
refine -> plan -> design -> build/deploy -> test/review -> evaluate
```

## Planning Decisions

### 1. Design Authority

- Use the existing runtime-layout design record in
  `.dev/02-architect/20260415_tizenclaw_dir.md` as the design authority for
  this cycle.
- The packaged-asset standard is runtime-context mutation denial under
  `/opt/usr/share/tizenclaw`, not a requirement for a read-only filesystem
  mount at the kernel level.

### 2. Validation Authority

- Earlier evaluator files remain preserved history only.
- The newest live-validation report produced by this cycle becomes the
  authoritative runtime-layout closure statement.

### 3. Evidence Freshness Rule

- Use evidence collected during this execution cycle.
- Distinguish fresh runtime-state observations from stale residue by using the
  current deployment window and unique probe names.

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

Authority:
- `.dev/02-architect/20260415_tizenclaw_dir.md`

Recorded outcome:
- confirmed that the live-validation cycle still follows the previously
  published runtime-layout design and acceptance model

### Stage 3. Build/Deploy

Status:
- Complete

Recorded outcome:
- executed `PATH="$HOME/tizen-studio/tools:$PATH" ./deploy.sh -d emulator-26101`
- `pkgcmd` returned `Operation not allowed [-4]`, then the script fell back to
  `rpm -Uvh`
- the install transaction was proven by the updated installed package state:
  `tizenclaw 1.0.0-3.x86_64 1776260997`
- the service restart completed at `2026-04-15 22:50:01 KST`

### Stage 4. Test/Review

Status:
- Complete

Recorded outcome:
- `systemctl show` and `ps` confirmed `User=owner`, `Group=users`, and the live
  process `/usr/bin/tizenclaw`
- fresh files were written under `/home/owner/.tizenclaw` from
  `2026-04-15 22:50:01 KST` onward, including `memory/memory.md`,
  `logs/tizenclaw.log`, `state/loop/scheduler_health.json`, `tools/tools.md`,
  and `actions/index.md`
- `/opt/usr/share/tizenclaw` remained `root:root`, no non-root entries were
  found, and the runtime-user write probe failed with `Permission denied`

### Stage 5. Evaluate

Status:
- Complete

Recorded outcome:
- published a new authoritative live-validation evaluator report for this
  execution cycle
