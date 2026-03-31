---
description: TizenClaw Development Overarching Management Document
---

# TizenClaw Development Agent Operation Rules

This document is the **top-level operational rule** representing the comprehensive management of the `tizenclaw` project's development process.
The agent (you) must strictly adhere to the procedures defined in this document to develop TizenClaw.

## Project Overview

**TizenClaw** is a Rust-based, high-performance Autonomous AI Agent daemon operating within the Embedded Linux (Tizen OS) environment.
The core goal is to build an ecosystem that operates autonomously, asynchronously, and flawlessly with optimal resource efficiency. The fundamental development principles are as follows:

- **TDD-Based Autonomous Agent Development**: All tests and validations are performed exclusively on the QEMU (emulator) or actual device environment via `./deploy.sh`. Testing a long-running async daemon involves verifying systemic stability and behavior precision. Local `cargo test` execution is **prohibited**.
- **Embedded Target Exclusive Build**: Unless requested otherwise, do not execute `cargo build` locally. All builds target the constrained Tizen device (emulator) via `./deploy.sh` (GBS Build), maximizing release performance optimizations.
- **Mandatory Multi-Architecture Build**: For robust systemic verification, builds for **both x86_64 and armv7l architectures** must mandatorily be performed.
- **Execution Centric**: Do not stop at just writing documents; physically execute the actual terminal commands (e.g., `./deploy.sh`, `git commit`).
- **Feedback Loops**: If an agent capability fails testing (e.g. memory leak, async panic), analyze the core embedded constraints, roll back to the previous stage, modify, and retry.

---

## Development Cycle (6 Stages)

All development tasks for the Autonomous Agent proceed **sequentially** through the 6 stages below.
No stages can be skipped, and the completion conditions of each stage must be met before moving onto the next.

```
1. Planning → [Supervisor] → 2. Design → [Supervisor] → 3. Development → [Supervisor] →
4. Build/Deploy → [Supervisor] → 5. Test/Review → [Supervisor] → 6. Commit → [Supervisor]
```

> [!IMPORTANT]
> **Complete Cycle Principle**: Once an agent capability or feature task is given by the user, the entire cycle from Planning (Stage 1) to Commit (Stage 6) must be flawlessly completed.
> Actions requiring approval during the development process are automatically granted (`SafeToAutoRun: true`).

---

## Roles and Skills per Stage

### Stage 1: Planning
- **Role**: Analyze requirements and define the scope of the autonomous agent behavior.
- **Activity**: Analyze Tizen Device API and daemon feature requirements to systematically organize the agent's perception, logic, and asynchronous action capabilities.
- **Artifact**: Save planning document in `.dev_note/01-planning/`
- **Skill Usage**: [`skills/planning-project/SKILL.md`](skills/planning-project/SKILL.md)

### Stage 2: Design
- **Role**: Design the optimized Rust architecture.
- **Activity**: Analyze Tizen C-API integration points and define robust async traits (Tokio), FFI boundaries, and zero-cost abstraction rules maximizing embedded performance. Tizen `.so` files rely on `libloading` based dynamic loading.
- **Artifact**: Save design document in `.dev_note/03-design/`
- **Skill Usage**: [`skills/designing-architecture/SKILL.md`](skills/designing-architecture/SKILL.md)

### Stage 3: Development
- **Role**: Write highly stable, memory-safe code under TDD principles.
- **Activity**: Develop state machines, continuous loops, and event-driven implementations. Validate memory safety and daemon code quality through the GBS build of `./deploy.sh`.
- **Artifact**: Save development note in `.dev_note/04-development/`
- **Skill Usage**: [`skills/developing-code/SKILL.md`](skills/developing-code/SKILL.md)

> [!CAUTION]
> **Local Build/Test Prohibition**: Do not execute `cargo build`, `cargo test`, `cargo clippy` locally.
> All builds and tests are to be performed in the QEMU/Device environment via `./deploy.sh`.

### Stage 4: Build & Deploy
- **Role**: Cross-compile release-optimized daemon and deploy to the Tizen Emulator/Device.
- **Activity**: Perform the GBS build and sdb deployment using the `./deploy.sh` script.
- **Prerequisite**: You **must perform builds across both x86_64 and armv7l architectures**.
- **Artifact**: Save build logs in `.dev_note/05-build-and-deploy/`
- **Skill Usage**: [`skills/building-deploying/SKILL.md`](skills/building-deploying/SKILL.md)

### Stage 5: Test & Review
- **Role**: Analyze autonomous E2E test results and review memory footprint, execution speed, and FFI safety.
- **Activity**: Run verification via `./deploy.sh --test` or `./deploy.sh --full-test` using daemon integration tests on the target.
- **Artifact**: Save the test results in `.dev_note/06-test-and-code-review/`
- **Skill Usage**: [`skills/reviewing-code/SKILL.md`](skills/reviewing-code/SKILL.md)

### Stage 6: Commit & Push
- **Role**: Record final deliverables to Git.
- **Activity**: Clean up unnecessary build artifacts, write a commit message adhering to upstream rules, and commit.
- **Artifact**: Save commit history in `.dev_note/07-commit-and-push/`
- **Skill Usage**: [`skills/managing-versions/SKILL.md`](skills/managing-versions/SKILL.md)

> [!WARNING]
> **Commit Rule**: Using `git commit -m "..."` is forbidden. Write the message inside `.dev_note/commit_msg.txt` first and execute `git commit -F .dev_note/commit_msg.txt`.

---

## Global Environment Management

> [!IMPORTANT]
> **Mandatory WSL Shell (Ubuntu) Usage**: Execute all terminal commands through the WSL shell (e.g., `wsl -e bash -c "..."`) as direct PowerShell executions are error-prone.

Follow the background limits and sequential execution commands in the environment skill carefully to avoid Samba/WSL lockups.
- **Skill Usage**: [`skills/managing-environment/SKILL.md`](skills/managing-environment/SKILL.md)

---

## Supervisor (Stage-Gate Validator)

The Supervisor is the **active stage-gate validator** that is invoked **after every stage completion** in the development cycle.
No stage transition occurs without the Supervisor's explicit PASS verdict.

- **Skill Usage**: [`skills/supervising-workflow/SKILL.md`](skills/supervising-workflow/SKILL.md)
- **Artifact**: Audit records are saved in `.dev_note/08-supervisor/`

### Updated Cycle Diagram (with Supervisor Gates)

```
1. Planning → [Supervisor Gate] → 2. Design → [Supervisor Gate] → 3. Development → [Supervisor Gate] →
4. Build/Deploy → [Supervisor Gate] → 5. Test/Review → [Supervisor Gate] → 6. Commit → [Supervisor Gate]
```

### Rollback Protocol

When the Supervisor detects a violation (e.g., missing artifacts, skipped architecture build, local `cargo` usage, inline `-m` commit):

1. The Supervisor writes a **Violation Report** (`violation-report-<stage>-<attempt>.md`) to `.dev_note/08-supervisor/` documenting which SKILL.md rule was broken.
2. Control is returned to the **violating stage's agent** with the violation report as corrective guidance.
3. The stage agent re-reads its SKILL.md, applies the corrective action, and re-executes.
4. The Supervisor re-validates upon stage completion.

> [!CAUTION]
> **Retry Limit**: A maximum of **3 retry attempts** per stage gate is allowed. If the violation persists after 3 attempts, the cycle is halted and escalated to the user for manual intervention.

---

## Operation Log (`.dev_note`)

All stages' thought processes and autonomous logic are recorded under the `.dev_note/` directory.

//turbo-all