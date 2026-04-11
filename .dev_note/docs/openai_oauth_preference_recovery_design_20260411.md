## OpenAI OAuth Preference Recovery Design

### Scope

This cycle hardens the recurring OpenAI Codex OAuth path by making the
runtime prefer `openai-codex` whenever valid OAuth auth state is
available, instead of treating it as just another configured backend.

### Ownership Boundaries

- `src/tizenclaw/src/core/agent_core.rs`
  owns backend candidate gathering and priority decisions
- `src/tizenclaw/src/core/ipc_server.rs`
  exposes runtime backend state so tests can verify which backend the
  daemon actually selected
- `tests/system/openai_oauth_regression.json`
  remains the narrow daemon-visible regression contract for OAuth shape
  plus live backend preference

### Persistence Impact

- no new persistence or registry file is needed
- preference is derived from existing `llm_config.json` OAuth fields or
  the shared Codex auth store on disk

### Verification Path

- extend the existing OAuth regression scenario first
- add a runtime backend IPC status method
- add unit coverage for the OAuth-priority boost so valid auth state
  outranks the default hosted backends

