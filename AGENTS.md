# TizenClaw Main Development Workflow

This document defines the core development process (Plan → Design → Develop → Build & Deploy → Test & Review → Commit) for the TizenClaw project. The AGENT must always follow this process when performing tasks.

> [!IMPORTANT]
> For detailed procedures on each topic, refer to the workflow documents under [`.agents/workflows/`](.agents/workflows/).

## 1. Plan (기획)
- Accurately understand the objectives and user requirements.
- Write a work unit (`task.md`) and establish a high-level plan before proceeding.

## 2. Design (설계)
- Analyze existing code and check applicable workflows to ensure the appropriate approach is selected.
- **CRITICAL**: The Agent MUST strictly adhere to the project's coding style as defined in [`.agents/workflows/coding_rules.md`](.agents/workflows/coding_rules.md) (e.g., Google C++ Style, 2-space indentation, trailing underscore `_` for members). Do not introduce or mimic inconsistent styles found in older legacy parts of the codebase.
- Establish architectural/structural decisions and write a detailed implementation plan (`implementation_plan.md`) if necessary.

## 3. Develop (개발)
- Modify source code and add/modify unit tests based on the design approach.
- **CRITICAL BRANCH POLICY**: Do not create or switch to new branches for development or feature work. Always apply patches, make commits, and push changes directly to the **current branch** you are currently on. Maintain this single-branch development policy at all times.
- **WORKFLOW DOC POLICY**: Workflow documents (.md) must only be created or modified after the corresponding feature has been fully verified (build, deploy, and runtime validation) on an actual device. Writing workflow documents for unverified features is prohibited. When adding a new workflow, you must also update the workflow README.

## 4. Build & Deploy (빌드 및 배포)
- After writing code, use the `deploy.sh` script to build, deploy, and restart the daemon via a single command.
  - Run: `./deploy.sh`
  - The script will automatically trigger a `gbs build`, locate the built rpm packages, install them on the device, and restart the `tizenclaw` service.
  - **IMPORTANT**: Do NOT run raw `gbs build` commands directly. Always use `deploy.sh` for build and deployment. Raw GBS commands should only be executed when explicitly requested by the user.
  - For advanced build options, refer to [`.agents/workflows/gbs_build.md`](.agents/workflows/gbs_build.md).
  - For deployment details, refer to [`.agents/workflows/deploy_to_emulator.md`](.agents/workflows/deploy_to_emulator.md).

## 5. Test & Review (테스트 및 리뷰)
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
- **FAIL (Development Fix)**: Issues found during test/review → return to **Develop (개발)** stage to fix → Build & Deploy → Test & Review.
- **CONTINUOUS FAIL (Design Re-evaluation)**: If the same or related issues continuously occur during the Test & Review stage despite fixes, you MUST pause development and return to the **Design (설계)** stage to completely re-evaluate the technical approach and root cause.
- This Review-Fix loop repeats up to **5 times**. If issues remain after 5 iterations, escalate to the user to prevent an infinite loop.

## 6. Commit (커밋)
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
1. One unit feature specified in the document is implemented.
2. `gbs build` (including `%check` gtests internally) passes without errors.
3. Perform `git commit` formatted as above after `git add .`.

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
