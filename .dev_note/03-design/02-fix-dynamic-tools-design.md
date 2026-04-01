# Design: Fix CLI Tool Routing & Dynamic Watcher Recursion

## 1. Overview
This document specifies the architectural changes required to resolve the `ToolDispatcher` path resolution bug and finalize the recursive `ToolWatcher` subdirectory polling features. These updates guarantee accurate Tizen API mappings.

## 2. ToolDispatcher Path Resolution
- **Issue**: Extraneous markdown headers or missing frontmatter properties caused `binary:` to be empty. The fallback naively hardcodes `/usr/bin/{name}`.
- **Solution**: The fallback hierarchy strategy will be modified to support `/opt/usr/share/tizen-tools/cli/` first (which is the actual target for GBS installed `tizenclaw` plugin tools on embedded QEMU).

## 3. ToolWatcher Dynamic Polling
- **Issue**: Standard `read_dir` loop was limited to a depth of `0` or `1`, ignoring deep sub-tools pushed by SDKs.
- **Solution**: Implement `scan_dir_recursive` mapping the folder trees recursively with an upper bound depth limit of `3`. (Already evaluated as viable and memory safe, integrated into `tool_watcher.rs`).

## 4. FFI & Memory Safety
- Core functionality changes are isolated within pure Rust async state spaces. No FFI boundary crossings required for string replacement.
- Ensures lock-safety as `ToolDispatcher` is loaded into a `RwLock`.

## 5. Implementation Path
- [x] Update `src/tizenclaw/src/core/tool_watcher.rs` (Recursive recursion implemented safely via depth tracking).
- [ ] Update `src/tizenclaw/src/core/tool_dispatcher.rs` (Inject `/opt/usr/share/tizen-tools/cli` fallback).
- [ ] Execute `deploy.sh` testing to QA functionality.
