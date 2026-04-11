## OpenAI OAuth Recovery Design

### Scope

This cycle targets the host-default OpenAI Codex OAuth flow used by:

- `src/tizenclaw-cli/src/main.rs`
- `src/tizenclaw/src/llm/openai.rs`
- daemon-visible config inspection through IPC

### Ownership Boundaries

- `tizenclaw-cli` owns importing Codex CLI login state into
  `~/.tizenclaw/config/llm_config.json`
- `tizenclaw` runtime owns loading OAuth credentials, refreshing them,
  and translating them into request headers for `openai-codex`
- the daemon IPC surface owns read-only observability of the configured
  backend shape through `get_llm_config`

### Persistence Boundaries

- Source auth state remains the Codex CLI store at `~/.codex/auth.json`
- Imported runtime config remains under
  `~/.tizenclaw/config/llm_config.json`
- Runtime refresh writes remain limited to the Codex auth file when the
  source is `codex_cli`

### Fix Strategy

- make OAuth token extraction tolerant of schema drift in `auth.json`
  while preserving the current `tokens.*` contract
- keep `account_id` derivation resilient by falling back to JWT claims
  when the explicit field is absent
- centralize the token extraction rules so CLI import and runtime auth
  loading do not diverge again

### IPC-observable Assertions

Update `tests/system/basic_ipc_smoke.json` so the host daemon must expose:

- `backends.openai-codex.oauth.source == "codex_cli"`
- a non-empty `backends.openai-codex.oauth.auth_path`
- a present `backends.openai-codex.oauth.account_id`

These assertions avoid checking secrets while still proving the daemon
loaded the expected OAuth-backed configuration shape.
