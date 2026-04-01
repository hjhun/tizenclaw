# 04-startup-indexing.md (Architecture Design)

## Traits and Structural Strategy
- The logic extends the `AgentCore` module exclusively.
- We will add `pub async fn run_startup_indexing(&self)` to evaluate connections safely.
- We utilize `tokio::spawn` encapsulating an `Arc<AgentCore>` to fire `agent.process_prompt(...)` non-blockingly during the start sequence (`main.rs`).

## FFI Boundary Profile
- No new FFI bridges needed. Native API boundaries rely on the existing tool definitions in the Agent (`execute_code`, `file_manager`).

## Sync / Async Tokio Maps
- Will wait for `main.rs` to reach Phase 8 (all managers initialized).
- Use `tokio::time::sleep(tokio::time::Duration::from_secs(3)).await` inside the spawn to ensure D-Bus and file permissions stabilize completely.
- `AgentCore::process_prompt` inherently locks memory safely avoiding data races.
