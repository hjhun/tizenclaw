# TizenClaw

TizenClaw is being reconstructed from documentation so the repository can
converge on the analyzed `claw-code` architecture without repeated layout
churn.

## Workspace Policy

- Rust under `rust/` is the canonical production runtime.
- Python under `src/` and `tests/` is the parity, audit, and explanation
  workspace.
- The existing legacy Rust tree under `src/tizenclaw*` remains in place during
  the reconstruction and migration period.

## Intended Architecture

The reconstructed project keeps the domain split described by the analysis
docs:

- `rust/crates/tclaw-runtime`: long-running runtime and orchestration core
- `rust/crates/tclaw-api`: stable contracts shared across surfaces
- `rust/crates/tclaw-cli`: CLI entrypoint on top of the Rust runtime
- `rust/crates/tclaw-tools`: host and platform tool adapters
- `rust/crates/tclaw-plugins`: plugin registry and loading boundaries
- `src/tizenclaw_py`: Python parity modules for auditing and explanation
- `tests/`: Python tests plus daemon/system scenario assets

## Repository Status

This repository already contains a substantial Rust implementation in the
legacy workspace rooted at [src](/home/hjhun/samba/github/tizenclaw/src). The
new `rust/` workspace added in this prompt is the forward-looking canonical
layout for future reconstruction work. Later prompts should add runtime code to
`rust/crates/` first, then retire or absorb legacy paths intentionally.

## Key Documents

- [ROADMAP.md](/home/hjhun/samba/github/tizenclaw/ROADMAP.md)
- [docs/ONBOARDING.md](/home/hjhun/samba/github/tizenclaw/docs/ONBOARDING.md)
- [.claude/CLAUDE.md](/home/hjhun/samba/github/tizenclaw/.claude/CLAUDE.md)
- [docs/claw-code-analysis/README.md](/home/hjhun/samba/github/tizenclaw/docs/claw-code-analysis/README.md)
- [docs/STRUCTURE.md](/home/hjhun/samba/github/tizenclaw/docs/STRUCTURE.md)
- [prompt/README.md](/home/hjhun/samba/github/tizenclaw/prompt/README.md)

## Build And Test Bootstrap

- Host-first repository validation remains `./deploy_host.sh`
- Rust canonical workspace bootstrap lives in `rust/`
- Python parity bootstrap is configured through `pyproject.toml` and
  `pytest.ini`
- Local checkout installation from the repository root is supported through
  `./install.sh` or `./install.sh --local-checkout`
- Documentation and parity verification helpers live at
  `scripts/verify_doc_architecture.py` and
  `rust/scripts/run_mock_parity_harness.sh`

## Root Commands

- Install the current checkout: `./install.sh --local-checkout`
- Run the host validation path: `./deploy_host.sh --test`
- Run only the doc/layout verifier:
  `python3 scripts/verify_doc_architecture.py`
- Run only the parity harness:
  `bash rust/scripts/run_mock_parity_harness.sh`

This prompt establishes the foundation only. It does not claim feature parity
with the analyzed system yet.
