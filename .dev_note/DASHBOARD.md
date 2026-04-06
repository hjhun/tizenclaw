# TizenClaw Dashboard

## Current Task
- Rewrite the public README for TizenClaw in detailed English
- Add dedicated English documentation under `docs/` for:
  - repository structure
  - build, deploy, and usage flows
- Use the README style and framing of `openclaw`, `nanoclaw`, and
  `hermes-agent` as reference input without copying their product claims
- Validate the documentation refresh through `./deploy.sh -a x86_64`
- Keep local `cargo build/test/check/clippy` unused

## Stage 1: Planning
- Status: Complete
- Goal:
  - Define the public documentation refresh scope
  - Replace the stale README narrative with a product-level introduction
  - Add detailed operator docs under `docs/`
  - Planning doc: `.dev_note/docs/readme_refresh_planning.md`
- Notes:
  - Reference README files were reviewed from `openclaw`, `nanoclaw`,
    and `hermes-agent`
  - The refreshed docs must stay Tizen-first and deployment-accurate
  - Execution mode classified as documentation refresh for a daemon
    project validated through the standard `deploy.sh` path

## Stage 2: Design
- Status: Complete
- Planned changes:
  - Rebuild `README.md` around a stronger product story, quick start,
    architecture summary, and documentation map
  - Add `docs/STRUCTURE.md` explaining workspace crates and runtime
    subsystems
  - Add `docs/USAGE.md` covering prerequisites, deploy flow, service
    lifecycle, CLI usage, dashboard access, and extension points
  - Design doc: `.dev_note/docs/readme_refresh_design.md`
- Risk notes:
  - Avoid stale guidance that recommends local cargo workflows as the
    primary validation path
  - Keep crate and subsystem descriptions accurate to the current
    workspace and runtime layout
  - Present FFI and dynamic loading boundaries clearly without exposing
    unstable internal details

- Stage 1 PASS: documentation scope and execution mode are recorded in
  the dashboard and planning doc
- Stage 2 PASS: the documentation information architecture captures
  crate boundaries, FFI ownership, and dynamic loading strategy

## Stage 3: Development
- Status: Complete
- Implemented:
  - Rewrote `README.md` with a product-level overview, capability map,
    quick start, workspace summary, and documentation index
  - Added `docs/STRUCTURE.md` covering workspace crates, daemon source
    layout, runtime relationships, and repository reading order
  - Added `docs/USAGE.md` covering `deploy.sh`, CLI usage, dashboard
    control, runtime paths, configuration touchpoints, and basic
    troubleshooting guidance
  - Added `.gitignore` exceptions so the new public docs under `docs/`
    are tracked instead of silently excluded
- Stage 3 PASS: documentation deliverables are written in English and
  no local cargo build/test/check/clippy commands were used

## Stage 4: Build & Deploy
- Status: Complete
- Command:
  - `./deploy.sh -a x86_64`
- Results:
  - GBS build completed successfully for `x86_64`
  - Deploy pipeline rebuilt the RPM, installed it on
    `emulator-26101`, and restarted the service stack
  - The deploy pipeline also ran the packaged Rust test suite during
    the build flow and completed without failures
  - Dashboard frontend assets were reinstalled to
    `/opt/usr/share/tizenclaw/web`
- Service proof:
  - `tizenclaw.service`: active (running)
  - `tizenclaw-web-dashboard`: running as the child dashboard process
    on port `9090`
  - `tizenclaw-tool-executor.socket`: active (listening)
- Environment notes:
  - `sdb` reported a client/server version mismatch warning
    (`4.2.25` vs `4.2.36`) but deployment still completed successfully
- Stage 4 PASS: x86_64 build, deployment, and service restart completed
  through `./deploy.sh` without local cargo build usage

## Stage 5: Test & Review
- Status: Complete
- Build-time proof:
  - `git diff --check` passed
  - Deploy pipeline cargo tests passed inside the build environment
  - Main Rust test suite passed: `183 passed; 0 failed`
  - Core library test suite passed: `14 passed; 0 failed`
- Runtime verification:
  - `systemctl status tizenclaw --no-pager` on `emulator-26101`
    reports `active (running)` with `tizenclaw-web-dashboard`
    launched as a child process
  - `systemctl status tizenclaw-tool-executor.socket --no-pager`
    reports `active (listening)`
  - `journalctl -u tizenclaw -n 20 --no-pager` confirms the latest
    stop/start cycle completed on `2026-04-06 16:46:54 KST`
