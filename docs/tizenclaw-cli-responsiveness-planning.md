# TizenClaw CLI Responsiveness Planning

## Step 1: Cognitive Requirements & Target Analysis
- **Goal**: Improve the perceived response latency of the `tizenclaw-cli` tool.
- **Analysis**: The daemon processes the prompts and connects to the active LLM backend successfully. However, `tizenclaw-cli` runs with `stream = false` by default, meaning that it waits until the entire token generation is complete (usually taking 1~3 seconds) before rendering anything. This blocks the CLI and produces a "slow" UX.

## Step 2: Agent Capabilities Listing and Resource Context
- Update `tizenclaw-cli` to use the real-time token streaming pipeline (`stream = true`) by default for both single-shot prompts and interactive REPL mode.
- Add an explicit `--no-stream` flag to support legacy fallback behavior if the user requires monolithic rendering.
- This mitigates the UX delay without taxing the embedded device since the daemon's IPC server already supports asynchronous pushed chunk streams.

## Step 3: Agent System Integration Planning
- **Module Convention**: Code updates primarily reside in `tizenclaw-cli` (`src/tizenclaw-cli/src/main.rs`).
- **Mandatory Execution Mode**: One-shot Worker (CLI frontend).
- **Environmental Context**: Always applicable on the Tizen target regardless of hardware.
