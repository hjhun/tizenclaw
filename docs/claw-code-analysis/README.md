# claw-code Analysis Baseline

This directory is the documentation baseline used for the reconstruction
prompts. The original analysis bundle was not present in the checkout for this
run, so the files here capture the minimum architectural contract needed to
bootstrap the repository consistently.

## Source-of-Truth Summary

- Rust is the canonical runtime target.
- Python exists for porting, parity checks, auditing, and explanation.
- The repository should expose clear boundaries for CLI, runtime, API, tools,
  and plugins.
- Host-first validation uses `./deploy_host.sh`; Tizen deployment remains
  opt-in through `./deploy.sh`.

## Files

- `expert-overview.md`: high-level architecture contract
- `overview-rust.md`: canonical Rust workspace layout
- `overview-python.md`: Python parity workspace layout
- `overview-shell-and-tests.md`: build/test and system-observability rules
