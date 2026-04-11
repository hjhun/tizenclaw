# Shell And Tests Overview

## Execution Policy

- Default host path: `./deploy_host.sh`
- Explicit Tizen path: `./deploy.sh`
- Do not use ad-hoc direct `cargo build` or `cargo test` for ordinary
  repository validation

## Test Surfaces

- `tests/system/`: daemon-visible JSON scenario contracts
- `tests/scenarios/`: existing scenario assets
- `tests/python/`: parity/bootstrap tests for the Python workspace

## Bootstrap Expectations

- repository docs must explain the split between canonical Rust and Python
  parity workspaces
- the Rust workspace must have a valid `Cargo.toml`
- the Python workspace must have importable modules and a runnable pytest
  configuration
