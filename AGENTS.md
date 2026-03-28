# TizenClaw Main Development Workflow

This document defines the core development process (Plan → Design → Develop → Build & Deploy → Test & Review → Commit) for the TizenClaw project. The AGENT must always follow this process when performing tasks.

> [!IMPORTANT]
> For detailed procedures on each topic, refer to the workflow documents under [`.agents/workflows/`](.agents/workflows/).

```text
  ┌────────┐    ┌────────┐    ┌─────────┐    ┌────────────────┐    ┌───────────────┐    ┌────────┐
  │ 1.Plan │───▶│2.Design│───▶│3.Develop│───▶│4.Build & Deploy│───▶│5.Test & Review│───▶│6.Commit│
  └────────┘    └────────┘    └─────────┘    └────────────────┘    └───────────────┘    └────────┘
                                   ▲                                       │                  
                                   │            [FAIL]                     │                  
                                   └───────────────────────────────────────┘                  
                                             (Development Fix)                                
```

## 1. Plan
- Accurately understand the objectives and user requirements.
- Write a work unit (`task.md`) and establish a high-level plan before proceeding.

## 2. Design
- Analyze existing code and check applicable workflows to ensure the appropriate approach is selected.
- **CRITICAL**: The Agent MUST strictly adhere to the project's coding style as defined in [`.agents/workflows/coding_rules.md`](.agents/workflows/coding_rules.md) (e.g., Google C++ Style, 2-space indentation, trailing underscore `_` for members). Do not introduce or mimic inconsistent styles found in older legacy parts of the codebase.
- Establish architectural/structural decisions and write a detailed implementation plan (`implementation_plan.md`) if necessary.

## 3. Develop
- Modify source code and add/modify unit tests based on the design approach.
- **CRITICAL BRANCH POLICY**: Development must be conducted on the `develRust` branch, and related changes must be pushed only to the `develRust` branch unless explicitly commanded otherwise by the user. If development proceeds on a branch whose name starts with `devel` (e.g., `devel-feature`), changes must only be pushed to that specific branch. Do not create new branches or push to `main` without explicit permission.
- **WORKFLOW DOC POLICY**: Workflow documents (.md) must only be created or modified after the corresponding feature has been fully verified (build, deploy, and runtime validation) on an actual device. Writing workflow documents for unverified features is prohibited. When adding a new workflow, you must also update the workflow README.

## 4. Build & Deploy
- **NO LOCAL CMAKE/MAKE**: Because this project is developed for Tizen, you MUST NEVER run `cmake` or `make` directly in the WSL or shell environment. Build targeting the appropriate platform using the provided rules.
- After writing code, use the `deploy.sh` script to build, deploy, and restart the daemon via a single command.
  - Run: `./deploy.sh`
  - The script will automatically trigger a `gbs build`, locate the built rpm packages, install them on the device, and restart the `tizenclaw` service.
  - **IMPORTANT**: Always use `deploy.sh` as the primary tool. If there is no device (e.g., emulator) connected and you only need to build, you may use raw `gbs build` commands directly.
  - For advanced build options, refer to [`.agents/workflows/gbs_build.md`](.agents/workflows/gbs_build.md).
  - For deployment details, refer to [`.agents/workflows/deploy_to_emulator.md`](.agents/workflows/deploy_to_emulator.md).

## 5. Test & Review
Once `deploy.sh` successfully finishes, verify functionality and perform code review:
- Check the log output of the TizenClaw daemon to verify correct startup and runtime execution:
  - Command: `sdb shell dlogutil TIZENCLAW TIZENCLAW_WEBVIEW`
- **Functional Testing via `tizenclaw-cli`**:
  - Use the CLI to send natural language prompts to the daemon and verify new features work end-to-end.
  - Single-shot mode: `sdb shell tizenclaw-cli "your prompt here"`
  - With session ID: `sdb shell tizenclaw-cli -s <session_id> "your prompt here"`
  - Streaming: `sdb shell tizenclaw-cli --stream "your prompt here"`
  - Interactive mode: `sdb shell tizenclaw-cli` (type prompts, Ctrl+D to exit)
  - Example (workflow tools): `sdb shell tizenclaw-cli "Use the list_workflows tool to show the workflow list"`
  - For detailed CLI testing procedures, refer to [`.agents/workflows/cli_testing.md`](.agents/workflows/cli_testing.md).
