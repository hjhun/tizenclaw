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
