---
description: TizenClaw Development Overarching Management Document
---

This root copy exists so tools that auto-detect `AGENTS.md` from the
repository root can load the project rules. The canonical source remains
`.agent/rules/AGENTS.md`.

# TizenClaw Development Agent Operation Rules

This document is the **top-level operational rule** representing the
comprehensive management of the `tizenclaw` project's development process.
The agent (you) must strictly adhere to the procedures defined in this
document to develop TizenClaw.

> [!IMPORTANT]
> **Language Rule**: You must plan and report in the same language as the
> user's input. (If asked in English, use English; if asked in Korean,
> use Korean / 사용자의 입력과 같은 언어로 plan과 보고를 합니다.
> 영어로 질의하면 영어로, 한국어로 질의하면 한국어로 답합니다).

## Project Overview

**TizenClaw** is a Rust-based, high-performance Autonomous AI Agent daemon
operating within the Embedded Linux (Tizen OS) environment.
The core goal is to build an ecosystem that operates autonomously,
asynchronously, and flawlessly with optimal resource efficiency.
The fundamental development principles are as follows:

- **TDD-Based Autonomous Agent Development**: All tests and validations
  are performed exclusively on the QEMU (emulator) or actual device
  environment via `./deploy.sh`. Testing a long-running async daemon
  involves verifying systemic stability and behavior precision.
  Local `cargo test` execution is **prohibited**.
- **Embedded Target Exclusive Build**: Unless requested otherwise,
  do not execute `cargo build` locally. All builds target the constrained
  Tizen device (emulator) via `./deploy.sh` (GBS Build), maximizing
  release performance optimizations.
- **Single Architecture Focus (x86_64)**: For rapid development and
  verification, builds for **x86_64 architecture** are mandatory.
- **[DISABLED] armv7l Multi-Architecture Build**: armv7l (ARM)
  architecture verification is currently disabled to prioritize execution
  speed but can be re-enabled for final cross-architecture validation
  if required.
- **Execution Centric**: Do not stop at just writing documents;
  physically execute the actual terminal commands (e.g., `./deploy.sh`,
  `git commit`).
- **Feedback Loops**: If an agent capability fails testing
  (e.g. memory leak, async panic), analyze the core embedded constraints,
  roll back to the previous stage, modify, and retry.

---

## Development Cycle (6 Stages)

All development tasks for the Autonomous Agent **MUST strictly proceed
through the 6 stages below sequentially**.
**Skipping stages or autonomously jumping between skill agents is strictly
prohibited.** Development must follow a rigid, step-by-step process.

```
1. Planning → [Supervisor] → 2. Design → [Supervisor] → 3. Development → [Supervisor] →
4. Build/Deploy → [Supervisor] → 5. Test/Review → [Supervisor] → 6. Commit → [Supervisor]
```

> [!IMPORTANT]
> **Strict Cycle Principle**: The coding agent **MUST strictly adhere to
> all 6 development stages sequentially** without skipping.
> Flexible execution or skipping stages is strictly forbidden.
> **Actions requiring approval** during the development process are
> automatically granted (`SafeToAutoRun: true`).

---

## Roles and Skills per Stage

### Stage 1: Planning
- **Role**: Analyze requirements and define the scope of the autonomous
  agent behavior.
- **Activity**: Analyze Tizen Device API and daemon feature requirements
  to systematically organize the agent's perception, logic, and
  asynchronous action capabilities.
- **Artifact**: Update stage status in `.dev_note/DASHBOARD.md`
- **Skill Usage**:
  [`.agent/skills/planning-project/SKILL.md`](.agent/skills/planning-project/SKILL.md)

### Stage 2: Design
- **Role**: Design the optimized Rust architecture.
- **Activity**: Analyze Tizen C-API integration points and define robust
  async traits (Tokio), FFI boundaries, and zero-cost abstraction rules
  maximizing embedded performance. Tizen `.so` files rely on `libloading`
  based dynamic loading.
- **Artifact**: Update stage status in `.dev_note/DASHBOARD.md`
- **Skill Usage**:
  [`.agent/skills/designing-architecture/SKILL.md`](.agent/skills/designing-architecture/SKILL.md)

### Stage 3: Development
- **Role**: Write highly stable, memory-safe code under TDD principles.
- **Activity**: Develop state machines, continuous loops, and event-driven
  implementations. Validate memory safety and daemon code quality through
  the GBS build of `./deploy.sh`.
- **Artifact**: Update stage status in `.dev_note/DASHBOARD.md`
- **Skill Usage**:
  [`.agent/skills/developing-code/SKILL.md`](.agent/skills/developing-code/SKILL.md)

> [!CAUTION]
> **Local Build/Test Prohibition**: Do not execute `cargo build`,
> `cargo test`, `cargo check`, `cargo clippy` locally.
> All builds, checks, and tests are to be performed in the QEMU/Device
> environment via `./deploy.sh`. **Running `cargo check` locally is
> strictly forbidden.**
> **Untested Code is Garbage**: A developer must *never* consider a task
> done simply by writing code. Running a build, deploying it, and testing
> its execution behavior using `./deploy.sh` is an imperative baseline.
> Code not proven by device testing must be rejected.

- **Prerequisite**: You **must perform builds across both x86_64 and
  armv7l architectures**.
