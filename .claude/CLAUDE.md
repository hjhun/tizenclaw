# TizenClaw Agent Guide

This is the durable project instruction file for coding agents working in
this repository.

## Start Here

Read in this order when joining the project:

1. `README.md`
2. `docs/ONBOARDING.md`
3. `docs/claw-code-analysis/README.md`
4. `docs/claw-code-analysis/expert-overview.md`
5. `docs/claw-code-analysis/overview-rust.md`
6. `docs/claw-code-analysis/overview-python.md`
7. `docs/claw-code-analysis/overview-shell-and-tests.md`
8. `prompt/README.md`

## Core Architecture

- `rust/` is the canonical production workspace for the reconstruction.
- `rust/crates/tclaw-runtime` owns runtime orchestration and durable execution.
- `rust/crates/tclaw-api` owns shared contracts and stable types.
- `rust/crates/tclaw-cli` owns the forward-looking operator CLI surface.
- `rust/crates/tclaw-tools` owns tool adapters and registry boundaries.
- `rust/crates/tclaw-plugins` owns plugin loading and plugin contracts.
- `rust/crates/tclaw-commands` and `rust/crates/rusty-claude-cli` are part of
  the canonical Rust workspace and must stay aligned with the runtime/API
  contracts.
- `src/tizenclaw_py` and `tests/python` are the Python parity workspace for
  audit, explanation, and lightweight contract checks.
- `src/tizenclaw*` still contains the legacy Rust implementation. Do not move
  functionality out of it casually; migration must be deliberate.
- `tests/system/` contains daemon-visible JSON-RPC scenario contracts.

## Canonical vs Parity Warning

- Treat Rust as the source of truth for production runtime behavior.
- Treat Python as a parity and explanation layer, not the production runtime.
- When changing public concepts, update both sides intentionally:
  - canonical Rust names and contracts in `rust/`
  - Python parity surfaces in `src/tizenclaw_py`
  - related parity tests in `tests/python`
  - relevant docs and prompt notes when architecture intent changes
- Do not mistake the legacy Rust tree under `src/tizenclaw*` for the Python
  parity workspace. They share the `src/` root during the migration period.

## Build And Test Commands

Use repository scripts, not ad-hoc direct Cargo commands.

```bash
./install.sh --local-checkout
./deploy_host.sh
./deploy_host.sh --test
python3 scripts/verify_doc_architecture.py
bash rust/scripts/run_mock_parity_harness.sh
```

Use `./deploy.sh` only when the task explicitly asks for Tizen, emulator, or
device validation.

## Repository Rules

- Default to the host path: `./deploy_host.sh`
- Do not run direct `cargo build`, `cargo test`, `cargo check`, `cargo clippy`,
  or ad-hoc `cmake` for ordinary repository work.
- Keep the default architecture focus on `x86_64`.
- When daemon-visible behavior changes, add or update a scenario in
  `tests/system/` **before** implementation and validate it against the host
  daemon using `tizenclaw-tests`.
- Keep onboarding docs and analysis docs aligned with structural changes.
- Prefer additive reconstruction. Do not delete the legacy runtime layout until
  migration work explicitly says so.

## Commit Rules

- **NEVER** use `git commit -m "..."`. Write the message to
  `.tmp/commit_msg.txt` first, then run `git commit -F .tmp/commit_msg.txt`.
- All commit messages must be in **English**.
- Title: ≤ 50 characters, imperative sentence, capitalized.
- Body: each line ≤ 80 characters.
- No `feat:`, `fix:`, `refactor:` prefixes. No explicit `Why:` / `What:`
  headers.
- Push target: `git push origin develRust`.
- All temporary files go in `.tmp/` (project root), never in `/tmp/`.
  `.tmp/` is in `.gitignore` and must never be committed.

## Repeatable Workflows

### Docs And Layout Validation

1. Update the relevant docs.
2. Run `python3 scripts/verify_doc_architecture.py`.
3. Run `bash rust/scripts/run_mock_parity_harness.sh`.
4. Run `./deploy_host.sh --test` if the change could affect repository layout
   assumptions or public contracts.

### Runtime Change Workflow

1. Update or add a `tests/system/*.json` scenario first.
2. Implement the Rust change in the canonical workspace or the explicitly
   targeted legacy runtime path.
3. Align Python parity surfaces if the public contract changed.
4. Validate with `./deploy_host.sh` and `./deploy_host.sh --test`.
5. Run `tizenclaw-tests scenario --file tests/system/<name>.json` against
   the live host daemon to confirm the IPC contract holds.

### Continue The Reconstruction

1. Read `prompt/README.md`.
2. Work through prompt files in numeric order unless a later prompt says
   otherwise.
3. Convert prompt output into durable repository files, not session-only notes.
4. Update onboarding docs when the prompt changes the repository's expected
   reading order or ownership boundaries.