- Review verdict:
  - PASS for the documentation refresh task
  - No source or packaging regressions were introduced by the README and
    `docs/` changes
  - Residual environment warning: the device still reports the known
    `sdb` client/server version mismatch, but it did not block the task
- Stage 5 PASS: runtime evidence was captured from the target and the
  deploy pipeline remained green after the documentation refresh

## Stage 6: Commit & Push
- Status: Complete
- Commit preparation:
  - Ran `bash .agent/scripts/cleanup_workspace.sh`
  - Verified the remaining workspace changes are limited to the
    documentation refresh, tracking rule updates, and dashboard records
  - Prepared the commit message in `.tmp/commit_msg.txt` using the
    required English title/body format
- Stage 6 PASS: workspace cleanup and commit message preparation follow
  the version-management rules for final Git recording

- Previous task history:
- Add outbound delivery support for user-facing responses
- Support proactive delivery to `web_dashboard` and `telegram`
- Add a shared outbound tool path so agent flows can send updates when
  a direct reply channel is not enough
- Surface queued outbound messages inside the web dashboard UI
- Validate the new outbound flow through `./deploy.sh -a x86_64`
- Keep local `cargo` commands unused during development

## Stage 1: Planning
- Status: Complete
- Goal:
  - Define outbound delivery requirements for dashboard and Telegram
  - Keep implementation compatible with the current channel model
  - Planning doc: `.dev_note/docs/outbound_delivery_planning.md`
- Notes:
  - `telegram` can already respond to inbound chats, but proactive daemon
    delivery is not exposed as a reusable tool path
  - `web_dashboard` currently treats `send_message()` as a no-op, so it
    cannot surface pushed user notifications

## Stage 2: Design
- Status: Complete
- Planned changes:
  - Add a built-in `send_outbound_message` tool in `AgentCore`
  - Deliver Telegram outbound messages directly from bot config without
    depending on a running channel instance
  - Persist dashboard outbound messages into a device-owned queue file
  - Extend `tizenclaw-web-dashboard` with an outbound polling endpoint
    and let the SPA display new messages as toast notifications
  - Teach the `web_dashboard` channel to persist outbound messages via
    the same queue path used by the HTTP API
  - Design doc: `.dev_note/docs/outbound_delivery_design.md`
- Risk notes:
  - Keep queued dashboard messages bounded to avoid unbounded growth
  - Fail softly when Telegram is not configured instead of panicking
  - Preserve the existing pull-based dashboard chat/session flow

## Stage 3: Development
- Status: Complete
- Implemented:
  - Added the built-in `send_outbound_message` tool declaration
  - Implemented `AgentCore` outbound delivery for `web_dashboard` and
    `telegram`
  - Added a persistent dashboard outbound queue under
    `data_dir/outbound/web_dashboard.jsonl`
  - Taught the `web_dashboard` channel to persist outbound messages
    instead of ignoring `send_message()`
  - Added `GET /api/outbound/messages` to the standalone dashboard
  - Added dashboard SPA polling and toast delivery for new outbound
    messages
  - Added unit coverage for outbound tool declaration exposure and
    dashboard queue retention

- Stage 1 PASS: Outbound delivery scope recorded in dashboard and
  planning doc added under `.dev_note/docs/`
- Stage 2 PASS: Outbound delivery design captured before implementation
- Stage 3 PASS: Outbound delivery implementation completed without local
  cargo build/test usage

## Stage 4: Build & Deploy
- Status: Complete
- Command:
  - `./deploy.sh -a x86_64`
  - `./deploy.sh -a x86_64 -n`
- Results:
  - First deploy/build pass succeeded but exposed missing test imports in
    the new dashboard queue unit test during the deploy pipeline
  - Patched the test module imports in `agent_core.rs`
  - Re-ran the deploy pipeline with `-n` and completed a clean x86_64
    build, package install, and service restart
- Service proof:
  - `tizenclaw.service`: active (running)
  - `tizenclaw-web-dashboard`: running as the dashboard child process on
    port `9090`
  - `tizenclaw-tool-executor.socket`: active (listening)
- Stage 4 PASS: x86_64 deploy pipeline completed successfully through
  `./deploy.sh` without local cargo commands

