## Architecture Design (Agent Core) Progress
- [x] Step 1: Review the current host and Tizen execution paths
- [x] Step 2: Define the default-vs-override command routing rules
- [x] Step 3: Align rule, skill, and supervisor language with the new
  default behavior
- [x] Step 4: Define a compatibility path for the new host entry point

# Host Default Build Rules Design

## Design Goal
Make host development the default agent workflow while keeping Tizen
deployment available as an explicit opt-in path.

## Structural Design
1. **Default Build Rule**
   - Treat `./devel_host.sh` as the default development command for
     build, run, and test activities.
   - Phrase Tizen `./deploy.sh` as a targeted packaging/deployment path,
     not the universal default.
2. **Command Routing**
   - Rules and skills must say "use `./devel_host.sh` unless the user
     explicitly requests Tizen/emulator/device validation."
   - Shell examples should include both host and Tizen cases so the
     environment rule remains actionable.
3. **Supervisor Criteria**
   - Development/build/review gates should accept host-default evidence
     for ordinary cycles.
   - Tizen-only evidence remains mandatory only when the task scope is
     explicitly device-oriented.
4. **Compatibility Wrapper**
   - Add `devel_host.sh` as the stable host-facing entry point.
   - Keep `deploy_host.sh` as the implementation backend so existing
     scripts and habits do not break immediately.

## Runtime Boundary Statement
- No Rust runtime behavior changes are required for this cycle.
- No new FFI boundary is introduced.
- This is an operational policy and tooling-entrypoint update only.

## Expected Verification
- `devel_host.sh` should be executable and forward to the existing host
  workflow.
- Rule/skill texts should consistently describe host-first development.
- Tizen-specific instructions should remain available as an explicit
  override path.

## Stage Completion
Design stage is complete for the host-default build-rule transition.
