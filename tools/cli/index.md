# TizenClaw CLI Tools Index

CLI tools for the TizenClaw agent. All tools are installed under
`/opt/usr/share/tizen-tools/cli/<tool-name>/<tool-name>` and output JSON.

## Tool Summary

| Tool | Category | Description |
|------|----------|-------------|
| [tizen-app-manager-cli](#tizen-app-manager-cli) | App Management | List, launch, terminate apps; query packages; recent apps |
| [tizen-control-display-cli](#tizen-control-display-cli) | Display | Get/set display brightness |
| [tizen-device-info-cli](#tizen-device-info-cli) | Device Info | Battery, CPU, memory, storage, thermal, display, settings |
| [tizen-file-manager-cli](#tizen-file-manager-cli) | File System | Read, write, copy, move, remove, list, mkdir, download |
| [tizen-hardware-control-cli](#tizen-hardware-control-cli) | Hardware | Haptic vibration, LED flash, power lock, feedback |

| [tizen-network-info-cli](#tizen-network-info-cli) | Network | Wi-Fi, Bluetooth, network status, scan, data usage |
| [tizen-notification-cli](#tizen-notification-cli) | Notification | Send notifications, schedule alarms |
| [tizen-sensor-cli](#tizen-sensor-cli) | Sensor | Read accelerometer, gyroscope, light, proximity, etc. |
| [tizen-sound-cli](#tizen-sound-cli) | Sound | Get/set volume, list devices, play tones |
| [tizen-vconf-cli](#tizen-vconf-cli) | Configuration | Read/write/watch vconf system settings |
| [tizen-web-search-cli](#tizen-web-search-cli) | Web Search | Multi-engine web search (Naver, Google, Brave, Gemini, etc.) |

---

## tizen-app-manager-cli
Manage applications: list, launch, terminate, query packages, recent apps.

| Command | Description |
|---------|-------------|
| `list` | List installed UI apps (app_id, label, icon, package, type) |
| `list-all` | List all installed apps regardless of component type |
| `running` | List running UI apps (app_id, pid, state) |
| `running-all` | List all running apps |
| `recent` | Recently used apps via RUA (app_id, launch_time, uri, image) |
| `recent-detail --app-id <id>` | Get launch args/context for a specific recent app |
| `terminate --app-id <id>` | Terminate a running app |
| `launch --app-id <id> [--operation <op>] [--uri <uri>] [--mime <mime>]` | Launch an app via app_control |
| `package-info --package-id <id>` | Query package information |

## tizen-control-display-cli
Control display brightness directly via device API.

| Command | Description |
|---------|-------------|
| `--info` | Get current and max brightness levels |
| `--brightness <N>` | Set brightness level (0 to max_brightness) |

## tizen-device-info-cli
Query comprehensive device hardware and system information.

| Command | Description |
|---------|-------------|
| `battery` | Battery percentage, charging status, level |
| `system-info` | Model, platform version, CPU arch, screen, features |
| `runtime` | CPU usage, memory usage |
| `storage` | Storage devices, total/available space |
| `thermal` | AP/CP/Battery temperatures |
| `display` | Display count, brightness, state |
| `settings` | Locale, timezone, device name, font, sound/vibration |

## tizen-file-manager-cli
File system operations: read, write, copy, move, remove, list, download.

| Command | Description |
|---------|-------------|
| `read --path <path>` | Read file contents |
| `write --path <path> --content <data>` | Write/overwrite file |
| `append --path <path> --content <data>` | Append to file |
| `remove --path <path>` | Remove file |
| `mkdir --path <path>` | Create directory |
| `list --path <path>` | List directory entries |
| `stat --path <path>` | Get file/directory metadata |
| `copy --src <path> --dst <path>` | Copy file |
| `move --src <path> --dst <path>` | Move/rename file |
| `download --url <url> --dest <path>` | Download file from URL |

## tizen-hardware-control-cli
Control device hardware: vibration, LED, power locks, feedback patterns.

| Command | Description |
|---------|-------------|
| `haptic --duration <ms>` | Vibrate for specified milliseconds (default 500) |
| `led --action on\|off [--brightness N]` | Control camera flash LED |
| `power --action lock\|unlock --resource display\|cpu` | Lock/unlock power state |
| `feedback --pattern <NAME>` | Play feedback pattern (TAP, MESSAGE, WAKEUP, etc.) |



## tizen-network-info-cli
Query network, Wi-Fi, Bluetooth status and scan devices.

| Command | Description |
|---------|-------------|
| `network` | Connection type, IP address, proxy |
| `wifi` | Wi-Fi activation state, connected ESSID |
| `wifi-scan` | Scan Wi-Fi networks (SSID, RSSI, frequency, security) |
| `bluetooth` | BT adapter state, name, address |
| `bt-scan` | List bonded/paired Bluetooth devices (name, address, connected) |
| `data-usage` | Wi-Fi/cellular data statistics |

## tizen-notification-cli
Send user notifications and schedule alarms.

| Command | Description |
|---------|-------------|
| `notify --title <t> --body <b>` | Post a notification |
| `alarm --app-id <id> --datetime <YYYY-MM-DDTHH:MM:SS>` | Schedule an alarm |

## tizen-sensor-cli
Read real-time sensor data from device sensors.

| Command | Description |
|---------|-------------|
| `--type <sensor>` | Read sensor data. Types: accelerometer, gravity, gyroscope, light, proximity, pressure, magnetic, orientation |

## tizen-sound-cli
Control volume levels, list audio devices, play tones.

| Command | Description |
|---------|-------------|
| `volume get` | Get all volume levels |
| `volume set --type <type> --level <N>` | Set volume for a specific type |
| `devices` | List available sound devices |
| `tone --name <TONE> --duration <ms>` | Play a tone |

## tizen-vconf-cli
Read, write, and watch Tizen vconf (virtual configuration) keys.
See `tizen-vconf-cli/tool.md` for a comprehensive key reference.

| Command | Description |
|---------|-------------|
| `get <key>` | Read a vconf key value |
| `set <key> <value>` | Write a vconf key value (type auto-detected) |
| `watch <key>` | Monitor key changes in real-time (streaming) |

## tizen-web-search-cli
Multi-engine web search with AI and traditional search support.

| Command | Description |
|---------|-------------|
| `--query <Q> [--engine <E>]` | Search the web. Engines: naver, google, brave, gemini, grok, kimi, perplexity |
