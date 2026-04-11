# Reconstruction Prompt Sequence

The files in this directory are the staged rebuild plan for the repository.

## How To Use This Directory

1. Read prompts in numeric order.
2. Treat each prompt as additive unless it explicitly says to replace or retire
   an earlier structure.
3. Preserve the analysis-doc architecture split while implementing each prompt:
   - canonical Rust production workspace under `rust/`
   - Python parity workspace under `src/tizenclaw_py` and `tests/python`
   - legacy Rust runtime under `src/tizenclaw*` until migration is complete
4. Turn prompt output into durable repository files, tests, and docs.
5. Update `README.md`, `docs/ONBOARDING.md`, or `.claude/CLAUDE.md` when a
   prompt changes the expected contributor workflow.

## Current Sequence

1. `0031-rebuild-foundation-and-scope.md`
2. `0032-rust-runtime-skeleton.md`
3. `0033-python-parity-surface.md`
4. `0034-tool-and-plugin-boundaries.md`
5. `0035-cross-workspace-validation.md`

## Practical Loop

For each prompt:

1. Re-read the relevant `docs/claw-code-analysis/*.md` files.
2. Inspect the existing implementation before adding new files.
3. Keep Rust, Python parity, tests, and docs aligned.
4. Prefer small durable scaffolding over large speculative placeholders.
