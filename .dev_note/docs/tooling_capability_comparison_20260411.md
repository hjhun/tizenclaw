# Tooling Capability Comparison

## Implemented In This Cycle

- TizenClaw now exposes daemon-visible runtime capability checks for
  `bash`, `sh`, `python3`, `node`, direct executable support, and core
  linux file utilities
- `file_manager` now prefers linux utilities for read/list/stat/mkdir,
  remove, copy, and move, while keeping Rust fallbacks with debug logs
- embedded descriptor inventory is now reported explicitly as
  documentation-only metadata with migration guidance toward textual
  skills or built-in runtime features

## Reference Comparison

### `openclaw`

- stronger shell trust and wrapper analysis
- stronger skill install and auto-allow safety policy
- richer separation between execution policy and skill metadata

### `openclaude`

- stronger tool-pool assembly and skill-prefetch pipeline
- stronger shell-backed file and command tooling model
- stronger plugin and marketplace consolidation

### `nanoclaw`

- compact runtime ownership around executable detection and service
  bring-up
- good practical handling for node path discovery and process fallback

### `hermes-agent`

- stronger user-facing skill capability configuration
- stronger environment reporting and persistent-shell configuration
- broader migration tooling around imported skills and workspace state

## Remaining Gaps Worth Another Cycle

- shell trust and approval analysis is still much weaker than
  `openclaw`
- skill installation, enablement, and dependency checks remain much
  thinner than `openclaw` and `hermes-agent`
- tool and skill pool composition is still less explicit than
  `openclaude`
- persistent-shell and long-lived execution ergonomics remain behind
  `hermes-agent`
