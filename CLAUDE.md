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
- the Python parity workspace under `src/tizenclaw_py` and `tests/python`
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
1. Planning → 2. Design → 3. Development →
4. Build/Deploy → 5. Test/Review → 6. Commit
```

Each stage has a corresponding skill in `.agent/skills/`. After each stage,
update `.dev/DASHBOARD.md` with the stage status.

| Stage | Skill | Key Output |
|-------|-------|------------|
| 1. Planning | `.agent/skills/planning-project/SKILL.md` | Module objectives, execution mode classification |
| 2. Design | `.agent/skills/designing-architecture/SKILL.md` | FFI boundaries, async topology docs |
| 3. Development | `.agent/skills/developing-code/SKILL.md` | TDD Red→Green→Refactor cycle |
| 4. Build/Deploy | `.agent/skills/building-deploying/SKILL.md` | `./deploy_host.sh` succeeded, or explicit `./deploy.sh -a x86_64` |
| 5. Test/Review | `.agent/skills/reviewing-code/SKILL.md` | Host or device logs as evidence |
| 6. Commit | `.agent/skills/managing-versions/SKILL.md` | Clean commit via `.tmp/commit_msg.txt` |

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

Rollback is triggered if:
- Local `cargo` commands were used
- The wrong script path was used for the cycle
- Commit used inline `-m` flag
- No host/device logs provided in Test/Review stage
- Extraneous build artifacts are staged for commit

Maximum 3 retries per stage gate before escalating to the user.
