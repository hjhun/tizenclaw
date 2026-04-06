# Web Dashboard Admin Refresh Planning

## Scope

- Change the Linux host default dashboard port from `9090` to `8080`.
- Allow `tizenclaw-cli` to start the dashboard with a custom port override.
- Replace the inline admin JSON editor with a focused popup workflow.
- Fix admin page recovery when the user returns after navigating away or
  reopening the page while still logged in.

## Constraints

- Do not use local `cargo build`, `cargo test`, `cargo check`, or
  `cargo clippy`.
- Validate the final behavior through `./deploy.sh -a x86_64`.
- Keep Tizen runtime behavior compatible with the current `9090` default.

## Work Items

1. Add a runtime-aware default dashboard port helper.
2. Thread optional runtime port overrides from CLI to the dashboard channel.
3. Add a lightweight admin session validation endpoint.
4. Make dashboard auth restoration resilient across page revisit and
   dashboard process restarts.
5. Redesign admin config editing around a popup with structured fields and
   a raw editor fallback.
