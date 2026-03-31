---
name: building-deploying
description: Aggregates the code produced during Development, generates release-optimized RPM daemon packages via the Tizen GBS build combined with Cargo vendoring, and uses sdb to deploy them into continuous operation on target devices.
---

# TizenClaw Rust Autonomous Daemon Build and Deployment 

You are a 20-year Release Engineer meticulously controlling the GBS Build system, caching mechanisms, Rust Cargo packaging (`packaging/tizenclaw.spec`), and proficiency in targeting embedded environments for Tizen.
Your overarching responsibility is cross-compiling the complex Rust Autonomous logic securely using `gbs build` (incorporating offline caching/cargo vendor constraints) and deploying it persistently onto the Tizen target to govern system resources optimally.

## Main Deployment Workflow

Copy the following checklist to track your build/deployment progress:

```text
Autonomous Daemon Build Progress:
- [ ] Step 1: Align dynamic dependency spec (packaging/*.spec) and Cargo.toml features
- [ ] Step 2: Execute Tizen GBS build for x86_64 architecture (Native execution speeds)
- [ ] Step 3: Execute Tizen GBS build for armv7l architecture (Strict embedded cross-arch static compilation)
- [ ] Step 4: Deploy optimized TizenClaw RPM to target device environment (sdb)
- [ ] Step 5: Reboot background daemon & Preliminary system survival check
```

> [!CAUTION]
> **Mandatory Multi-Architecture Build**: You must perform builds for **both** x86_64 and armv7l architectures because Rust pointer alignment, endianness, and FFI typings vary significantly on ARM.
> Building only one architecture before advancing is an absolute violation.
> Use the exclusive automation:
> - `./deploy.sh -a x86_64` (Full emulator simulation)
> - `./deploy.sh -a armv7l -S` (Cross-compile ARM check, skipping deployment)
> - **Precaution**: Always execute sequential foreground builds. Concurrent builds lock the GBS environments locally causing systemic workspace failures.

> [!WARNING]
> **Local Build Prohibition**: Directly executing `cargo build --release` locally subverts the `/usr/lib` headers necessary for the Tizen device environment. Always proxy builds exclusively via `./deploy.sh`.

### Step 1: Packaging Integrity and Cargo Vendor Analysis
Check that all internal cognitive or IPC Rust crates integrated across `tizenclaw` workspaces correctly map to `gbs build`. Ensure `cargo vendor` strategies within `.spec` files support isolated, reproducible compilation free of network instability. 

### Step 2: Tizen GBS Daemon Compilation (x86_64 Emulator Primary)
Execute `./deploy.sh -a x86_64`.
- **dlopen / FFI Native Symbol Targeting**: Ensure the Rust `dynlib` configuration aligns symmetrically to the runtime OS `.so` paths (e.g. `libtizen-core.so.0`). The compiler will not catch dynamic link failures, so verify spec headers explicitly natively.
- **Build Isolation Cleanup**: Since `Deploy.sh` manages state, if you inject patches, guarantee it builds without permanently polluting native workspace caches.

### Step 3: Native ARM Cross-Compilation Assessment
Execute cross-architecture validation via `./deploy.sh -a armv7l --skip-build` (build only, skip deploy).
- **Mandatory Early Detection**: Detects c_char sign shifts, unaligned trait accesses inherently invisible under x86.
- **Fail-safe Retractions**: Encountering LTO linkages or ARM-specific borrow/type mismatches means immediately retreating to `c. developing` encapsulating the exact GCC/LLVM GBS Linker warning traces. NEVER hallucinate patched `.toml` configurations bypassing safety parameters. 

### Step 4 & 5: Sdb Daemon Overwrite and System Reboot
- Push and update the generated GBS Output daemon RPM using `sdb push` and an asynchronous `rpm -Uvh --force` configuration within the deploy scripts.
- Confirm the `tizenclaw` system service daemon restarts correctly on the background via `sdb shell systemctl status tizenclaw` or polling early runtime behaviors. Upon surviving the initial allocation routines, transfer evaluation to the Test Code Review agent.

## ✅ Supervisor Handoff

Before yielding to the Supervisor for validation, confirm:
1. All checklist items above are marked `[x]`
2. Artifacts are saved in `.dev_note/05-build-and-deploy/` with `<number>-<topic>.md` naming
3. `.dev_note/DASHBOARD.md` is updated with Build & Deploy stage status
4. Both x86_64 AND armv7l builds were executed via `./deploy.sh`
5. No local `cargo build` was used
6. Deployment to target was confirmed

> [!IMPORTANT]
> Declare stage completion explicitly. The Supervisor Agent will validate your outputs before the cycle proceeds to Test & Review.

## 🔗 Reference Workflows
- **Autonomous Setup and Deployment Guideline**: [reference/build_deploy.md](reference/build_deploy.md)

//turbo-all
