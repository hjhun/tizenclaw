## OpenAI OAuth Expiry Hardening Design

### Scope

This cycle hardens the host-default OpenAI Codex OAuth path so that a
valid linked session does not regress into an avoidable refresh failure
after reconnects, restarts, or follow-up code changes.

### Ownership Boundaries

- `src/tizenclaw-cli/src/main.rs`
  owns the import/connect path from `~/.codex/auth.json` into the
  runtime cache at `~/.tizenclaw/config/llm_config.json`
- `src/tizenclaw/src/llm/openai.rs`
  owns runtime OAuth loading, JWT expiry derivation, and refresh
  decisions for the `openai-codex` backend
- `tests/system/basic_ipc_smoke.json`
  exposes the cached OAuth shape through daemon IPC so the guard remains
  observable outside unit tests

### Persistence Rules

- `~/.codex/auth.json` remains the source of truth shared with Codex CLI
- `~/.tizenclaw/config/llm_config.json` remains the imported runtime
  cache used when the source file is unavailable or the daemon is
  restarted
- cached `oauth.expires_at` must be populated from either explicit auth
  data or the JWT payload; placeholder values such as `0` must not
  override a valid JWT-derived expiry

### Verification Path

- update `tests/system/basic_ipc_smoke.json` first so daemon IPC asserts
  `backends.openai-codex.oauth.expires_at`
- add unit coverage for:
  - CLI snapshot export including JWT-derived expiry metadata
  - runtime fallback ignoring placeholder `expires_at=0` and using JWT
    expiry instead

