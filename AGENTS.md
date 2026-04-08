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
that supports both host Linux development and Tizen deployment.
The core goal is to build an ecosystem that operates autonomously,
asynchronously, and flawlessly with optimal resource efficiency.
The fundamental development principles are as follows:

- **Host-First Development Default**: Unless the user explicitly asks for
  Tizen/emulator/device validation, perform development through
  `./deploy_host.sh` on Ubuntu/WSL.
- **Script-First Build/Test Rule**: Do not invoke `cargo build`,
  `cargo test`, `cargo check`, or `cargo clippy` directly for ordinary
  development work. Use `./deploy_host.sh` for the host path and
  `./deploy.sh` only when the user explicitly requests the Tizen path.
- **Explicit Tizen Override**: When the user asks for device packaging,
  emulator deployment, or Tizen validation, switch to `./deploy.sh`
  (GBS build / deploy flow) for that cycle.
- **Single Architecture Focus (x86_64)**: For rapid development and
  verification, the default focus remains **x86_64**.
- **Execution Centric**: Do not stop at just writing documents;
  physically execute the actual terminal commands (e.g.,
  `./deploy_host.sh`, `./deploy.sh`, `git commit`).
- **Feedback Loops**: If an agent capability fails testing
  (e.g. memory leak, async panic), analyze the active host/Tizen
  constraints, roll back to the previous stage, modify, and retry.

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
- **Activity**: Analyze whether the cycle should use the default
  host workflow (`./deploy_host.sh`) or an explicitly requested Tizen
  workflow (`./deploy.sh`), then organize the required daemon behavior.
- **Artifact**: Update stage status in `.dev_note/DASHBOARD.md`
- **Skill Usage**:
  [`.agent/skills/planning-project/SKILL.md`](.agent/skills/planning-project/SKILL.md)

### Stage 2: Design
- **Role**: Design the optimized Rust architecture.
- **Activity**: Define the Rust architecture, host/Tizen execution
  boundaries, async traits (Tokio), FFI boundaries, and zero-cost
  abstraction rules that fit the requested environment.
- **Artifact**: Update stage status in `.dev_note/DASHBOARD.md`
- **Skill Usage**:
  [`.agent/skills/designing-architecture/SKILL.md`](.agent/skills/designing-architecture/SKILL.md)

### Stage 3: Development
- **Role**: Write highly stable, memory-safe code under TDD principles.
- **Activity**: Develop state machines, continuous loops, and event-driven
  implementations. Validate code quality through `./deploy_host.sh` by
  default, and use `./deploy.sh` only when the user explicitly requests
  Tizen/emulator/device validation.
- **Artifact**: Update stage status in `.dev_note/DASHBOARD.md`
- **Skill Usage**:
  [`.agent/skills/developing-code/SKILL.md`](.agent/skills/developing-code/SKILL.md)

> [!CAUTION]
> **Direct Cargo/CMake Prohibition**: Do not execute `cargo build`,
> `cargo test`, `cargo check`, `cargo clippy`, or ad-hoc `cmake` commands
> directly for ordinary development work.
> Use `./deploy_host.sh` for default host builds/tests and `./deploy.sh`
> only for explicit Tizen/emulator/device cycles.
> **Untested Code is Garbage**: A developer must *never* consider a task
> done simply by writing code. Running the appropriate script-driven
> build/test path is the baseline requirement.

### Stage 4: Build & Deploy
- **Role**: Execute the build/install/deploy path that matches the active
  cycle.
- **Activity**: Run `./deploy_host.sh` by default for host install/restart
  validation, or `./deploy.sh` when the user explicitly requests Tizen
  packaging/deployment.
- **Artifact**: Update stage status in `.dev_note/DASHBOARD.md`
- **Skill Usage**:
  [`.agent/skills/building-deploying/SKILL.md`](.agent/skills/building-deploying/SKILL.md)

### Stage 5: Test & Review
- **Role**: Analyze autonomous E2E test results and review memory
  footprint, execution speed, and FFI safety.
- **Activity**: Run verification via `./deploy_host.sh --test` by default,
  and use `./deploy.sh --test` / device-specific validation only when the
  user explicitly requests a Tizen cycle.
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
> PowerShell executions are error-prone. The default project command is
> `./deploy_host.sh`; `./deploy.sh` is reserved for explicit Tizen cycles.

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

When the Supervisor detects a violation (e.g., missing artifacts, using
the wrong execution script for the cycle, direct `cargo` / `cmake`
usage, inline `-m` commit):

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
