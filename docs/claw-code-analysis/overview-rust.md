# Rust Overview

The canonical runtime workspace lives under `rust/`.

## Workspace Layout

```text
rust/
├── Cargo.toml
├── README.md
└── crates/
    ├── tclaw-api/
    ├── tclaw-cli/
    ├── tclaw-plugins/
    ├── tclaw-runtime/
    └── tclaw-tools/
```

## Responsibilities

- `tclaw-runtime`: runtime boot, orchestration, and durable execution
- `tclaw-api`: shared contracts and stable types
- `tclaw-cli`: command surface for operators and local workflows
- `tclaw-tools`: tool abstractions and host/platform adapters
- `tclaw-plugins`: dynamic or declarative plugin boundaries

## Bootstrap Rule

Each crate should compile independently with small but valid scaffolding so
later prompts can extend rather than replace it.
