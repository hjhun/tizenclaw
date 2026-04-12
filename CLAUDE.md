# TizenClaw — Claude Code Project Rules

This file is the entrypoint for Claude-oriented repository guidance.
The durable project instructions now live in `.claude/CLAUDE.md`.
Read that file first, then use the sections below as the repository-level
workflow guardrails.

> **Language Rule**: Always respond in the same language as the user's input.
> Korean input → Korean response. English input → English response.

---

## Project Overview

**TizenClaw** is a Rust-based Autonomous AI Agent daemon for Tizen OS
(embedded Linux) and Ubuntu/WSL host development. The default workflow
uses `./deploy_host.sh`; the Tizen GBS workflow uses `./deploy.sh` when
explicitly requested. The repository is currently split across:

- the canonical reconstruction workspace under `rust/`
- the repository support tools under `src/` and
  `tests/test_porting_workspace.py`
- the still-active legacy Rust implementation under `src/tizenclaw*`

**Active branch**: `develRust`  
**Target device**: `emulator-26101` (x86_64) — auto-detected via `sdb`

---

## Absolute Rules (Never Violate)

### Build & Test
- **NEVER** run `cargo build`, `cargo check`, `cargo test`, or
  `cargo clippy` directly. Default development builds/tests must go
  through `./deploy_host.sh`.
- **NEVER** run `cmake .` or any local CMake build.
- Use `./deploy.sh` only when the user explicitly asks for Tizen,
  emulator, or device validation.
- Architecture focus: **x86_64 only**.

### Commits
- **NEVER** use `git commit -m "..."`. Write the message to
  `.tmp/commit_msg.txt` first, then run `git commit -F .tmp/commit_msg.txt`.
- Commit messages must be in **English**.
- Title: ≤ 50 characters, imperative sentence, capitalized.
- Body: each line ≤ 80 characters. No `feat:`, `fix:` prefixes. No explicit
  `Why:` / `What:` headers.
- Push target: `git push origin develRust`.

### Temporary Files
- Use `.tmp/` (project root) for all temporary files — not `/tmp/`.
- Delete temp files when no longer needed.
- `.tmp/` is in `.gitignore` and must never be committed.

### Local Build Cleanup
If a local build is accidentally triggered, immediately clean up:
```bash
rm -rf target/
rm -f CMakeCache.txt Makefile cmake_install.cmake
rm -rf CMakeFiles/ build_local/
find ./src -name 'CMakeFiles' -type d -exec rm -rf {} + 2>/dev/null
find ./src \( -name '*.o' -o -name '*.d' \) -delete 2>/dev/null
```

---

## Development Cycle (6 Stages)

All tasks must follow these stages **sequentially**. Skipping is forbidden.

```
1. Planning → [Supervisor Gate] → 2. Design → [Supervisor Gate] →
3. Development → [Supervisor Gate] → 4. Build/Deploy → [Supervisor Gate] →
5. Test/Review → [Supervisor Gate] → 6. Commit → [Supervisor Gate]
```

Each stage has a corresponding skill in `.agent/skills/`. After each stage,
update `.dev/DASHBOARD.md` with the stage status.

| Stage | Skill(s) | Key Output |
|-------|----------|------------|
| 1. Planning | `.agent/skills/planning-project/SKILL.md` | Module objectives, execution mode classification |
| 2. Design | `.agent/skills/designing-architecture/SKILL.md` | FFI boundaries, async topology docs |
| 3. Development | `.agent/skills/developing-code/SKILL.md`, `.agent/skills/testing-with-tizenclaw-tests/SKILL.md` | TDD Red→Green→Refactor cycle; system scenario added/updated |
| 4. Build/Deploy | `.agent/skills/building-deploying/SKILL.md` | `./deploy_host.sh` succeeded, or explicit `./deploy.sh` |
| 5. Test/Review | `.agent/skills/reviewing-code/SKILL.md`, `.agent/skills/testing-with-tizenclaw-tests/SKILL.md` | Host or device logs as evidence; `tizenclaw-tests` scenario result |
| 6. Commit | `.agent/skills/managing-versions/SKILL.md` | Clean commit via `.tmp/commit_msg.txt` |

---

## Global Environment Management

- **Primary Shell Context**: The agent runs **directly inside a WSL
  Ubuntu shell**. All project commands (`./deploy_host.sh`, `git`, etc.)
  are run as plain bash — no `wsl -e bash -c "..."` wrapper needed.
- **Edge Case — PowerShell**: If the agent is invoked from a Windows
  PowerShell session (e.g., Windows-side IDE), wrap every Linux command
  with `wsl -e bash -c "..."`. Consult `.agent/rules/shell-detection.md`
  to detect the active shell context.
