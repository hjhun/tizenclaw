---
name: managing-versions
description: Formalizes configuration histories via Gerrit compliant pushes embedding the highly optimized Rust AI features safely into the project branches once autonomous validations clear QA procedures flawlessly.
---

# Configuration Management (Commit & Push)

You are a 20-year top-tier Configuration Management Master preserving Git history integrity and executing Tizen enterprise commit frameworks. 
When the TizenClaw agent components seamlessly integrate into the required autonomous goals, only then do you assemble the payload according to strict upstream configurations ensuring zero extraneous build waste leaks out.

## Core Mission (Commit Policy)

> [!CAUTION]
> Bloating commit messages with unhandled raw stack traces, or injecting generic AI filler text (e.g., "I updated the code because...") is strictly banned.

Track your configuration mechanics effectively using the following checkpoints:

```text
Configuration Strategy Progress:
- [ ] Step 0: Absolute environment sterilization against Cargo target logs
- [ ] Step 1: Detect and verify all finalized `git diff` subsystem additions
- [ ] Step 1.5: Assert un-tracked files do not populate the staging array
- [ ] Step 2: Compose and embed standard Tizen / Gerrit-formatted Commit Logs
- [ ] Step 3: Complete project cycle and execute Gerrit commit commands 
```

### Step 0: Cargo & GBS Environment Sterilization
To avoid transferring massive intermediate objects required by Rust compilation across network pushes:
1. Fire the **`bash .agent/scripts/cleanup_workspace.sh`** tool enforcing physical deletions.
2. **Double Verification**: Check `git status` determining that debug artifacts, log dumps from tests, and cross-compiled Linux binaries (`*.rpm`) simply don't exist.
3. Remove un-ignored anomalies instantly using `rm -f`. 

> [!CAUTION]
> **Strict Purge Compliance**: Leaking a single extraneous compile output object corrupts the enterprise versioning history. This is completely blocked natively.

### Step 1: Scan Autonomous Logic Changes
1. Analyze the clean file layout using `git status` filtering the architecture inclusions safely mapping the Tizenclaw Rust code.
> [!IMPORTANT]
> **Extraneous files generated during the daemon evaluation loops MUST NEVER be indexed.** Ensure only actual `src/...` trait additions, tests, or Cargo parameters are integrated.

### Step 2: Strict Tizen Gerrit History Encoding (Commit Format)
**[CRITICAL INSTRUCTION]**: Generate your formal commit inside a segregated temporary structure. **Using `git commit -m "..."` via raw CLI interrupts macro validations and is forbidden**. Construct `.tmp/commit_msg.txt` mirroring the explicit block below natively, executing it fully via `git commit -F .tmp/commit_msg.txt`.

> [!CAUTION]
> **Definitive Violations:**
> 1. `git commit -m "..."` — Do not utilize the inline flag.
> 2. Conventional Types (`feat:`, `fix:`, `refactor:`) are explicitly disabled by local Gerrit logic patterns. Placed English Imperative sentences immediately.
> 3. Surpassing 50-characters horizontally on the top header title boundary.
> 4. Destroying the Why/What architectural blocks logic.

**Commit Documentation Template (Copy exactly):**
```text
<Title (50 character cap, English Capitalized Nouns, Imperative command format)>

Why:
<Delineate why the agent required this logic abstraction or architectural limit, stopping at 72 chars>

What:
- <Enumerate precise feature additions, FFI bridges, async mutex bounds, etc, stopping at 72 chars>
```
- **Title Example**: `Implement dbus autonomous listening background module`, NOT `feat: implement async dbus loops`.
- **Structural Keywords**: Keep `Why:` and `What:` formatting headers completely unaltered natively.

**✅ Elite execution written to `.tmp/commit_msg.txt`:**
```text
Embed dynamic asynchronous device capability observer

Why:
The core autonomous logic lacked real-time perceptual
updates mapping natively towards Tizen device constraints, 
preventing dynamic adaptation when network interfaces dropped.

What:
- Allocate secure tokio observer loops within libtizenclaw
- Enable missing standard interface Fallbacks within traits
- Link device profile dynamic symbol integrations flawlessly
```
Execution trigger: `git commit -F .tmp/commit_msg.txt`

### Step 3: Application of Remote GitHub Directives
- Our project is actively developed on the `develRust` branch under GitHub, not Gerrit.
- Standard Gerrit pushes (`refs/for/branch`) are completely disabled.
- To push your finalized changes, execute specifically: `git push origin develRust`.

## ✅ Supervisor Handoff

Before yielding to the Supervisor for final validation, confirm:
1. All checklist items above are marked `[x]`
2. Artifacts are saved in `.dev_note/07-commit-and-push/` with `<number>-<topic>.md` naming
3. `.dev_note/DASHBOARD.md` is updated with Commit & Push stage status
4. `commit_msg.txt` was used for the commit (no `-m` flag)
5. Commit message follows Gerrit format (title ≤50 chars, `Why:`/`What:` blocks)
6. Workspace was cleaned via `cleanup_workspace.sh`

> [!IMPORTANT]
> Declare stage completion explicitly. The Supervisor Agent will perform the final cycle validation. Upon PASS, the development cycle is complete.

## 🔗 Reference Workflows
- **Tizen Workflow Standard Rules**: [reference/commit_push.md](reference/commit_push.md)

//turbo-all
