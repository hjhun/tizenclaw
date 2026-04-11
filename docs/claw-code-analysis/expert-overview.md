# Expert Overview

TizenClaw is being reconstructed as a dual-workspace project.

## Architectural Intent

- Rust is the production implementation path.
- Python mirrors public behavior for audits, parity checks, and explanatory
  tooling.
- Canonical paths must stay stable so later prompts can add real code without
  another repository reshuffle.

## Domain Split

- CLI surface
- runtime
- API contracts
- tools
- plugins
- Python parity workspace

## Migration Constraint

The repository already contains a legacy Rust workspace under `src/`. The new
canonical workspace under `rust/` should grow alongside it until later prompts
complete the migration intentionally.
