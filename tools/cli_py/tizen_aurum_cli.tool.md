---
name: tizen-aurum-cli
description: "UI Automation: inspect screen, find UI elements, simulate input, watch accessibility events"
type: cli
command: "python3 /opt/usr/share/tizenclaw/tools/cli_py/tizen_aurum_cli.py"
---
# aurum-cli

**Description**: CLI for Aurum UI Automation — inspect screen, find UI elements, simulate input, and watch accessibility events.
**Category**: Device Control

## Subcommands

### Screen Information
| Command | Description | Example |
|---------|-------------|---------|
| `screen-size` | Get screen dimensions (width, height) | `aurum-cli screen-size` |
| `screenshot --output FILE` | Capture screenshot to file | `aurum-cli screenshot --output /tmp/shot.png` |
| `get-angle` | Get screen rotation angle | `aurum-cli get-angle` |
| `device-time` | Get device system time | `aurum-cli device-time` |

### Input Simulation
| Command | Description | Example |
|---------|-------------|---------|
| `click --x X --y Y` | Tap at coordinates | `aurum-cli click --x 360 --y 640` |
| `flick --sx X --sy Y --ex X --ey Y` | Swipe/flick gesture | `aurum-cli flick --sx 180 --sy 800 --ex 180 --ey 200` |
| `send-key --key KEY` | Hardware key press (back, home, menu, volup, voldown, power) | `aurum-cli send-key --key home` |

### Element Search
| Command | Description | Example |
|---------|-------------|---------|
| `find-element [options]` | Find single UI element | `aurum-cli find-element --text "OK"` |
| `find-elements [options]` | Find all matching elements | `aurum-cli find-elements --type "Elm_Button"` |
| `dump-tree` | Dump full UI element tree | `aurum-cli dump-tree` |

**Search Options**: `--text`, `--text-partial`, `--element-id`, `--type`, `--role`, `--automation-id`, `--package`, `--xpath`

### Element Actions
| Command | Description | Example |
|---------|-------------|---------|
| `click-element [search options]` | Click a found element | `aurum-cli click-element --text "Submit"` |
| `set-value [search options] --value V` | Set element value | `aurum-cli set-value --automation-id "slider" --value 50` |

## Output
All output is JSON.
```json
{"width": 720, "height": 1280}
```