## Stage 5: Test & Review
- Status: Complete
- Build-time proof:
  - `git diff --check` passed
  - `node --check data/web/app.js` passed
  - Deploy pipeline tests passed after the import fix
  - Main Rust test suite in the deploy pipeline passed:
    `183 passed; 0 failed`
- Runtime verification:
  - Confirmed the deployed service stayed healthy after redeploy on
    `emulator-26101`
  - Confirmed host `127.0.0.1:9090` is forwarded by `sdb` to the device
    dashboard service
  - Injected a verification record into the dashboard outbound queue on
    device and confirmed `GET /api/outbound/messages` returned it
  - Removed the verification queue file and confirmed the same endpoint
    returned an empty message list afterward
- Stage 5 PASS: outbound delivery path is reachable at runtime for the
  web dashboard and the deploy pipeline test suite is green

## Stage 6: Commit & Push
- Status: Complete
- Commit preparation:
  - Ran `bash .agent/scripts/cleanup_workspace.sh`
  - Verified the workspace contains only the intended source and
    dashboard tracking changes for outbound delivery support
  - Prepared the commit message via `.tmp/commit_msg.txt` in English with
    the required title/body format
- Stage 6 PASS: commit payload prepared under the version-management
  rules and ready for final Git recording

- Improve the web dashboard admin workflow
- Change the Linux host dashboard default port to `8080`
- Allow `tizenclaw-cli dashboard start --port <n>`
- Replace inline admin JSON editing with a popup editor
- Fix admin page recovery when revisiting after login
- Restore the generated web app flow
- Reconnect `tools/embedded/generate_web_app.md` to live Rust execution
- Match legacy `tizenclaw-cpp` behavior and `tizenclaw-webview` launch path
- Verify end-to-end launch with deployed `tizenclaw-webview`
- Keep host builds unaffected by Tizen-only launch support
- Route semantic dashboard app requests to `generate_web_app` even when
  the user does not explicitly say "web app"

## Historical Stage 1: Planning
- Status: Complete
- Goal:
  - Capture the web dashboard admin refresh scope in `.dev_note/docs/`
  - Keep Tizen runtime default port at `9090` while using `8080` on host
  - Support dashboard runtime port overrides from `tizenclaw-cli`
  - Fix login-session restore when the user revisits the admin page
  - Replace always-open raw JSON editing with a modal-based editor flow
  - Planning doc: `.dev_note/docs/web_dashboard_admin_refresh_planning.md`
  - Recover the legacy generated web app lifecycle in Rust
  - Support app file generation under `/web/apps/<app_id>`
  - Restore dashboard-side app listing/detail/delete and Bridge API access
  - Reuse `tizenclaw-webview` launch behavior when available
- Notes:
  - `tools/embedded/generate_web_app.md` exists but runtime handling is missing
  - Rust `tizenclaw-web-dashboard` currently exposes only app list/detail
  - Legacy C++ implementation writes `manifest.json`, downloads assets,
    exposes bridge endpoints, and auto-launches bridge/webview apps

## Historical Stage 2: Design
- Status: Complete
- Planned changes:
  - Add a runtime-aware `default_dashboard_port()` helper
  - Thread optional `settings` into `start_channel` so the dashboard can
    accept CLI port overrides
  - Add `GET /api/auth/session` and signed admin tokens to restore
    authenticated views after dashboard page revisits or process restart
  - Redesign the admin configuration UI into summary cards plus a modal
    with structured and raw editing modes
  - Design doc: `.dev_note/docs/web_dashboard_admin_refresh_design.md`
  - Add `generate_web_app` builtin declaration to workflow tools
  - Implement web app generation in `AgentCore` with manifest/assets support
  - Add IPC methods so `tizenclaw-web-dashboard` can execute bridge tools
    and enumerate allowed tools through the daemon
  - Extend `tizenclaw-web-dashboard` with `/api/apps` delete and
    `/api/bridge/{tool,tools,data,chat}` endpoints
  - Keep SSE bridge events out of scope unless required by build/test
- Risk notes:
  - Preserve current standalone dashboard process model
  - Keep path validation strict to avoid traversal via app ids or filenames
  - Use best-effort Tizen app launch so non-Tizen host paths do not panic

