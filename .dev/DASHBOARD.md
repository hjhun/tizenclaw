# DASHBOARD

## Actual Progress

- Goal: Runtime flexibility improvements (provider selection, Telegram config
  externalization, ClawHub update flow, skill snapshot caching).
- Active roadmap focus: reviewer rework pass — all three findings addressed.
- Current workflow phase: complete
- Last completed workflow phase: evaluate
- Supervisor verdict: `approved` (pending rework pass verification)
- Escalation status: none

## Workflow Phases

```mermaid
flowchart LR
    refine([Refine]) --> plan([Plan])
    plan --> design([Design])
    design --> develop([Develop])
    develop --> build_deploy([Build/Deploy])
    build_deploy --> test_review([Test/Review])
    test_review --> commit([Commit])
    commit --> evaluate([Evaluate])
```

## Completed Work

### Reviewer Finding 1 — High: Plugin fallback broken when providers[] present

**Root cause**: The old retain logic used `ordered_names.iter().any(...)` which
dropped any backend not explicitly listed in `providers[]`, preventing
plugin-discovered backends from serving as last-resort fallbacks.

**Fix**: Changed retain predicate to `routing.providers.iter().find(...).map(|p|
p.enabled).unwrap_or(true)` in both startup (`init_backends`) and reload paths.
Plugin-discovered backends absent from `providers[]` are now kept at the end of
the instance list as last-resort fallbacks.

Files: `runtime_core_impl.rs` (startup ~line 1108, reload ~line 1304)

### Reviewer Finding 2 — Medium: Empty providers[] not authoritative

**Root cause**: The sort/retain block was guarded by `if !ordered_names.is_empty()`
which skipped the block entirely when `providers: []` (empty array), letting
legacy candidate routing run unchanged instead of treating the empty array as
an authoritative signal.

**Fix**: Added `providers_array_present: bool` field to `ProviderRoutingConfig`
and changed the guard to `if routing.providers_array_present ||
!ordered_names.is_empty()`. An explicit empty `providers: []` now correctly
enters the authoritative retain block, which (with `unwrap_or(true)`) retains
only plugin-discovered backends as fallbacks — no explicitly-listed-and-disabled
backends survive.

Files: `provider_selection.rs` (struct + translator), `runtime_core_impl.rs`
(both paths)

### Reviewer Finding 3 — Medium: backends.gemini.model fallback dead in practice

**Root cause**: The fallback was skipped via `if model_already_set { continue; }`
which treated the builtin Gemini default model as an operator-set value, so
`backends.gemini.model` in `llm_config.json` was never applied unless the
operator first cleared the builtin model. The test masked this by manually
clearing the builtin model before calling the function.

**Fix**: Replaced `is_some()` guard with a before/after snapshot comparison.
The function now snapshots `pre_telegram_models` before merging the telegram
section. The fallback applies if the model did not change during telegram-section
merge (i.e., the telegram section did not provide an explicit override). The
builtin Gemini default no longer blocks the `backends.gemini.model` fallback.
The test no longer clears the builtin model manually.

Files: `client_impl.rs` (`read_backend_models_from_llm_config`),
`tests.rs` (removed manual model clear)

## Validation Evidence

- `./deploy_host.sh --test` — PASS
  - 659+ unit/integration tests across all crates
  - 0 failures, 0 ignored failures
  - Mock parity harness: PASS
  - Doc architecture verification: PASS

## Risks and Residual Notes

- The `providers_array_present` flag distinguishes `providers: []` (no
  configured providers, plugin fallbacks still eligible) from an absent key
  (legacy routing applies). This is intentional and matches the design doc.
- Telegram model fallback now correctly applies `backends.<name>.model` when
  no telegram-section override exists, including when the builtin model is set.
  Operators using `backends.gemini.model` in `llm_config.json` no longer need
  to also configure telegram-specific overrides.
- No Tizen/emulator validation was performed; host-first scope is confirmed
  by the task prompt.
