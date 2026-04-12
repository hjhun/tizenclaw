[O] Phase 1. Re-read the required guidance, inspect `.dev/SCORE.md`, and
    document the current host-default resume decision plus the root cause of
    the failed verification in `.dev/DASHBOARD.md`.
[O] Phase 2. Finish the generic research-validation implementation and any
    needed runtime/unit coverage without adding PinchBench-specific logic.
[O] Phase 3. Deploy the current host build with `./deploy_host.sh` and record
    daemon/OAuth health in `.dev/DASHBOARD.md`.
[O] Phase 4. Run `./deploy_host.sh --test` plus the required
    `tizenclaw-tests` runtime-contract scenarios, then record the review
    evidence in `.dev/DASHBOARD.md`.
[ ] Phase 5. Re-run the host OpenAI OAuth PinchBench slice, update
    `.dev/SCORE.md`, synchronize `.dev/DASHBOARD.md`, and complete the commit
    stage only if the verified score is `>=95%`.
