# TizenClaw Python Port — Development Workflow

This document defines the core development process (Plan → Develop → Verify → Review → Commit) for the TizenClaw Python port (`develPython` branch). The AGENT must always follow this process when performing tasks.

> [!IMPORTANT]
> This branch is a **full Python 3 rewrite** of TizenClaw. The `main` and `devel` branches contain the original C++20 implementation. For detailed procedures on each topic, refer to the workflow documents under [`.agents/workflows/`](.agents/workflows/).

## 1. Plan
- Accurately understand the objectives and requirements.
- Analyze existing Python code under `src_py/` and check applicable workflows.
- **CRITICAL**: The Agent MUST follow Python coding conventions:
  - **PEP 8** style (4-space indentation, snake_case functions/variables, PascalCase classes)
  - **Type hints** for function signatures
  - **Docstrings** for classes and public methods
  - Module layout follows the existing `src_py/tizenclaw/` package structure
- **CRITICAL BRANCH POLICY**: Do not create or switch to new branches for development or feature work. Always apply patches, make commits, and push changes directly to the **current branch** you are currently on. Maintain this single-branch development policy at all times.
- **WORKFLOW DOC POLICY**: Workflow documents (.md) must only be created or modified after the corresponding feature has been fully verified (build, deploy, and runtime validation) on an actual device. Writing workflow documents for unverified features is prohibited. When adding a new workflow, you must also update the workflow README.
- Write a work unit (`task.md`) and establish a detailed plan before implementation.

## 2. Develop & Deploy
- Modify Python source code in `src_py/` and add/modify tests.
- The Python source tree:
  ```
  src_py/
  ├── tizenclaw_daemon.py          # Main daemon (asyncio IPC + MCP stdio)
  ├── tizenclaw_cli.py             # CLI client tool
  ├── tizenclaw_tool_executor.py   # Socket-activated tool executor
  ├── tizenclaw_code_sandbox.py    # Socket-activated code sandbox
  └── tizenclaw/                   # Core Python package
      ├── core/                    # AgentCore, ToolIndexer, ToolDispatcher, WorkflowEngine
      ├── llm/                     # LlmBackend ABC, OpenAiBackend
      ├── infra/                   # ContainerEngine, TizenSystemEventAdapter
      ├── storage/                 # SessionStore, MemoryStore, EmbeddingStore
      ├── embedding/               # OnDeviceEmbedding (ONNX Runtime)
      ├── scheduler/               # TaskScheduler (asyncio-based)
      └── utils/                   # TizenDlogHandler, NativeWrapper (ctypes)
  ```
- After writing code, use the `deploy.sh` script to build, deploy, and restart the daemon via a single command.
  - Run: `./deploy.sh`
  - The script will automatically trigger a `gbs build`, locate the built RPM packages, install them on the device, and restart the `tizenclaw` service.
  - **IMPORTANT**: Do NOT run raw `gbs build` commands directly. Always use `deploy.sh` for build and deployment. Raw GBS commands should only be executed when explicitly requested by the user.
  - For advanced build options, refer to [`.agents/workflows/gbs_build.md`](.agents/workflows/gbs_build.md).
  - For deployment details, refer to [`.agents/workflows/deploy_to_emulator.md`](.agents/workflows/deploy_to_emulator.md).

### Key Architecture Notes (Python Port)
- **No C++ compilation**: `CMakeLists.txt` uses `LANGUAGES NONE` — it only installs Python scripts and systemd units.
- **Installed paths**: Python packages go to `/opt/usr/share/tizenclaw-python/`, executables to `/usr/bin/`.
- **Zero external dependencies**: The daemon uses only Python stdlib (`asyncio`, `json`, `urllib.request`, `sqlite3`, `ctypes`, `struct`).
- **LLM Backend**: Currently only `OpenAiBackend` using `urllib.request` with `asyncio.to_thread`.
- **IPC**: JSON-RPC 2.0 over abstract Unix Domain Sockets with 4-byte network-endian length prefix.

## 3. Verify
Once `deploy.sh` successfully finishes:
- Check the log output of the TizenClaw daemon to verify correct startup and runtime execution:
  - Command: `sdb shell dlogutil TIZENCLAW TIZENCLAW_WEBVIEW`
