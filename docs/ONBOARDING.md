# TizenClaw Onboarding

This guide is the shortest reliable path for a new contributor or coding agent
to understand how to keep the repository moving without rediscovering its
architecture.

## Recommended Reading Order

1. `README.md` for the current repository status
2. `.claude/CLAUDE.md` for the durable build/test and repository rules
3. `docs/claw-code-analysis/README.md`
4. `docs/claw-code-analysis/expert-overview.md`
5. `docs/claw-code-analysis/overview-rust.md`
6. `docs/claw-code-analysis/overview-python.md`
7. `docs/claw-code-analysis/overview-shell-and-tests.md`
8. `docs/STRUCTURE.md` if you need the legacy runtime map
9. `prompt/README.md` before continuing the staged rebuild

## Workspace Responsibilities

### Canonical Rust Workspace

- `rust/crates/tclaw-runtime`: forward-looking runtime orchestration
- `rust/crates/tclaw-api`: shared contracts and stable types
- `rust/crates/tclaw-cli`: CLI entrypoint for the canonical workspace
- `rust/crates/tclaw-tools`: tool registry and platform adapters
- `rust/crates/tclaw-plugins`: plugin boundaries and plugin loading
- `rust/crates/tclaw-commands`: command-layer support shared by the rebuilt
  Rust surfaces
- `rust/crates/rusty-claude-cli`: reconstructed Claude-oriented CLI surface

### Python Parity Workspace

- `src/tizenclaw_py`: explanation-friendly parity modules
- `tests/python`: lightweight parity and bootstrap tests

Python mirrors public concepts and contract shape. It is not the production
runtime.

### Legacy Runtime Workspace

- `src/tizenclaw*`, `src/libtizenclaw*`, and related crates still hold the
  active legacy Rust implementation.
- Treat these paths as operational code, not dead history.
- Migrate code from them only when the active prompt or task explicitly does
  that work.

### Contract And Scenario Surfaces

- `tests/system/`: daemon-visible JSON-RPC scenario contracts
- `tests/scenarios/`: existing scenario assets
- `docs/claw-code-analysis/`: reconstruction intent and ownership baselines

## Installer And Validation Commands

Use repository scripts so onboarding and CI stay aligned.

```bash
./install.sh --local-checkout
./deploy_host.sh
./deploy_host.sh --test
python3 scripts/verify_doc_architecture.py
bash rust/scripts/run_mock_parity_harness.sh
```

Use `./deploy.sh` only for explicit Tizen, emulator, or device validation.

## How To Continue The Rebuild

The `prompt/` directory is the reconstruction queue. Treat each prompt as a
durable repository change request, not a one-off note.

1. Read `prompt/README.md`.
2. Start from the lowest-numbered prompt that is not already reflected in the
   repository.
3. Use the analysis docs to preserve the intended domain split while you work.
4. Land the prompt as durable files, tests, and docs.
5. Update onboarding guidance if the prompt changes repository ownership or
   the recommended reading path.

## Rules That Prevent Drift

- Rust under `rust/` is the canonical production target.
- Python under `src/tizenclaw_py` and `tests/python` exists for parity,
  audit, and explanation.
- The legacy Rust tree under `src/tizenclaw*` remains operational during the
  migration period.
- Do not run ad-hoc direct `cargo` or `cmake` commands for ordinary work.
- If a change affects daemon-visible behavior, add or update a
  `tests/system/*.json` scenario before implementation.
