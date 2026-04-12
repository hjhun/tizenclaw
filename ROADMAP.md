# Reconstruction Roadmap

This roadmap maps the prompt backlog to concrete build phases for the
documentation-driven reconstruction.

## Phase Map

| Phase | Prompt File | Focus |
| --- | --- | --- |
| 1 | `prompt/0031-rebuild-foundation-and-scope.md` | Establish the canonical Rust workspace, repository support tooling, root docs, and shared bootstrap config |
| 2 | `prompt/0032-rust-runtime-skeleton.md` | Move the runtime skeleton into `rust/crates/tclaw-runtime` and wire core contracts through `tclaw-api` |
| 3 | `prompt/0033-python-parity-surface.md` | Historical prompt for the now-retired Python parity surface |
| 4 | `prompt/0034-tool-and-plugin-boundaries.md` | Rebuild tool execution and plugin loading boundaries under the canonical Rust workspace |
| 5 | `prompt/0035-cross-workspace-validation.md` | Align host scripts, repository audits, and system scenarios across the supported workspaces |

## Delivery Rules

- New runtime work lands in `rust/` first.
- Repository support tooling should reflect the supported Rust surfaces rather
  than advertise independent runtime behavior.
- `tests/system/` remains the daemon-facing contract area.
- The legacy Rust tree under `src/tizenclaw*` should only change when a prompt
  explicitly migrates or retires part of it.

## Prompt Backlog Notes

The current checkout did not contain a `prompt/` directory, so this prompt
creates a minimal numbered backlog that later reconstruction prompts can extend
without renaming paths again.
