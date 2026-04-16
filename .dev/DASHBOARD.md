# DASHBOARD

## Actual Progress

- Goal: <!-- dormammu:goal_source=/home/hjhun/.dormammu/goals/tizenclaw_improve.md -->
- Prompt-driven scope: runtime flexibility roadmap (provider selection, Telegram model config, ClawHub update, skill snapshot cache, host validation)
- Current workflow phase: complete
- Last completed workflow phase: evaluate
- Supervisor verdict: `approved`
- Escalation status: `approved`
- All PLAN/TASKS items: `[O]` complete

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

## Completed Work

All five roadmap targets delivered:

1. **Provider selection layer** — `core/provider_selection.rs` introduced
   `ProviderRegistry`, `ProviderSelector`, and `ProviderCompatibilityTranslator`.
   Legacy `active_backend`/`fallback_backends` translated via compatibility path.
   Routing authority enforced: providers absent from routing config cannot be
   selected (`unwrap_or(false)` at `first_available` and `ordered_enabled_names`).

2. **Telegram model configuration** — model lists externalized from Rust
   defaults into operator config with documented precedence rule.

3. **ClawHub update flow** — `clawhub_update()` reads `workspace/.clawhub/lock.json`
   and re-installs tracked skills using locked source identity.

4. **Skill snapshot cache** — deterministic invalidation on root, registration,
   and capability-config changes added to `skill_capability_manager.rs`.

5. **Host validation** — `./deploy_host.sh --test` passed with 600 tests;
   `./deploy_host.sh` build confirmed.

## Risks And Watchpoints

- Do not overwrite existing operator-authored Markdown.
- Keep JSON merges additive so interrupted runs stay resumable.

## Review Gate — 2026-04-16

**Reviewer findings (rework pass 9) — RESOLVED**

Finding 1: `provider_selection.rs` — `ordered_enabled_names` and
`first_available` used `.unwrap_or(true)`, allowing providers absent
from the routing config to be selected as fallbacks, breaking routing
authority.
Resolution: changed to `.unwrap_or(false)` in both call sites; updated
comments; added `unconfigured_provider_excluded_from_selection` test.

Finding 2: `clawhub_client.rs` — success-path reinstall unverified.
Resolution: added `clawhub_update_success_path_installs_skill_and_updates_lock`
test that spins up an in-process axum server, verifies install dir +
SKILL.md presence, and confirms lock file version and source URL are
updated correctly.

`./deploy_host.sh --test`: 600 tizenclaw tests — all passed.
Commit: 7470229f
Supervisor verdict: RESOLVED — ready for re-review gate.
