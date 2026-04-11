# Canonical Rust Workspace

This directory is the forward-looking production workspace for TizenClaw.

- `crates/tclaw-runtime` owns runtime orchestration
- `crates/tclaw-api` owns shared contracts
- `crates/tclaw-cli` owns the CLI surface
- `crates/tclaw-tools` owns tool abstractions
- `crates/tclaw-plugins` owns plugin boundaries

The legacy root Rust workspace remains available while the reconstruction
prompt series migrates functionality into this layout.
