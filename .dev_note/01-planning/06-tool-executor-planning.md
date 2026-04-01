# 06-Tool-Executor-Planning (Revision 2)

## Goal
Integrate `tizenclaw-tool-executor.service` as the primary TizenClaw tool runner and implement asynchronous event-handling to support robust tool modalities: `oneshot`, `streaming`, and `interactive` execution. Sandboxing constraints are lifted.

## Step 1: Cognitive Requirements & Target Analysis
A central executor is required because tool runs range from rapid scripts (`oneshot`), persistent log tailing (`streaming`), to interactive CLI experiences (`interactive` user prompts). The previous architecture relied on simplistic synchronous null-byte delimited sockets and direct invocations inside the LLM daemon, fundamentally breaking UX for long-running stream outputs.

We will upgrade `tizenclaw-tool-executor` and `tizenclaw-core` to use a full-duplex multiplexed JSON protocol capable of streaming standard out/err and piping standard input over the domain IPC socket dynamically.

## Step 2: Agent Capabilities & Resource Context
- **Tool Executor Service (`tizenclaw-tool-executor`)**: As an asynchronous continuous daemon (Daemon Sub-task), it must use `tokio` and non-blocking sockets. It spawns the child processes (with piped standard IO) and acts as an efficient asynchronous proxy, streaming data frames backward and forward.
- **Tizen Native Tools**: Legacy CLI binaries, shell scripts, or AI-enabled tools mapped from `/opt/usr/share/tizen-tools/cli/`.

## Step 3: Agent System Integration Planning
- **Execution Mode Classification**:
  - `Oneshot Worker`: `ContainerEngine/ToolDispatcher` makes an IPC request; executor runs to completion and exits.
  - `Streaming Event Listener`: `ContainerEngine` listens actively for `stdout/stderr` JSON frames until process termination.
  - `Interactive Mode`: Persistent background event looping sending terminal inputs directly into the CLI tool process.

### Recommended Fixes for Implementation:
1. Update `tizenclaw-tool-executor/Cargo.toml` to link `tokio`.
2. Rewrite `main.rs` to process socket I/O streams using `tokio::process` handles and multiplex JSON responses (`{event: stdout, data: ...}`).
3. Deprecate legacy socket paths and string-buffer bindings in `ContainerEngine.rs`, migrating to the new mode-based IPC interface mapping.

Status: Planning phase completed under user feedback. Awaiting Plan Approval.
