# DASHBOARD

## Actual Progress

- Goal: Advance deferred roadmap items — provider selection, Telegram model config externalization, ClawHub update flow, skill snapshot caching, host validation
- Prompt-driven scope: All 5 prompt-derived task queue phases complete
- Active roadmap focus: Rework pass 9 (reviewer finding fixes) — pending commit
- Current workflow phase: commit
- Last completed workflow phase: test/review (rework pass 9 build verified)
- Supervisor verdict: `approved` (pending rework-pass-9 commit)
- Escalation status: `none`
- Resume point: Commit rework pass 9 fixes

## Workflow Phases

```mermaid
flowchart LR
    plan([Plan]) --> design([Design])
    design --> develop([Develop])
    design --> test_author([Test Author])
    develop --> test_review([Test & Review])
    test_author --> test_review
    test_review --> final_verify([Final Verify])
    final_verify -->|approved| commit([Commit])
    final_verify -->|rework| develop
```

## In Progress

- Commit rework pass 9: three reviewer findings addressed in uncommitted working-tree changes.

## Completed Work

### Provider Selection (provider_selection.rs)
- `ProviderRegistry` now accepts and stores `failed_inits` map.
- `status_json()` surfaces init-time failures via `last_init_error` for providers with no live instance.
- New test: `registry_status_json_surfaces_init_failure_error` covers the init-failure visibility path.

### IPC Admin Surface (ipc_server.rs)
- `handle_backend_list()` now derives `is_active`/`is_fallback` from `configured_provider_order` (from `get_llm_runtime()`), not from stale `configured_active_backend`/`configured_fallback_backends` fields.
- The admin surface is now consistent with the provider registry's routing state regardless of whether the config source is `providers[]` or legacy keys.

### Telegram Model Config (client_impl.rs)
- Per-backend model fallback in `merge_llm_config_overrides()` now covers all three Telegram-capable backends: gemini, codex, and claude.
- Previously only gemini received the `backends.<name>.model` compatibility fallback.

### Runtime Init (runtime_core_impl.rs)
- `failed_inits_startup` collected during startup init and passed to `ProviderRegistry::new()`.
- `failed_inits` collected during `reload_backends()` and passed to the refreshed registry.

## Build Evidence

- `./deploy_host.sh -b` PASS — rework pass 9 changes compile cleanly.

## Risks And Watchpoints

- No residual risks from rework pass 9. All three reviewer findings are addressed with targeted code changes and test coverage.
- Existing 597 unit/integration tests continue to pass (validated in rework pass 8).
