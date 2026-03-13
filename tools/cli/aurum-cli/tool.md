# aurum-cli

**Description**: Native CLI for Aurum UI Automation — inspect screen, find UI elements, simulate input, and watch accessibility events.  
**Category**: Device Control

## Usage

```
aurum-cli <subcommand> [options]
aurum-cli --grpc <subcommand> [options]
aurum-cli --grpc --grpc-addr HOST:PORT <subcommand> [options]
```

## Modes

- **Default (libaurum)**: Direct AT-SPI2 access via `libaurum`. No server needed but requires accessibility bus.
- **gRPC (`--grpc`)**: Connects to `aurum-bootstrap` gRPC server (default: `localhost:50051`). Use `--grpc-addr` to specify a custom address.

## Subcommands

### Screen Information
| Command | Description | Example |
|---------|-------------|---------|
| `screen-size` | Get screen dimensions (width, height) | `aurum-cli screen-size` |
| `screenshot --output FILE` | Capture screenshot to file | `aurum-cli screenshot --output /tmp/shot.png` |
| `get-angle` | Get screen rotation angle | `aurum-cli get-angle` |
| `device-time [--type wallclock\|monotonic]` | Get device system time | `aurum-cli device-time` |

### Input Simulation
| Command | Description | Example |
|---------|-------------|---------|
| `click --x X --y Y [--duration MS]` | Tap or long-press at coordinates | `aurum-cli click --x 360 --y 640` |
| `flick --sx X --sy Y --ex X --ey Y [--steps N] [--duration MS]` | Swipe/flick gesture | `aurum-cli flick --sx 180 --sy 800 --ex 180 --ey 200` |
| `send-key --key KEY [--action ACTION]` | Hardware key press (back, home, menu, volup, voldown, power) | `aurum-cli send-key --key home` |
| `touch-down --x X --y Y` | Touch down (returns seqId) | `aurum-cli touch-down --x 100 --y 100` |
| `touch-move --x X --y Y --seq-id ID` | Move touch | `aurum-cli touch-move --x 200 --y 200 --seq-id 0` |
| `touch-up --x X --y Y --seq-id ID` | Release touch | `aurum-cli touch-up --x 200 --y 200 --seq-id 0` |

### Element Search
| Command | Description | Example |
|---------|-------------|---------|
| `find-element [options]` | Find single UI element | `aurum-cli find-element --text "OK"` |
| `find-elements [options]` | Find all matching elements | `aurum-cli find-elements --type "Elm_Button"` |
| `dump-tree` | Dump full UI element tree | `aurum-cli dump-tree` |

**Search Options**: `--text`, `--text-partial`, `--element-id`, `--type`, `--role`, `--automation-id`, `--package`, `--xpath`, `--description`, `--is-visible`, `--is-enabled`, `--is-focused`, `--is-clickable`, `--is-checked`

### Element Actions
| Command | Description | Example |
|---------|-------------|---------|
| `click-element [search options]` | Click a found element | `aurum-cli click-element --text "Submit"` |
| `set-focus [search options]` | Set focus to element | `aurum-cli set-focus --automation-id "input1"` |
| `do-action [search options] --action NAME` | Execute AT-SPI action | `aurum-cli do-action --text "Menu" --action activate` |
| `set-value [search options] --text-value T\|--value V` | Set element text or numeric value | `aurum-cli set-value --automation-id "slider" --value 50` |

### Event Watching
| Command | Description | Example |
|---------|-------------|---------|
| `wait-event --event E [--timeout MS] [--package P]` | Wait for a specific event | `aurum-cli wait-event --event WINDOW_ACTIVATE --timeout 5000` |
| `watch --event E [--timeout MS]` | Persistent event monitoring (outputs JSON per event) | `aurum-cli watch --event STATE_CHANGED_FOCUSED --timeout 30000` |

**Events**: `WINDOW_ACTIVATE`, `WINDOW_DEACTIVATE`, `WINDOW_MINIMIZE`, `WINDOW_RAISE`, `STATE_CHANGED_FOCUSED`, `STATE_CHANGED_VISIBLE`, `STATE_CHANGED_CHECKED`

## Output

All output is JSON. Examples:

```json
// screen-size
{"width": 720, "height": 1280}

// find-element
{"id": "...", "text": "OK", "type": "Elm_Button", "role": "push button", "geometry": {"x": 100, "y": 200, "width": 80, "height": 40}, ...}

// click
{"success": true, "x": 360, "y": 640}
```
