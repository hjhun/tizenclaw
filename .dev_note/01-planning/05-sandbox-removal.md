# 05-sandbox-removal.md (Planning)

## Autonomous Core Requirements
- **Goal**: Remove all references, systemd units, and build artifacts related to `tizenclaw-code-sandbox`.
- **Reasoning**: The sandbox project component has become redundant since TizenClaw core now fully manages container boundaries natively via `tizenclaw-tool-executor` and `ActionBridge`.

## Module Integration Strategy
- Target structural cleanup across the Build system (`CMakeLists.txt`, `tizenclaw.spec`) and Deploy system (`deploy.sh`).
- Remove dangling `systemd` service files (`tizenclaw-code-sandbox.service`, `tizenclaw-code-sandbox-debug.service`, `tizenclaw-code-sandbox.socket`).