- **Shell Detection Rule**: Before any command, follow
  `.agent/rules/shell-detection.md`. That rule is authoritative.
- **No Background Sub-processes**: Never use `nohup` or `&` for build or
  deploy commands. Run them synchronously in the foreground to avoid
  Samba/WSL I/O lockups.
- **Skill Reference**: `.agent/skills/managing-environment/SKILL.md`

---

## Documentation Location

All development-process documents (plans, designs, review artifacts)
created during Planning, Design, Review, or similar stage work **MUST**
be created under `.dev/docs/`.
Do **not** create new workflow or stage artifact documents under `docs/`.

---

## Deploy Commands

### Tizen (GBS build → emulator/device)
```bash
# Full build + deploy (auto-detect arch)
./deploy.sh -d emulator-26101

# Fast rebuild (skip GBS init)
./deploy.sh -d emulator-26101 -n

# Fastest incremental rebuild
./deploy.sh -d emulator-26101 -n -i

# Skip build, deploy existing RPM only
./deploy.sh -d emulator-26101 -s

# Build only, no deploy
./deploy.sh -d emulator-26101 -S
```

### Host Linux (default Ubuntu/WSL workflow)
```bash
# Release build + install + run (Generic Linux mode)
./deploy_host.sh

# Debug build
./deploy_host.sh -d

# Build only (no install/run)
./deploy_host.sh -b

# Run tests (offline, vendored)
./deploy_host.sh --test

# Daemon management
./deploy_host.sh --status   # check status
./deploy_host.sh --log      # follow logs
./deploy_host.sh --stop     # stop daemon
```

---

## Code Quality Rules

- **Zero build warnings**: All compiler warnings must be resolved at the code
  level. Do not suppress with `#![allow(...)]` except for C bindgen FFI.
- **No `.unwrap()` in production paths**: Use proper error propagation.
- **Minimal FFI**: Core AGI logic must be pure Rust. FFI only where
  Tizen-specific hardware/API is unavoidable.
- **Dynamic loading**: Tizen `.so` symbols must be loaded via `libloading`.
  The daemon must never panic if a native library is absent — always fall back
  gracefully.
- **`Send + Sync` on all async types**: Explicitly declare ownership bounds.

---

## Workspace Crates

### Canonical Rust Workspace (`rust/`)

| Crate | Role |
|-------|------|
| `tclaw-runtime` | Forward-looking runtime orchestration |
| `tclaw-api` | Shared contracts and stable types |
| `tclaw-cli` | Canonical CLI surface |
| `tclaw-tools` | Tool adapters and registries |
| `tclaw-plugins` | Plugin boundaries |
| `tclaw-commands` | Shared command-layer support |
| `rusty-claude-cli` | Claude-oriented CLI reconstruction |

### Legacy Rust Workspace (`src/`)

| Crate | Role |
|-------|------|
| `tizenclaw` | Main daemon — AgentCore, PromptBuilder, IPC |
| `tizenclaw-cli` | User-facing CLI (stdio → IPC) |
| `tizenclaw-tool-executor` | Secure native tool runner daemon |
| `libtizenclaw` | C-ABI bridge for legacy C/C++ callers |
| `libtizenclaw-core` | Core shared library |
| `tizenclaw-metadata-*` | Plugin metadata crates |

---

## Operation Log

All stage progress and Supervisor audit records are tracked in:
`.dev/DASHBOARD.md`

---

## Supervisor Gate

After each stage, the Supervisor validates compliance before authorizing
the next stage. See `.agent/skills/supervising-workflow/SKILL.md`.

### Per-Stage Pass Criteria

| Stage | Critical Pass/Fail Criteria |
|-------|----------------------------|
| **1. Planning** | Execution mode classified (host-default vs explicit Tizen); DASHBOARD updated |
| **2. Design** | FFI boundaries defined; `Send+Sync` specs present; `libloading` strategy documented; DASHBOARD updated |
| **3. Development** | No direct `cargo`/`cmake`; TDD cycle followed; system scenario added/updated; DASHBOARD updated |
| **4. Build/Deploy** | Correct script used for cycle; no direct `cargo build`; runtime install/deploy confirmed |
| **5. Test & Review** | Runtime logs captured; PASS/FAIL verdict issued with evidence; `tizenclaw-tests` result recorded |
| **6. Commit & Push** | `commit_msg.txt` used (no `-m` flag); workspace cleaned; no extraneous artifacts staged |

### Rollback Protocol

When a violation is detected:
1. Supervisor writes a Violation Record in `.dev/DASHBOARD.md`
2. Control returns to the violating stage with corrective guidance
3. Stage re-reads SKILL.md, applies fix, re-executes
4. Supervisor re-validates

Maximum **3 retry attempts** per stage gate before escalating to the user.
