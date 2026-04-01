# TizenClaw Tool Catalog

This document provides a consolidated index of all available tools.
For detailed usage, refer to each category's `index.md` file.

## CLI Tools

See [cli/index.md](cli/index.md) for the full list of native CLI tools.

CLI tools are pre-built native executables installed under
`/opt/usr/share/tizen-tools/cli/<tool-name>/` that output JSON.

Available CLI tools:
- **tizen-app-manager-cli** — App Management (list, launch, terminate, packages, recent apps)
- **tizen-control-display-cli** — Display brightness control
- **tizen-device-info-cli** — Device information (battery, CPU, memory, storage, thermal, display, settings)
- **tizen-file-manager-cli** — File system operations (read, write, copy, move, remove, list, mkdir, download)
- **tizen-hardware-control-cli** — Hardware control (haptic, LED, power lock, feedback)
- **tizen-network-info-cli** — Network information (WiFi, Bluetooth, connection, data usage)
- **tizen-notification-cli** — Notifications and alarms
- **tizen-sensor-cli** — Sensor data (accelerometer, gyroscope, light, proximity, etc.)
- **tizen-sound-cli** — Sound control (volume, devices, tones)
- **tizen-vconf-cli** — VConf key read/write/watch
- **tizen-web-search-cli** — Multi-engine web search

## Embedded Tools

See [embedded/index.md](embedded/index.md) for built-in embedded tools.

## Skills

Skills are Python/Bash-based scripts executed inside a secure OCI container sandbox.
The Agent manages Skills via the `/opt/usr/share/tizen-tools/skills/` directory.

We strictly adhere to the **Anthropic Model Context Protocol (MCP)** and Tool Use standards. All tools and skills manifest as correctly structured JSON parameters ingested by the agent's LLM Backend.

The `skills/index.md` file (and this file) are auto-generated or dynamically injected at runtime when the `ToolWatcher` detects structural changes in the RW tool directories. This guarantees real-time synchronization between the filesystem and the LLM systemic prompt.

## System CLI Tools

System CLI tools are host-level tools registered via `tizenclaw-cli --register-tool`.
The `system_cli/index.md` file is auto-generated based on registered tools.

## Device Actions

See [actions/index.md](actions/index.md) for the dynamically generated list of native Tizen actions.

Device Actions are native Tizen platform features provided by the Action Framework.
The `actions/index.md` file and its contents are auto-generated at runtime when action schemas are synced by `ActionBridge`. They handle core device control such as display brightness, volume, network toggles, and more.
