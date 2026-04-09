# robotic-vacuum-cli
**Description**: Control Samsung Jet Bot robot vacuum via SmartThings: start/stop cleaning, dock, adjust suction, get status.
**Category**: Home Automation

## Subcommands
| Subcommand | Options | Description |
|---|---|---|
| `start` | `--mode auto\|part\|repeat\|manual\|map` | Start cleaning cycle in specified mode (default: auto) |
| `stop` | | Stop cleaning and leave vacuum in place |
| `pause` | | Pause current cleaning cycle |
| `dock` | | Return to charging dock |
| `status` | | Get battery level, movement state, cleaning mode, turbo mode |
| `turbo` | `--level on\|off\|silence` | Set suction power level (default: on) |

## LLM Agent Instructions
**CRITICAL**: Pass exactly ONE subcommand as the first positional argument. Do NOT pass credentials or device ID as arguments — they are read from `/opt/usr/share/tizenclaw/data/config/robotic_vacuum_config.json` automatically.

Example: `start --mode auto`
Example: `start --mode part`
Example: `status`
Example: `dock`
Example: `stop`
Example: `pause`
Example: `turbo --level on`
Example: `turbo --level silence`
