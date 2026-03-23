---
name: tizen-control-display-cli
description: "Manage display brightness: get info, set brightness level"
type: cli
command: "python3 /opt/usr/share/tizenclaw/tools/cli_py/tizen_control_display_cli.py"
---
# tizen-control-display-cli

**Description**: Native CLI tool to manage display brightness.
**Category**: Hardware Control

## Options

| Option | Description | Example |
|--------|-------------|---------|
| `--brightness B` | Brightness level to set (integer, 0 to max_brightness). Use `--info` first to check max brightness. | `tizen-control-display-cli --brightness 50` |
| `--info` | Get current and max brightness levels. | `tizen-control-display-cli --info` |

## Usage
```
tizen-control-display-cli --info
tizen-control-display-cli --brightness 50
```

## Output
All output is JSON. Examples:
```json
// Setting brightness
{"status": "success", "brightness_set": 50, "max_brightness": 100}

// Getting info
{"status": "success", "current_brightness": 50, "max_brightness": 100}
```
