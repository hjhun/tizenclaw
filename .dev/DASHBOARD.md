# DASHBOARD

## Actual Progress

- Goal: Advance runtime flexibility and operator maintainability (tizenclaw_improve)
- Active roadmap focus: all five roadmap items completed + five rework passes complete
- Current workflow phase: evaluate (complete)
- Last completed workflow phase: evaluate
- Supervisor verdict: `approved`
- Escalation status: `none`

## Fifth Rework Pass — Reviewer Findings Fixed

### Finding 1 (High): legacy `backends.*.priority` ignored in compatibility translation

**Root cause**: `ProviderCompatibilityTranslator::translate()` synthesized
provider order from only `active_backend` and `fallback_backends`, ignoring
any explicit `backends.<name>.priority` values in the config document.
`build_backend_candidates()` in `tool_runtime.rs` reads those priorities and
uses them to sort candidates (higher number = selected first), but the routing
config produced by the translator did not reflect them.  A legacy config that
preferred a provider through `backends.<name>.priority` could therefore
initialize the right backend but still route prompts in the wrong order.

**Fix**: Rewrote the legacy synthesis path in `translate()`.  For each
provider name derived from `active_backend` / `fallback_backends`, the
translator now reads `backends.<name>.priority` from the config document when
present.  Candidates are sorted descending by this raw priority (matching
`sort_backend_candidates` semantics).  When no explicit priority is set, the
existing positional tie-break scores are used (active=1000,
fallback[0]=900, …) so existing configs that rely on `active_backend` order
are unaffected.  The resulting `providers` vector is ordered so
`ordered_names()` returns the correct selection order.

### Finding 2 (Medium): `status_json()` reported `"ready"` regardless of circuit-breaker state

**Root cause**: `ProviderRegistry::status_json()` marked any initialized
provider as `"availability": "ready"` based only on whether an instance
existed, without consulting the circuit-breaker state that `is_backend_available()`
checks at request time.  This left the operator-facing status claiming a
provider was ready even when routing would skip it due to an open circuit.

**Fix**: Added an `is_available: impl Fn(&str) -> bool` predicate parameter
to `status_json()`.  The caller in `runtime_admin_impl.rs` passes
`|name| self.is_backend_available(name)`.  Initialized providers now report:
- `"open_circuit"` when `is_available` returns false
- `"ready"` when `is_available` returns true
- `"unavailable"` when the backend failed to initialize (unchanged)

**Test coverage added**: Three new unit tests:
- `legacy_backend_priority_overrides_active_backend_position`: verifies
  `backends.openai.priority=2000` routes openai ahead of active_backend gemini
  (default 1000).
- `legacy_fallback_priority_ordering_respected`: verifies two fallbacks with
  explicit priorities sort in priority order.
- `status_json_reflects_circuit_breaker_state`: verifies `"open_circuit"` when
  the predicate returns false and `"ready"` when it returns true, using a stub
  `LlmBackend`.

**Validation**: `./deploy_host.sh --test` — 597 passed, 0 failed.

## Completed Work

All five roadmap targets have been implemented, tested, and committed.
Five rework passes have addressed all reviewer findings.

1. **Provider-selection layer** — `src/tizenclaw/src/core/provider_selection.rs`
   - `ProviderRegistry` owns initialized backends with preference-ordered routing
   - `ProviderSelector` selects the first available provider at request time
   - Compatibility translation now respects `backends.<name>.priority` from
     legacy config, mirroring `build_backend_candidates` sort semantics (rework 5)
   - Admin/runtime status exposes `configured_provider_order`, `providers[]`, and
     `current_selection`; availability field reflects circuit-breaker state (rework 5)
   - Fallback path (write-locked registry) populates `providers[]` from routing
     config with `"availability": "unknown"` (rework 4)

2. **Telegram model configuration externalized**
   - All three builtin backends (codex, gemini, claude) have `model_choices: vec![]`
   - Operators configure model choices via `telegram_config.json`
   - Precedence chain documented and tested

3. **ClawHub update flow** — `src/tizenclaw/src/core/clawhub_client.rs`
   - `clawhub_update()` reads `workspace/.clawhub/lock.json` and re-installs skills
   - Reports `updated`, `skipped`, and `failed` entries
   - One failure does not abort the full batch

4. **Skill snapshot caching** — `src/tizenclaw/src/core/skill_capability_manager.rs`
   - `SkillSnapshotCache` with `SkillSnapshotFingerprint` tracks root mtimes,
     registration, and capability-config changes
   - `invalidate_snapshot_cache` called on all clawhub install/update paths

5. **Host validation** — all tests passed via `./deploy_host.sh --test`
   (597 passed, 0 failed after fifth rework pass)

## Workflow Phases

```mermaid
flowchart LR
    refine([Refine]) --> plan([Plan])
    plan --> design([Design])
    design --> develop([Develop])
    develop --> build([Build/Deploy])
    build --> test([Test/Review])
    test --> commit([Commit])
    commit --> evaluate([Evaluate])
    evaluate -->|rework| develop
```

- [O] Stage 0. Refine — DONE
- [O] Stage 1. Plan — DONE
- [O] Stage 2. Design — DONE
- [O] Stage 3. Develop — DONE (rework pass 5: priority ordering + circuit-breaker status)
- [O] Stage 4. Build/Deploy — DONE (`./deploy_host.sh -b` PASS)
- [O] Stage 5. Test/Review — DONE (`./deploy_host.sh --test` PASS: 597; 0 failed)
- [O] Stage 6. Commit — DONE
- [O] Stage 7. Evaluate — DONE

## Risks And Watchpoints

- Provider init-time failures degrade gracefully to next available provider.
- ClawHub update failure for one entry does not abort the full batch.
- Snapshot cache fingerprint uses 1-second mtime resolution; same-second writes
  are covered by explicit `invalidate_snapshot_cache` calls on all clawhub
  operation handlers.
- Telegram model choices are empty in builtins; operators must supply them via config.
- Legacy config compatibility: `backends.<name>.priority` is now respected in
  the routing layer; existing configs using only `active_backend` /
  `fallback_backends` are unaffected (positional defaults preserved).
- `status_json()` now requires an `is_available` predicate; callers must pass
  the circuit-breaker check function to get accurate availability reporting.
