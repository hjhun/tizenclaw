# 2026-04-13 Host CI Artifact Fix Design

## Scope

Fix the host GitHub workflow failure where bundle generation expects
release artifacts that were never built, and restore offline vendored
dependency consistency for the canonical Rust workspace build.

## Subsystem Boundaries And Ownership

- `scripts/create_host_release_bundle.sh`
  - owns bundle-oriented host build orchestration
  - must request a release build when release artifacts are required
- `deploy_host.sh`
  - remains the canonical host build/test entrypoint
  - build mode selection stays externalized through CLI flags
- `vendor/`
  - owns offline dependency reproducibility for host CI
  - must match `rust/Cargo.lock`

## Persistence And Runtime Path Impact

- No daemon-state persistence changes
- No IPC message schema changes
- No runtime path changes outside build artifact selection in CI

## IPC-Observable Assertions

- None required for daemon runtime behavior
- Verification assertions are build-path oriented:
  - `scripts/create_host_release_bundle.sh` must produce release-mode
    binaries without requiring a debug-to-release path mismatch
  - `./deploy_host.sh --test` must still pass on the host path
  - release bundle generation must find the expected binaries in
    `~/.tizenclaw/build/cargo-target/release/`

## FFI Boundaries, Send+Sync, And libloading Strategy

- No new FFI introduced
- Existing Tizen-specific FFI boundaries remain unchanged
- Existing `libloading` strategy for Tizen `.so` symbols remains
  unchanged and out of scope for this CI-only fix
- No async ownership model changes; `Send + Sync` expectations remain
  unchanged

## Verification Plan

1. Update the bundle script so the build-only invocation uses
   `./deploy_host.sh --release -b`
2. Refresh vendored dependencies so `vendor/libc` matches the locked
   version required by `rust/Cargo.lock`
3. Run `./deploy_host.sh --test`
4. Run the host bundle generation script and confirm it finds release
   artifacts successfully