- Verify the Web Dashboard is accessible:
  - Dashboard Port: `9090` (e.g., `http://<device-ip>:9090`)
  - **Web UI QA Testing**: You MUST use the `gstack` skill located in `.agents/skills/gstack` to perform headless browser QA testing, UI verification, and capture screenshots for the Web Dashboard. Before testing, read `.agents/skills/gstack/SKILL.md` for detailed usage instructions.
- If you need a more advanced component test, refer to [`.agents/workflows/gtest_integration.md`](.agents/workflows/gtest_integration.md).

> [!TIP]
> If a crash occurs after deployment, refer to [`.agents/workflows/crash_debug.md`](.agents/workflows/crash_debug.md) to analyze the crash dump.

After functional verification passes, perform a code review on all changed files using the [`.agents/workflows/code_review.md`](.agents/workflows/code_review.md) workflow checklist.
Key areas include Coding Style, Correctness, Memory, Performance, Logic, Security, Thread Safety, Resource Management, Test Coverage, and Error Handling.

### Issue Resolution Loop
- **PASS**: All items pass → proceed to Commit stage.
- **FAIL (Development Fix)**: Issues found during test/review → return to **Develop** stage to fix → Build & Deploy → Test & Review.
- **CONTINUOUS FAIL (Design Re-evaluation)**: If the same or related issues continuously occur during the Test & Review stage despite fixes, you MUST pause development and return to the **Design** stage to completely re-evaluate the technical approach and root cause.
- This Review-Fix loop repeats up to **5 times**. If issues remain after 5 iterations, escalate to the user to prevent an infinite loop.

## 6. Commit
When all review passes, perform a `git commit` to finalize the work according to [`.agents/workflows/commit_guidelines.md`](.agents/workflows/commit_guidelines.md).
Refer to the detailed rules in the respective workflow, but the core points are as follows.

### Basic Structure of a Commit Message
Write in the Conventional Commits style. **The commit message MUST be written in English.**

```text
Title (Under 50 chars, clear and concise English)

Provide a detailed explanation of the implemented features, bug fixes,
or structural changes. Describe 'Why' and 'What' was done extensively
but clearly. (Wrap text at 72 characters)
```

### Writing Example (Good)
```text
Switch from LXC to lightweight runc for ContainerEngine

Refactored the ContainerEngine implementation to use the lightweight
`runc` CLI via `std::system` instead of relying on `liblxc` APIs.
This change was necessary because the Tizen 10 GBS build environment
does not provide the `pkgconfig(lxc)` dependency.
```

### Prohibitions
- Mechanical text, such as Verification/Testing Results blocks, must **NEVER be included** in the commit message.
- Do not add unnecessary, verbose phrases generated by a bot.

### Commit Timing
**CRITICAL PROHIBITION**: You MUST NEVER execute a commit immediately after a build finishes. The system requires a strict 6-phase sequence. You must wait to verify the runtime behavior in the **Test & Review** phase. 

1. **Develop**: One unit feature is implemented.
2. **Build & Deploy**: `deploy.sh` executes successfully.
3. **Test & Review (MANDATORY)**: You actively run the feature on the device (using `tizenclaw-cli`, `dlogutil`, etc.) and ensure it works perfectly. You also perform the code review checklist.
4. **Commit**: Only after verifying the feature works in runtime, perform `git commit` formatted as above.

---

## Rust Migration

The project is actively migrating from C++ to Rust. The following rules apply during the migration:

### Migration Plan Tracking
- The migration plan lives at `.plan/migration_plan.md`.
- **After each phase or module is completed**, you MUST update `migration_plan.md` to reflect the current status (mark items with `[x]` and add completion dates).
- Keep the plan as the single source of truth for migration progress.

### Zero Warnings Build Policy (Rust)
- **All Rust code must compile with zero warnings.** This is a build-breaking policy — treat any `rustc` warning (unused imports, dead code, missing docs, etc.) as an error.
- `#![allow(unused)]` is **temporarily** enabled in `main.rs` during migration only. When a module is wired into the daemon and actively used, its unused items must be cleaned up.
- **TODO**: Remove `#![allow(unused)]` once all ported modules are integrated (Phase 6).