## Historical Stage 3: Development
- Status: Complete
- Implemented:
  - Added host-aware dashboard default port resolution and runtime channel
    configuration plumbing for custom port overrides
  - Added admin session validation endpoints and restart-stable signed
    dashboard tokens
  - Replaced inline admin config editing with summary cards and
    a modal editor workflow
  - Added localStorage-backed admin token restore and modal UX updates in
    English for global maintainability
  - Added `generate_web_app` builtin declaration back into workflow tools
  - Restored Rust-side web app generation in `AgentCore`
  - Added manifest writing, optional asset download, and app listing metadata
  - Added bridge IPC methods for tool execution and tool enumeration
  - Extended web dashboard with `/api/apps/:id` delete and
    `/api/bridge/{tool,tools,data,chat}` endpoints
  - Added bridge compatibility alias for `execute_cli` so legacy
    generated apps keep working without host-only build fallout
  - Kept Tizen-only app launch as best-effort runtime behavior guarded
    by Tizen environment detection

## Historical Stage 4: Build & Deploy
- Status: Complete
- Command:
  - `./deploy.sh -a x86_64 -d emulator-26101`
  - `~/samba/github/tizenclaw-webview/deploy.sh -d emulator-26101`
- Results:
  - Updated dashboard/admin changes built successfully through GBS
  - Device deployment succeeded on `emulator-26101`
  - Dashboard frontend assets were pushed to
    `/opt/usr/share/tizenclaw/web`
  - Service restart succeeded and the dashboard child process relaunched
    on device
  - GBS build succeeded
  - Device deployment succeeded
  - Service restart succeeded
  - Companion webview app build/deploy succeeded after aligning
    pkg-config dependency with `chromium-efl`
- Service proof:
  - `tizenclaw.service`: active (running)
  - `tizenclaw-tool-executor.socket`: active (listening)

## Historical Stage 5: Test & Review
- Status: Complete
- Build-time test proof:
  - `node --check data/web/app.js` passed locally for SPA syntax
  - `cargo test --release --offline -- --test-threads=1` ran inside
    the deploy pipeline
  - Main test suite passed: `182 passed; 0 failed`
  - `git diff --check` passed after edits
- Runtime verification:
  - Confirmed `GET /api/status` on device returns
    `{"channels":"active","status":"running","version":"1.0.0"}`
  - Confirmed `GET /api/config/list` without auth returns `401
    Unauthorized`
  - Confirmed admin login on device returns a bearer token from
    `POST /api/auth/login`
  - Confirmed that bearer token succeeds on both `GET /api/auth/session`
    and `GET /api/config/list`
  - Confirmed `tizenclaw-cli dashboard stop` followed by
    `tizenclaw-cli dashboard start --port 9091` relaunches the dashboard
    process with `--port 9091`
  - Confirmed the same bearer token remains valid after dashboard
    process restart via `GET /api/auth/session`, proving the
    revisit/login-recovery fix survives process reset
  - Restored the dashboard child process to device default port `9090`
    and confirmed `GET /api/status` works again on `127.0.0.1:9090`
  - Created `webtest_demo` through direct daemon `bridge_tool` IPC
    using `generate_web_app`
  - Confirmed app files under `/opt/usr/share/tizenclaw/web/apps/webtest_demo`
  - Confirmed manifest persisted `allowed_tools: ["execute_cli"]`
  - Confirmed `GET /api/apps` and `GET /api/apps/webtest_demo` return
    the generated app metadata
  - Confirmed `GET /api/bridge/tools?app_id=webtest_demo` returns
    legacy-compatible `execute_cli`
  - Confirmed `POST /api/bridge/tool` successfully ran
    `tizen-device-info-cli battery`
  - Confirmed `POST/GET /api/bridge/data` round-trip works for
    `app_id=webtest_demo`, `key=mode`
  - Deployed and registered `org.tizen.tizenclaw-webview` on
    `emulator-26101`
  - Verified manual launch with
    `app_launcher -s org.tizen.tizenclaw-webview __APP_SVC_URI__ ...`
    succeeds on device
  - Updated Rust launch flow to prefer legacy-compatible AUL bundle launch
    for `QvaPeQ7RDA.tizenclawbridge` and `org.tizen.tizenclaw-webview`
  - Created `webtest_autorun2` via daemon IPC after redeploy and verified
    the webview process auto-starts with
    `__APP_SVC_URI__=http://localhost:9090/apps/webtest_autorun2/`
  - Verified `dlogutil` from `TIZENCLAW_WEBVIEW` reports the generated app
    URI, proving auto-launch is wired through at runtime
  - Confirmed non-Tizen host path now falls back to a user-facing host URL
    message instead of depending on Tizen launch support
  - Identified a generated app regression where separate `css` and `js`
    files were written but not auto-linked into `index.html` when the
    model omitted explicit `<link>` or `<script>` tags
  - Fixed `generate_web_app` so generated apps under
    `/opt/usr/share/tizenclaw/web/apps/<app_id>/` auto-inject
    `style.css` and `app.js` references when needed
  - Hardened tool guidance so browser UI requests prefer
    `generate_web_app` instead of `run_generated_code`
  - Added semantic dashboard request detection so creation and update
    prompts like games, screens, dashboards, and UI edits are routed to
    `generate_web_app` without requiring explicit "web app" wording
  - Added a fallback that parses assistant JSON app specs and executes
    `generate_web_app` server-side when the model returns app payloads as
    plain text instead of calling the tool directly
  - Verified `/api/apps/<id>` delete removes the app directory from
    `/opt/usr/share/tizenclaw/web/apps/<app_id>` using `webtest_delete`
  - Re-verified on `emulator-26101` that a prompt asking for a
    Tetris game without saying "web app" created
    `/opt/usr/share/tizenclaw/web/apps/webtest_semantic_a`
  - Re-verified that a follow-up "modify/improve" prompt for the same
    `app_id` updated `index.html`, relaunched webview, and reflected the
    requested `Score Board` text
  - Re-verified that `DELETE /api/apps/webtest_semantic_a` removed the
    generated app directory and removed it from `/api/apps`
