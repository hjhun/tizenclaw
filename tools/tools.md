# TizenClaw Tool Catalog

This document provides a consolidated index of all available tools.
For detailed usage, refer to each category's `index.md` file.

## CLI Tools

See [cli/index.md](cli/index.md) for the full list of native CLI tools.

CLI tools are pre-built native executables installed under
`/opt/usr/share/tizen-tools/cli/<tool-name>/` that output JSON.

Available CLI tools:
- **tizen-app-manager-cli** — App Management (list, launch, terminate, packages, recent apps)
- **tizen-aurum-cli** — UI Automation (screen inspect, element find, input simulation, event watch)
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

Skills are Python-based tools executed inside a secure OCI container sandbox.
The `skills/index.md` file is auto-generated at runtime based on installed skill manifests.

## Custom Skills

Custom skills are user-defined or AI-generated Python scripts added at runtime.
The `custom_skills/index.md` file is auto-generated when custom skills are installed.

## System CLI Tools

System CLI tools are host-level tools registered via `tizenclaw-cli --register-tool`.
The `system_cli/index.md` file is auto-generated based on registered tools.