### Idiomatic Rust Style
- Leverage Rust's type system: use `enum` for discriminated unions, `Option`/`Result` for error handling, and trait objects for polymorphism.
- Prefer `impl Trait` over `Box<dyn Trait>` where possible.
- Use `?` operator for error propagation instead of nested `match` blocks.
- Use iterators and closures over explicit loops when they improve readability.
- Derive common traits (`Debug`, `Clone`, `Default`, `Serialize`, `Deserialize`) liberally.
- Follow Rust naming conventions: `snake_case` for functions/variables, `CamelCase` for types, `SCREAMING_SNAKE_CASE` for constants.

### Auto Build, Deploy & Test
When developing Rust modules, the AGENT must automatically perform the full cycle without waiting for user approval:
1. **Develop** — Write/modify Rust code in `src/` crates
2. **Build & Deploy** — Run `./deploy.sh` (includes GBS build + RPM install + daemon restart)
3. **Test** — Check daemon logs via `sdb shell journalctl -u tizenclaw -n 30`
4. **Review** — Verify no crashes, check for warnings in build log
5. **Fix & Repeat** — If build/test fails, fix and re-run `./deploy.sh` automatically

### Testing Config
- Development/testing configuration lives at `data/devel/` (contains API keys — **excluded from git**).
- Use `data/devel/llm_config.json` for testing LLM backends on the emulator.

---

## Skill Format Standard (Anthropic Standard)

TizenClaw skills follow the Anthropic standard skill format. Each skill is organized as a directory containing a `SKILL.md` file with YAML frontmatter.

### Skill Directory Structure
```
tools/skills/<skill_name>/     ← Project-level skills
.agents/skills/<skill_name>/   ← Agent core skills (e.g., gstack)
├── SKILL.md               ← Required: YAML frontmatter + Markdown documentation
├── <skill_name>.py        ← Entry point script (or .js / binary)
├── manifest.json          ← Optional: Legacy format (backward compatible)
├── scripts/               ← Optional: Helper scripts
├── examples/              ← Optional: Reference implementations
├── resources/             ← Optional: Additional assets
```

### SKILL.md Format
```markdown
---
name: skill_name
description: "What the skill does"
category: Device Info
risk_level: low
runtime: python
entry_point: skill_name.py
---

# Skill Title

Detailed documentation about the skill...

```json:parameters
{
  "type": "object",
  "properties": {},
  "required": []
}
```​
```

### Priority Rules
- If `SKILL.md` exists → use it (Anthropic standard)
- Else if `manifest.json` exists → use it (legacy fallback)
- Both can coexist; `SKILL.md` always takes priority

---

## Workflow Reference List
Detailed workflow files are located under [`.agents/workflows/`](.agents/workflows/).

| Workflow | File | Referenced Stage |
|---|---|---|
| Coding Rules | [`.agents/workflows/coding_rules.md`](.agents/workflows/coding_rules.md) | Plan |
| Code Review | [`.agents/workflows/code_review.md`](.agents/workflows/code_review.md) | Code Review |
| Commit Guidelines | [`.agents/workflows/commit_guidelines.md`](.agents/workflows/commit_guidelines.md) | Commit |
| GBS Build | [`.agents/workflows/gbs_build.md`](.agents/workflows/gbs_build.md) | Develop & Deploy |
| Deploy to Emulator | [`.agents/workflows/deploy_to_emulator.md`](.agents/workflows/deploy_to_emulator.md) | Develop & Deploy |
| GTest Unit Testing | [`.agents/workflows/gtest_integration.md`](.agents/workflows/gtest_integration.md) | Verify |
| CLI Functional Testing | [`.agents/workflows/cli_testing.md`](.agents/workflows/cli_testing.md) | Verify |
| Crash Dump Debugging | [`.agents/workflows/crash_debug.md`](.agents/workflows/crash_debug.md) | Verify |
| WSL Environment | [`.agents/workflows/wsl_environment.md`](.agents/workflows/wsl_environment.md) | Setup |
