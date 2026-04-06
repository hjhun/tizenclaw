# README and Documentation Refresh Planning

## Goal

Rewrite the public-facing documentation so TizenClaw is introduced as a
cohesive product rather than a loose Rust workspace. The deliverables
must explain what the daemon does, how the workspace is organized, and
how new users can build, deploy, and operate it on Tizen targets.

## Execution Mode

- Documentation refresh for a long-running daemon project
- Output scope: public README plus detailed guides under `docs/`
- Validation path: repository edits followed by `./deploy.sh -a x86_64`

## Inputs Reviewed

- Existing `README.md`
- Workspace `Cargo.toml` and crate manifests
- `deploy.sh`
- Source layout under `src/tizenclaw/src`
- Reference README files from:
  - `openclaw`
  - `nanoclaw`
  - `hermes-agent`

## Documentation Objectives

1. Present a stronger top-level product story in polished English.
2. Reflect the current Rust workspace, daemon runtime, dashboard, CLI,
   tool executor, and plugin crates accurately.
3. Replace stale or misleading guidance, especially any workflow that
   suggests local `cargo test` as the primary verification path.
4. Add dedicated documentation for:
   - repository and runtime structure
   - build, deploy, and usage flows

## Planned Deliverables

- Rewritten `README.md`
- New `docs/STRUCTURE.md`
- New `docs/USAGE.md`

## Content Direction

- Borrow the strengths of the reference READMEs:
  - strong opening narrative
  - fast navigation links
  - operator-oriented quick start
  - clear capability summary
  - architecture and deployment framing
- Keep the tone professional and infrastructure-focused to match the
  embedded Tizen daemon audience.

## Risks

- The current codebase mixes host-oriented comments with Tizen-first
  deployment rules, so the new documentation must clearly separate
  development convenience from the required validation path.
- The workspace contains multiple metadata and bridge crates that should
  be grouped coherently rather than listed without explanation.

## Completion Criteria

- Public docs are fully in English.
- README narrative is significantly more detailed than the current file.
- Structure and usage guides exist under `docs/`.
- Dashboard records this stage and the supervisor gate outcome.