- **[DISABLED]**: armv7l (ARM) architecture verification is currently
  disabled to prioritize execution speed but can be re-enabled if
  required.
- **Artifact**: Update stage status in `.dev_note/DASHBOARD.md`
- **Skill Usage**:
  [`.agent/skills/building-deploying/SKILL.md`](.agent/skills/building-deploying/SKILL.md)

### Stage 5: Test & Review
- **Role**: Analyze autonomous E2E test results and review memory
  footprint, execution speed, and FFI safety.
- **Activity**: Run verification via `./deploy.sh --test` or
  `./deploy.sh --full-test` using daemon integration tests on the target.
- **Artifact**: Update stage status in `.dev_note/DASHBOARD.md`
- **Skill Usage**:
  [`.agent/skills/reviewing-code/SKILL.md`](.agent/skills/reviewing-code/SKILL.md)

### Stage 6: Commit & Push
- **Role**: Record final deliverables to Git.
- **Activity**: Clean up unnecessary build artifacts, write a commit
  message adhering to upstream rules, and commit.
- **Artifact**: Update stage status in `.dev_note/DASHBOARD.md`
- **Skill Usage**:
  [`.agent/skills/managing-versions/SKILL.md`](.agent/skills/managing-versions/SKILL.md)

> [!WARNING]
> **Commit Rule**: Using `git commit -m "..."` is forbidden. Write the
> message inside `.tmp/commit_msg.txt` first and execute
> `git commit -F .tmp/commit_msg.txt`.

> [!CAUTION]
> **Mandatory Skill Enforcement**: ALL commit and push operations
> **MUST** use the
> `.agent/skills/managing-versions/SKILL.md` skill without exception.
> Directly running `git commit` or `git push` outside of this skill is a
> critical protocol violation subject to immediate Supervisor rollback.

> [!IMPORTANT]
> **Commit Message Rules** (enforced by Supervisor):
> 1. **Language**: All commit messages must be written in **English**.
> 2. **Line Length**: No single line may exceed **80 characters**.
> 3. **Format**: Summarized content style.
>    ```
>    <Title: concise imperative sentence, ≤50 chars>
>
>    <Body: Summarize the purpose and specific changes in clear prose
>    or bullet points. Do NOT use explicit 'Why:' or 'What:' headers.
>    Each line ≤80 chars.>
>    ```
> 4. Titles must clearly convey the intent; bodies must provide a
>    comprehensive summary of the modifications.

---

## Global Environment Management

> [!IMPORTANT]
> **Mandatory WSL Shell (Ubuntu) Usage**: Execute all terminal commands
> through the WSL shell (e.g., `wsl -e bash -c "..."`) as direct
> PowerShell executions are error-prone.

> [!IMPORTANT]
> **Shell Detection Rule**: Before executing ANY command, follow the
> [`.agent/rules/shell-detection.md`](.agent/rules/shell-detection.md)
> decision matrix to determine the correct shell (PowerShell vs WSL Bash).
> This rule is authoritative for all shell decisions.

Follow the background limits and sequential execution commands in the
environment skill carefully to avoid Samba/WSL lockups.
- **Rule Reference**:
  [`.agent/rules/shell-detection.md`](.agent/rules/shell-detection.md)
- **Skill Usage**:
  [`.agent/skills/managing-environment/SKILL.md`](.agent/skills/managing-environment/SKILL.md)

---

## Supervisor (Stage-Gate Validator)

The Supervisor is the **active stage-gate validator** that is invoked
**after every stage completion** in the development cycle.
No stage transition occurs without the Supervisor's explicit PASS verdict.

- **Skill Usage**:
  [`.agent/skills/supervising-workflow/SKILL.md`](.agent/skills/supervising-workflow/SKILL.md)
- **Artifact**: Audit records are logged in `.dev_note/DASHBOARD.md`

### Updated Cycle Diagram (with Supervisor Gates)

```
1. Planning → [Supervisor Gate] → 2. Design → [Supervisor Gate] → 3. Development → [Supervisor Gate] →
4. Build/Deploy → [Supervisor Gate] → 5. Test/Review → [Supervisor Gate] → 6. Commit → [Supervisor Gate]
```

### Rollback Protocol

When the Supervisor detects a violation (e.g., missing artifacts, skipped
architecture build, local `cargo` or `cargo check` usage, inline `-m`
commit):

1. The Supervisor writes a **Violation Record** inside
   `.dev_note/DASHBOARD.md` documenting which SKILL.md rule was broken.
2. Control is returned to the **violating stage's agent** with the
   violation report as corrective guidance.
3. The stage agent re-reads its SKILL.md, applies the corrective action,
   and re-executes.
4. The Supervisor re-validates upon stage completion.

> [!CAUTION]
> **Retry Limit**: A maximum of **3 retry attempts** per stage gate is
> allowed. If the violation persists after 3 attempts, the cycle is halted
> and escalated to the user for manual intervention.

---

## Operation Log (`.dev_note`)

All stages' thought processes and autonomous logic are recorded
exclusively in the single `.dev_note/DASHBOARD.md` tracking file.
Keep the dashboard concise.

## Documentation Location

All development-process documents created during Planning, Design,
Review, or similar stage work MUST be created under `.dev_note/docs/`.
Do not create new workflow or stage artifact documents under `docs/`.

//turbo-all