- **Functional Testing via `tizenclaw-cli`**:
  - Use the CLI to send natural language prompts to the daemon and verify new features work end-to-end.
  - Single-shot mode: `sdb shell tizenclaw-cli "your prompt here"`
  - With session ID: `sdb shell tizenclaw-cli -s <session_id> "your prompt here"`
  - Streaming: `sdb shell tizenclaw-cli --stream "your prompt here"`
  - Interactive mode: `sdb shell tizenclaw-cli` (type prompts, Ctrl+D to exit)
  - For detailed CLI testing procedures, refer to [`.agents/workflows/cli_testing.md`](.agents/workflows/cli_testing.md).
- Verify the Web Dashboard is accessible:
  - Dashboard Port: `9090` (e.g., `http://<device-ip>:9090`)
- For shell-based verification suites, see `tests/verification/run_all.sh` (28 test scripts).

> [!TIP]
> If a crash occurs after deployment, refer to [`.agents/workflows/crash_debug.md`](.agents/workflows/crash_debug.md) to analyze the crash dump.

## 4. Code Review
After verification passes, perform a code review on all changed files using the [`.agents/workflows/code_review.md`](.agents/workflows/code_review.md) workflow checklist:
1. **Coding Style** — PEP 8 compliance, type hints, docstrings
2. **Correctness** — logic errors, boundary conditions, missing error handling
3. **Resource Management** — unclosed sockets/files, asyncio task cleanup
4. **Performance** — unnecessary blocking in async context, O(n) lookups
5. **Logic Issues** — dead code, unreachable branches, variable shadowing
6. **Security** — missing input validation, injection vulnerabilities
7. **Concurrency** — asyncio lock usage, race conditions, event loop safety
8. **Error Propagation & Logging** — dlog handler usage, silent failure prevention
9. **Test Coverage** — verification test scripts for new features
10. **Python-specific** — proper `await` usage, exception handling, `asyncio.Lock` patterns

### Review-Fix Loop (max 5 iterations)
- **PASS**: All items pass → proceed to Commit stage
- **FAIL**: Issues found → return to **Develop** stage to fix → `deploy.sh` → **Verify** → re-**Review**
- This loop repeats up to **5 times**. If exceeded, escalate to the user.

> [!CAUTION]
> If the Review-Fix loop exceeds 5 iterations, you must report to the user to prevent an infinite loop.

## 5. Commit (Completion of Work)
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
Add hybrid RAG search to EmbeddingStore

Implemented Reciprocal Rank Fusion (RRF) combining FTS5 keyword
search with vector cosine similarity in the Python EmbeddingStore.
This brings parity with the C++ implementation's hybrid search
capability while maintaining zero external dependencies.
```

### Prohibitions
- Mechanical text, such as Verification/Testing Results blocks, must **NEVER be included** in the commit message.
- Do not add unnecessary, verbose phrases generated by a bot.

### Commit Timing
1. One unit feature specified in the document is implemented.
2. `gbs build` (via `deploy.sh`) passes without errors.
3. Perform `git commit` formatted as above after `git add .`.

---

## Skill Format Standard (Anthropic Standard)

TizenClaw skills follow the Anthropic standard skill format. Each skill is organized as a directory containing a `SKILL.md` file with YAML frontmatter.

### Skill Directory Structure
```
tools/skills/<skill_name>/
├── SKILL.md               ← Required: YAML frontmatter + Markdown documentation
├── <skill_name>.py        ← Entry point script
├── scripts/               ← Optional: Helper scripts
├── examples/              ← Optional: Reference implementations
├── resources/             ← Optional: Additional assets
```

### SKILL.md Format
```markdown
---
name: skill_name
description: "What the skill does"
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
| CLI Functional Testing | [`.agents/workflows/cli_testing.md`](.agents/workflows/cli_testing.md) | Verify |
| Crash Dump Debugging | [`.agents/workflows/crash_debug.md`](.agents/workflows/crash_debug.md) | Verify |
| WSL Environment | [`.agents/workflows/wsl_environment.md`](.agents/workflows/wsl_environment.md) | Setup |
