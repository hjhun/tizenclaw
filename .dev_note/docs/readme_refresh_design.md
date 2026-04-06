# README and Documentation Refresh Design

## Information Architecture

The documentation set will be split into three layers:

1. `README.md`
   - public landing page
   - product overview
   - capability summary
   - quick start and deployment entry points
   - concise architecture summary
   - documentation map
2. `docs/STRUCTURE.md`
   - workspace layout
   - runtime component responsibilities
   - subsystem map for daemon, channels, LLM backends, storage, Tizen
     adapters, and supporting binaries
3. `docs/USAGE.md`
   - prerequisites
   - build and deploy flow
   - service lifecycle checks
   - CLI and dashboard usage
   - configuration and extension touchpoints

## README Layout

The README will use the following flow:

1. Hero section with product statement and badges
2. Quick navigation links
3. Why TizenClaw exists
4. Core capabilities
5. Workspace highlights
6. Quick start for deployment
7. Daily operation examples
8. Architecture summary
9. Documentation index
10. License

## Accuracy Rules

- Describe TizenClaw as a Rust-based autonomous agent daemon for Tizen
  and embedded Linux environments.
- Emphasize `deploy.sh` as the primary build and deployment entry point.
- Avoid recommending local `cargo test` or `cargo build` as the default
  validation path.
- Reflect the current crate boundaries from the workspace manifests.

## Technical Details to Surface

- Dynamic platform integration relies on `libloading` and runtime-loaded
  Tizen shared libraries instead of hard link assumptions.
- Shared runtime components are designed around async orchestration, and
  public docs will describe the daemon as a coordinated Tokio-based
  service without overspecifying unstable internals.
- FFI boundaries belong to `libtizenclaw` and `libtizenclaw-core`,
  while the main daemon keeps most orchestration logic in Rust.
- The dashboard and CLI both communicate with the daemon through IPC.

## Design Constraints

- All public-facing prose must be in English.
- The docs should be externally readable, not just internal notes.
- Structure descriptions should group crates by operational role instead
  of presenting a raw manifest dump.
- Usage guidance should cover both emulator and device deployment,
  keeping x86_64 validation explicit.

## Supervisor-Relevant Compliance Notes

- FFI boundaries are documented at the crate level.
- The design explicitly calls out runtime dynamic loading through
  `libloading`.
- Async orchestration and shared runtime behavior are described in a way
  consistent with `Send`/`Sync`-friendly service boundaries, without
  inventing traits that are not public API.
