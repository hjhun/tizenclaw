---
name: tizen-hardware-control-cli
description: "Control device hardware: haptic vibration, LED flash, power locks, feedback patterns"
type: cli
command: "python3 /opt/usr/share/tizenclaw/tools/cli_py/tizen_hardware_control_cli.py"
---
# tizen-hardware-control-cli

**Description**: Control device hardware: haptic vibration, LED flash, power locks, feedback patterns.
**Category**: Hardware Control

## Subcommands

| Subcommand | Options | Description |
|---|---|---|
| `haptic` | `--duration <ms>` | Vibrate for specified milliseconds (default 500) |
| `led` | `--action on\|off [--brightness N]` | Control camera flash LED |
| `power` | `--action lock\|unlock --resource display\|cpu` | Lock/unlock power state |
| `feedback` | `--pattern <NAME>` | Play feedback pattern (TAP, MESSAGE, WAKEUP, etc.) |

## Usage
```
tizen-hardware-control-cli haptic --duration 1000
tizen-hardware-control-cli led --action on --brightness 100
tizen-hardware-control-cli power --action lock --resource display
tizen-hardware-control-cli feedback --pattern TAP
```

## Output
All output is JSON.
