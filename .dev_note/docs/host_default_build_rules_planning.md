## Planning Progress (Autonomous Core)
- [x] Step 1: Analyze the requested build-rule change and identify the
  affected rule/skill files
- [x] Step 2: Define the default execution mode and fallback conditions
- [x] Step 3: Record the documentation and verification scope in
  `.dev_note/`

# Host Default Build Rules Planning

## Goal
Change the project's default development build workflow so that, unless
the user explicitly asks otherwise, agents develop against
`./devel_host.sh` instead of the Tizen `./deploy.sh` path.

## Scoped Fix Items
1. Update top-level repository rules to describe host-first development.
2. Update environment, development, build/deploy, review, and supervisor
   skills so their default command path is `./devel_host.sh`.
3. Preserve the explicit Tizen fallback path for emulator/device work
   when the user asks for it.
4. Remove rule text that incorrectly forbids all local Cargo activity
   for host-default work.
5. Ensure the new default command actually exists in the repository.

## Planned Execution Policy
| Context | Default command | Override condition |
|---|---|---|
| Everyday development without extra user instruction | `./devel_host.sh` | None |
| Tizen emulator/device packaging, deployment, or device validation | `./deploy.sh` | User explicitly requests Tizen/GBS/device flow |
| Commit/push housekeeping | Existing git workflow | Unchanged |

## Integration Objectives
- `AGENTS.md`
- `.agent/rules/*.md`
- `.agent/skills/*/SKILL.md`
- `.agent/skills/*/reference/*.md`
- `CLAUDE.md`
- `devel_host.sh`

## Environmental Notes
- This cycle is a host-default workflow transition, not a Tizen device
  feature change.
- Verification will focus on rule consistency plus host entry-point
  validity.

## Stage Completion
Planning stage is complete for the host-default build-rule transition.
