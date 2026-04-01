# 04-startup-indexing.md (Planning)

## Autonomous Core Requirements
- **Goal**: Establish a startup phase where the TizenClaw daemon executes a self-organizing LLM prompt over `/opt/usr/share/tizen-tools/`.
- **Mitigation Strategy**: Only trigger if LLM active backend is online (avoids infinite loops or crashes).

## Agent Capabilities Listing
- **Inputs Required**: 
  - Valid LLM connection (`self.backend.is_some()`).
  - Read-write files in `/opt/usr/share/tizen-tools/`.
- **Outputs Generated**: `tools.md`, `index.md` refreshed via LLM file operations.

## Module Integration Strategy
- **Module Convention**: `tizenclaw-core`.
- **Execution Mode**: **Daemon Sub-task**. (Asynchronous Tokio spawn right after agent initialization, executing a single prompt transaction).
- **Environment**: If offline, skip seamlessly.
