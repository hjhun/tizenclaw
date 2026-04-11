# Runtime Topology And Registry Plan

[O] Phase 1. Establish the comparative runtime baseline across
`tizenclaw`, `openclaw`, `nanoclaw`, and `openclaude`
[O] Phase 2. Select the clean architecture for topology ownership,
registration metadata, and IPC visibility
[O] Phase 3. Define the TDD contract, logging strategy, and
`tizenclaw-tests` scenario updates for the runtime-visible slice
[O] Phase 4. Extend the same ownership model into agent-loop control,
resume state, and broader observability seams
[O] Phase 5. Refactor memory and session persistence to align with the
runtime topology contract
[ ] Phase 6. Rebuild tool and skill loading around richer capability
activation and registry metadata
[O] Phase 7. Complete host-first build, deploy, review, and commit
preparation for this implementation slice

## Active Slice

- [O] Add a daemon-facing runtime topology contract
- [O] Persist typed registration entries with compatibility retention
- [O] Expose topology and registration metadata through IPC
- [O] Add unit tests and a `tizenclaw-tests` scenario for the new payload
- [O] Validate through `./deploy_host.sh`, IPC smoke, and
  `./deploy_host.sh --test`
- [O] Expose session runtime control-plane and resume metadata through
  IPC-backed loop snapshots
- [O] Expose memory persistence and session context-flow metadata
  through `get_session_runtime`
