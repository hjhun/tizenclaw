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
All output is JSON.
