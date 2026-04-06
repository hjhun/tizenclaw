# Web Dashboard Admin Refresh Design

## Port Resolution

- Introduce `default_dashboard_port()` in runtime path helpers.
- Return `8080` on generic Linux hosts and `9090` on Tizen runtime.
- Use the helper in daemon bootstrap, the standalone dashboard binary,
  and the web dashboard channel.

## CLI Override Path

- Extend the channel trait with a small `configure(settings)` hook.
- Pass optional `settings` through IPC `start_channel`.
- Implement dashboard-specific runtime updates for `port` and
  `localhost_only`.
- Support `tizenclaw-cli dashboard start --port <n>`.

## Admin Session Recovery

- Keep client-side token persistence in both `localStorage` and
  `sessionStorage` for smooth migration.
- Add `GET /api/auth/session` to validate an existing bearer token.
- Replace the previous memory-only token approach with a signed token that
  remains valid after dashboard process restart until expiration.
- Keep password changes invalidating older tokens by deriving the
  signature from the stored password hash.

## Admin Editor UX

- Render the config list as summary cards instead of expanded raw JSON.
- Open a modal dialog for the selected config.
- Provide two editing modes:
  - Structured editor for top-level JSON object fields.
  - Raw editor for full document editing and non-object payloads.
- Keep save and reload actions inside the modal and refresh the card list
  after successful writes.
