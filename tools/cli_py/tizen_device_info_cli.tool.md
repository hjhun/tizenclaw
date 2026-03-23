---
name: tizen-device-info-cli
description: "Query device hardware and system information: battery, system-info, runtime, storage, thermal, display, settings"
type: cli
command: "python3 /opt/usr/share/tizenclaw/tools/cli_py/tizen_device_info_cli.py"
---
# tizen-device-info-cli

**Description**: Query device hardware and system information.
**Category**: Device Info

## Subcommands

| Subcommand | Description |
|---|---|
| `battery` | Battery percentage, charging status, level |
| `system-info` | Model, platform version, CPU arch, screen, features |
| `runtime` | CPU usage, memory usage |
| `storage` | Storage devices, total/available space |
| `thermal` | AP/CP/Battery temperatures |
| `display` | Display count, brightness, state |
| `settings` | Locale, timezone, device name, font, sound/vibration |

## Usage
```
tizen-device-info-cli battery
tizen-device-info-cli system-info
tizen-device-info-cli runtime
tizen-device-info-cli storage
tizen-device-info-cli thermal
tizen-device-info-cli display
tizen-device-info-cli settings
```

## Output
All output is JSON. Examples:
```json
// battery
{"status": "success", "percent": 85, "charging": true, "level": "normal"}

// system-info
{"status": "success", "model": "emulator", "platform_version": "10.0", "cpu_arch": "x86_64"}

// runtime
{"status": "success", "cpu_usage": 12.5, "memory_total": 2048, "memory_available": 1024}
```