- Runtime log proof:
  - `tizenclaw.service`: active (running)
  - `tizenclaw-tool-executor.socket`: active (listening)

## Supervisor Gate Log
- Stage 1 PASS: Admin refresh planning scope recorded in dashboard and
  planning doc added under `.dev_note/docs/`
- Stage 2 PASS: Admin refresh design captured before implementation
- Stage 3 PASS: Admin refresh implementation completed without local
  cargo build/test usage
- Stage 4 PASS: Updated dashboard/admin flow built and deployed on
  `emulator-26101`
- Stage 5 PASS: Admin auth restore, CLI port override, and runtime
  dashboard responses verified on device
- Stage 1 PASS: Web app restoration scope recorded in dashboard
- Stage 2 PASS: Rust/CPP parity design captured before edits
- Stage 3 PASS: Rust web app generation and bridge routes restored
- Stage 4 PASS: x86_64 deploy completed on emulator-26101
- Stage 5 PASS: Generated app, bridge API, and webview auto-launch
  verified on device
  Additional PASS: app delete API removes generated app directories under
  the legacy C++-compatible web apps path
  Additional PASS: semantic dashboard prompts now route to web app
  generation and update flows without explicit "web app" wording
  Additional PASS: semantic update prompts modified the generated app and
  app delete removed the device files and dashboard listing

## Historical Stage 6: Commit & Push
- Status: Complete
- Notes:
  - Workspace cleaned with `.agent/scripts/cleanup_workspace.sh`
  - Added ignored `.dev_note/docs/web_dashboard_admin_refresh_*` files
    with `git add -f` so the stage artifacts are preserved in history
  - Prepared commit message in `.tmp/commit_msg.txt` for
    `git commit -F .tmp/commit_msg.txt`
  - Commit captures the dashboard admin refresh, auth restore, and CLI
    dashboard port override updates
  - Previous restoration commit: `e57fb0da`
    `Restore generated web app bridge flow`
  - Workspace cleaned with `.agent/scripts/cleanup_workspace.sh`
  - Removed untracked local `.pc` artifacts before staging
  - Removed tracked local CLI ELF binaries from `tools/cli/*` and added
    ignore rules so regenerated executables do not get committed again
  - Selected only semantic routing and IPC cleanup files for commit to
    avoid unrelated local `tools/cli/*` changes
  - Commit created with `.tmp/commit_msg.txt` and `git commit -F`
    following repository message rules
  - Commit stage prepared for branch-local completion without push
    because the user requested commit creation only
  - Stage 6 PASS: workspace cleaned, extraneous files removed, and
    commit prepared with `.tmp/commit_msg.txt`
