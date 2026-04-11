## OpenAI OAuth Devel Regression Design

### Scope

This cycle adds a dedicated daemon-visible OAuth regression test and
binds it to `./deploy_host.sh --devel` so devel-mode startup fails fast
when the Codex OAuth cache regresses again.

### Ownership Boundaries

- `src/tizenclaw-tests/src/scenario.rs`
  owns scenario assertion semantics, including any comparison operator
  needed to express the regression cleanly
- `tests/system/openai_oauth_regression.json`
  owns the narrow daemon-facing contract for the OpenAI Codex OAuth
  cache shape
- `deploy_host.sh`
  owns the host devel-mode entry workflow and should trigger the
  installed regression scenario after the daemon starts

### Persistence Impact

- no new persistence file or registry entry is needed
- the regression reads existing `get_llm_config` output from the live
  daemon and validates `backends.openai-codex.oauth`

### Verification Path

- add the dedicated OAuth regression scenario before product-code
  changes
- extend `tizenclaw-tests` with a numeric comparison assertion so the
  scenario can prove `oauth.expires_at > 0`
- make `./deploy_host.sh --devel` run the installed scenario and abort
  the entry flow if the regression fails

