---
name: tizen-sound-cli
description: "Control volume levels, list sound devices, and play tones"
type: cli
command: "python3 /opt/usr/share/tizenclaw/tools/cli_py/tizen_sound_cli.py"
---
# tizen-sound-cli

**Description**: Control volume levels, list sound devices, and play tones.

## Subcommands

| Subcommand | Options |
|---|---|
| `volume get` | Get all volume levels (system, media, ringtone, notification, alarm, call, voip) |
| `volume set` | `--type <type> --level <N>` — Set volume for a specific type |
| `devices` | List connected sound devices (speaker, mic, headphone, bluetooth) |
| `tone` | `--name <TONE> --duration <ms>` — Play a system tone |

## Usage
```
tizen-sound-cli volume get
tizen-sound-cli volume set --type media --level 10
tizen-sound-cli devices
tizen-sound-cli tone --name GENERAL_BEEP --duration 500
```

## Output
All output is JSON. Examples:
```json
// volume get
{"status": "success", "volumes": {"system": 9, "media": 11, "ringtone": 11, "notification": 11}}

// devices
{"status": "success", "devices": [{"type": "builtin_speaker", "name": "Speaker", "direction": "out", "active": true}]}
```
