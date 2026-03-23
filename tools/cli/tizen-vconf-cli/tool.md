# tizen-vconf-cli

Read, write, and monitor Tizen vconf (virtual configuration) keys.
Binary path: /opt/usr/share/tizen-tools/cli/tizen-vconf-cli/tizen-vconf-cli

## Commands

### `get <key>`
Retrieve the current value of a vconf key. Returns JSON `{"key":"...","type":"...","value":...}`.

### `set <key> <value>`
Update the value of a vconf key. Type is auto-detected from existing key.

### `watch <key>`
Monitor a vconf key for real-time changes. Streams JSON events until stopped.
Best used with `start_cli_session` in streaming mode.

## Useful Vconf Key Reference

### Display & Brightness
| Key | Type | Description |
|-----|------|-------------|
| `db/setting/Brightness` | int | LCD brightness level (1-100) |
| `db/setting/brightness_automatic` | int | Auto brightness (0=off, 1=on, 2=pause) |
| `db/setting/automatic_brightness_level` | int | Auto brightness target level |
| `db/setting/lcd_backlight_normal` | int | Screen timeout in seconds (e.g., 15, 30, 60) |
| `db/setting/auto_display_adjustment` | bool | Auto display color adjustment |
| `memory/pm/state` | int | Power manager state (1=normal, 2=dim, 3=off, 4=sleep) |
| `memory/pm/current_brt` | int | Current display brightness |

### Sound & Volume
| Key | Type | Description |
|-----|------|-------------|
| `db/setting/sound/media/sound_volume` | int | Media volume level |
| `db/setting/sound/call/ringtone_sound_volume` | int | Ringtone volume |
| `db/setting/sound/call/ringtone_path` | string | Current ringtone file path |
| `db/setting/sound/call/ringtone_default_path` | string | Default ringtone path |
| `db/setting/sound/noti/msg_ringtone_path` | string | Notification sound path |
| `db/setting/sound/touch_sounds` | bool | Touch sounds enabled |
| `db/setting/sound/button_sounds` | bool | Button sounds enabled |
| `db/setting/sound/sound_lock` | bool | Lock sound enabled |
| `db/setting/sound/call/vibration_level` | int | Call vibration intensity |
| `db/setting/sound/noti/vibration_level` | int | Notification vibration intensity |
| `db/setting/sound/touch_feedback/vibration_level` | int | Touch feedback vibration (0-5) |
| `memory/Sound/SoundStatus` | int | Sound status |

### Battery & Power
| Key | Type | Description |
|-----|------|-------------|
| `memory/sysman/battery_capacity` | int | Battery percentage (0-100) |
| `memory/sysman/battery_status_low` | int | Battery level (1=power_off, 2=critical, 3=warning, 4=normal, 5=full) |
| `memory/sysman/battery_level_status` | int | Battery level status (0=empty..4=full) |
| `memory/sysman/charge_now` | int | Currently charging (0/1) |
| `memory/sysman/charger_status` | int | Charger connected (0=disconnected, 1=connected) |
| `memory/sysman/charger_type` | int | Charger type |
| `db/sysman/low_power_mode` | int | Power saving mode (0=off, 1=on) |
| `db/setting/battery_percentage` | bool | Show battery percentage in status bar |

### Wi-Fi & Network
| Key | Type | Description |
|-----|------|-------------|
| `memory/wifi/state` | int | Wi-Fi state (0=off, 1=unconnected, 2=connected) |
| `memory/wifi/strength` | int | Wi-Fi signal strength (0-4) |
| `memory/wifi/connected_ap_name` | string | Connected AP name (SSID) |
| `memory/dnet/status` | int | Network status (0=off, 1=cellular, 2=wifi, 3=ethernet) |
| `memory/dnet/ip` | string | Current IP address |
| `memory/dnet/wifi` | int | Network Wi-Fi state (0=off, 1=not connected, 2=connected) |
| `memory/dnet/cellular` | int | Cellular network state |

### Bluetooth
| Key | Type | Description |
|-----|------|-------------|
| `db/bluetooth/status` | int | BT status (0=off, 1=on, 2=visible, 4=transfer) |
| `db/bluetooth/lestatus` | int | BT LE status (0=off, 1=on) |
| `memory/bluetooth/device` | int | Connected BT device bitmask |
| `memory/bluetooth/btsco` | bool | BT SCO headset connected |
| `memory/bluetooth/sco_headset_name` | string | Connected headset name |

### Locale & Language
| Key | Type | Description |
|-----|------|-------------|
| `db/menu_widget/language` | string | System language (e.g., "en_US.UTF-8") |
| `db/menu_widget/regionformat` | string | Region format (e.g., "en_US.UTF-8") |
| `db/setting/languages` | string | Language priority list (e.g., "en_US:en_GB:en") |
| `db/setting/date_format` | int | Date format (0=DD/MM/YYYY, 1=MM/DD/YYYY, 2=YYYY/MM/DD) |
| `db/setting/weekofday_format` | int | First day of week (0=Sunday..6=Saturday) |
| `db/setting/automatic_time_update` | bool | Automatic time update (NTP) |
| `db/setting/cityname_id` | string | Timezone city name |

### Accessibility
| Key | Type | Description |
|-----|------|-------------|
| `db/setting/accessibility/tts` | bool | Screen reader (TTS) enabled |
| `db/setting/accessibility/font_size` | int | Accessibility font size |
| `db/setting/accessibility/font_name` | string | Font name (e.g., "Default") |
| `db/setting/accessibility/high_contrast` | bool | High contrast mode |
| `db/setting/accessibility/greyscale` | bool | Greyscale mode |
| `db/setting/accessibility/screen_zoom` | bool | Screen zoom enabled |

### Display & UI
| Key | Type | Description |
|-----|------|-------------|
| `db/setting/font_size` | int | Font size (0=small, 1=medium, 2=large) |
| `db/setting/screen_lock_type` | int | Screen lock type (0=none, 1=swipe, 5=simple_pw, 6=password, 9=pattern) |
| `db/setting/menuscreen/package_name` | string | Home screen app package |

### USB & Peripherals
| Key | Type | Description |
|-----|------|-------------|
| `memory/sysman/usb_status` | int | USB status (0=disconnected, 1=connected, 2=available) |
| `db/setting/debug_mode` | bool | USB debugging mode |
| `memory/sysman/earjack` | int | Earjack status (0=removed, 1=3-wire, 3=4-wire) |
| `memory/sysman/hdmi` | int | HDMI status (0=disconnected, 1=connected) |

### System
| Key | Type | Description |
|-----|------|-------------|
| `memory/sysman/low_memory` | int | Low memory status (1=normal, 2=soft_warn, 4=hard_warn) |
| `memory/sysman/mmc` | int | SD card status (0=removed, 1=mounted) |
| `memory/sysman/booting_status` | int | Boot status (0=running, 1=success, 3=failure) |

## Usage Examples

### One-shot read
```
tizen-vconf-cli get db/setting/Brightness
# → {"key":"db/setting/Brightness","type":"int","value":80}
```

### Set value
```
tizen-vconf-cli set db/setting/Brightness 50
# → {"status":"ok"}
```

### Monitor changes (with CLI session)
```
# Start a watch session:
start_cli_session(tool_name="tizen-vconf-cli", arguments="watch memory/sysman/battery_capacity", mode="streaming")
# Then periodically read:
read_cli_output(session_id="...")
# → {"key":"memory/sysman/battery_capacity","type":"int","value":85,"event":"changed"}
```
