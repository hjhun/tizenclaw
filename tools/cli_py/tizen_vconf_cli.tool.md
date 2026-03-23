---
name: tizen-vconf-cli
description: "Read, write, and monitor Tizen vconf (virtual configuration) keys"
type: cli
command: "python3 /opt/usr/share/tizenclaw/tools/cli_py/tizen_vconf_cli.py"
---
# tizen-vconf-cli

Read, write, and monitor Tizen vconf (virtual configuration) keys.

## Commands

### `get <key>`
Retrieve the current value of a vconf key. Returns JSON `{"key":"...","type":"...","value":...}`.

### `set <key> <value>`
Update the value of a vconf key. Type is auto-detected from existing key.

### `watch <key>`
Monitor a vconf key for real-time changes. Streams JSON events until stopped.

## Useful Vconf Key Reference

### Display & Brightness
| Key | Type | Description |
|-----|------|-------------|
| `db/setting/Brightness` | int | LCD brightness level (1-100) |
| `db/setting/brightness_automatic` | int | Auto brightness (0=off, 1=on) |
| `db/setting/lcd_backlight_normal` | int | Screen timeout in seconds |
| `memory/pm/current_brt` | int | Current display brightness |

### Sound & Volume
| Key | Type | Description |
|-----|------|-------------|
| `db/setting/sound/media/sound_volume` | int | Media volume level |
| `db/setting/sound/call/ringtone_sound_volume` | int | Ringtone volume |
| `db/setting/sound/touch_sounds` | bool | Touch sounds enabled |

### Battery & Power
| Key | Type | Description |
|-----|------|-------------|
| `memory/sysman/battery_capacity` | int | Battery percentage (0-100) |
| `memory/sysman/charge_now` | int | Currently charging (0/1) |
| `memory/sysman/charger_status` | int | Charger connected (0=disconnected, 1=connected) |
| `db/sysman/low_power_mode` | int | Power saving mode (0=off, 1=on) |

### Wi-Fi & Network
| Key | Type | Description |
|-----|------|-------------|
| `memory/wifi/state` | int | Wi-Fi state (0=off, 1=unconnected, 2=connected) |
| `memory/wifi/connected_ap_name` | string | Connected AP name (SSID) |
| `memory/dnet/ip` | string | Current IP address |

### Bluetooth
| Key | Type | Description |
|-----|------|-------------|
| `db/bluetooth/status` | int | BT status (0=off, 1=on, 2=visible) |

## Usage Examples
```
tizen-vconf-cli get db/setting/Brightness
tizen-vconf-cli set db/setting/Brightness 50
tizen-vconf-cli get memory/sysman/battery_capacity
```

## Output
All output is JSON.
```json
{"key": "db/setting/Brightness", "type": "int", "value": 80}
```
