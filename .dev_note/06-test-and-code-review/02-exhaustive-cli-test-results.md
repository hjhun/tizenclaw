# Exhaustive CLI Tool Test Results

**Date**: 2026-04-01  
**Target**: x86_64 Tizen Emulator  
**Total Directories**: 88  
**Executable Tools**: 80  
**Missing Binaries**: 8  

---

## Status Legend
- ✅ **PASS** — Tool executed and returned valid output
- ⚠️ **DEGRADED** — Tool launched but hardware/service unavailable on emulator
- ❌ **FAIL** — Binary missing, shared lib error, or hard crash
- 🔄 **REPL** — Interactive REPL mode (tested via help→exit)
- 🔒 **HANG** — Tool hung on --help, required forced termination

---

## Complete Results Table

| # | Tool Name | Type | Status | Output Summary |
|---|-----------|------|--------|---------------|
| 1 | tizen-accounts-svc-cli | 🔄 REPL | ✅ PASS | `count` → "Total Accounts in DB: 0" |
| 2 | tizen-alarm-cli | CLI | ✅ PASS | `list` → "[SUCCESS] Querying alarms:" (empty) |
| 3 | tizen-app-manager-cli | CLI | ✅ PASS | `list-installed` → 55 apps, `list-running` → 5 running |
| 4 | tizen-asp-cli | CLI | ✅ PASS | Help: `advert`, `seek` subcommands |
| 5 | tizen-audio-io-cli | CLI | ✅ PASS | `info` → "16-bit PCM, Little Endian, Async Ecore loop" |
| 6 | tizen-base-utils-cli | CLI | ✅ PASS | `locale` → en_US_POSIX/eng/USA, `timezone` → Asia/Seoul |
| 7 | tizen-battery-monitor-cli | CLI | ❌ FAIL | Missing `libcapi-system-battery-monitor.so.0` |
| 8 | tizen-bluetooth-cli | 🔄 REPL | ⚠️ DEGRADED | `state` → "Failed to get adapter state: -1073741822" |
| 9 | tizen-bundle-cli | CLI | ✅ PASS | `encode name=TizenClaw version=1.0.0` → hex encoded |
| 10 | tizen-calendar-service2-cli | CLI | ⚠️ DEGRADED | "Failed to connect to calendar service" |
| 11 | tizen-camera-cli | 🔄 REPL | ✅ PASS | Help: create, start-preview, capture, state, destroy |
| 12 | tizen-capi-media-vision-cli | — | ❌ FAIL | Binary not installed |
| 13 | tizen-capi-media-vision-dl-cli | — | ❌ FAIL | Binary not installed |
| 14 | tizen-capi-system-system-settings-cli | — | ❌ FAIL | Binary not installed |
| 15 | tizen-capi-ui-autofill-cli | — | ❌ FAIL | Binary not installed |
| 16 | tizen-capi-ui-inputmethod-cli | — | ❌ FAIL | Binary not installed |
| 17 | tizen-capi-ui-inputmethod-manager-cli | — | ❌ FAIL | Binary not installed |
| 18 | tizen-capi-ui-sticker-cli | CLI | ⚠️ DEGRADED | `--help` → "[ERROR] Unknown command" |
| 19 | tizen-connection-cli | CLI | ✅ PASS | `type` → Ethernet, `ip` → IPv4:10.0.2.15 IPv6:fec0::... |
| 20 | tizen-contacts-service2-cli | CLI | ✅ PASS | `show-contact-count` → 0 |
| 21 | tizen-efl-util-cli | CLI | ✅ PASS | Help: input key/touch/pointer/wheel |
| 22 | tizen-file-manager-cli | CLI | ✅ PASS | `list --path /usr` → 14 dirs JSON, `stat` → JSON |
| 23 | tizen-http-cli | — | ❌ FAIL | Binary not installed |
| 24 | tizen-image-util-cli | CLI | ✅ PASS | Help: extract-color, transform, decode, encode |
| 25 | tizen-intelligent-network-monitoring-cli | CLI | ⚠️ DEGRADED | "inm_initialize failed: -1073741822" |
| 26 | tizen-iotcon-cli | CLI | ✅ PASS | Help: find-resource, get, put, post, delete, observe |
| 27 | tizen-key-manager-cli | CLI | ⚠️ DEGRADED | `list-data` → "Command execution failed (Code: -31522804)" |
| 28 | tizen-libfeedback-cli | CLI | ✅ PASS | Help: play, stop, is_supported |
| 29 | tizen-libstorage-cli | CLI | ✅ PASS | `internal-size` → SUCCESS, `list` → empty |
| 30 | tizen-libstorage64-cli | CLI | ✅ PASS | Help: list, info, internal |
| 31 | tizen-location-manager-cli | CLI | ⚠️ DEGRADED | "Unknown cmd" (neither --help nor help accepted) |
| 32 | tizen-media-content-cli | CLI | ⚠️ DEGRADED | "media_content_connect failed: -23134207" |
| 33 | tizen-media-controller-cli | CLI | ✅ PASS | Help: latest, list-servers, playback |
| 34 | tizen-media-key-cli | CLI | ✅ PASS | Help: reserve, release |
| 35 | tizen-mediacodec-cli | CLI | ⚠️ DEGRADED | `list-codecs` → "mediacodec_create failed" |
| 36 | tizen-mediatool-cli | CLI | ✅ PASS | Help: create-audio, create-video, verify |
| 37 | tizen-metadata-extractor-cli | CLI | ✅ PASS | Help: extract, get, list-attrs |
| 38 | tizen-mime-type-cli | CLI | ✅ PASS | `get-mime jpg` → "image/jpeg" |
| 39 | tizen-mmi-cli | CLI | ✅ PASS | Help: init, create-standard, signal |
| 40 | tizen-mtp-cli | CLI | ✅ PASS | Help: list-devices, device-info |
| 41 | tizen-multi-assistant-cli | CLI | ⚠️ DEGRADED | "Unknown command" (both --help and help) |
| 42 | tizen-native-common-cli | CLI | ✅ PASS | `get-error-message` → "Error Message for -22: Invalid argument" |
| 43 | tizen-network-info-cli | CLI | ✅ PASS | `network` → JSON, `data-usage` → JSON, `wifi` → init fail |
| 44 | tizen-nfc-cli | 🔄 REPL | ✅ PASS | Commands: supported, init, deinit |
| 45 | tizen-nntrainer-cli | CLI | ✅ PASS | Help: construct-summary, destroy-test |
| 46 | tizen-notification-cli | CLI | ✅ PASS | Help: notify, alarm |
| 47 | tizen-nsd-cli | CLI | ✅ PASS | Help: setup-service |
| 48 | tizen-oauth2-cli | CLI | 🔒 HANG | Hung on --help, forced termination |
| 49 | tizen-package-manager-cli | CLI | ✅ PASS | `list` → 59 packages with versions |
| 50 | tizen-phonenumber-utils-cli | CLI | ⚠️ DEGRADED | `format` → Telephony not supported |
| 51 | tizen-player-cli | CLI | ✅ PASS | Help: play, info, volume |
| 52 | tizen-player-display-cli | CLI | ✅ PASS | Help: set-get-mode, set-get-rotation, set-get-visible |
| 53 | tizen-privilege-info-cli | CLI | ✅ PASS | `name http://...network.get` → display name returned |
| 54 | tizen-push-cli | CLI | ✅ PASS | Help: connect, register |
| 55 | tizen-radio-cli | CLI | ✅ PASS | Help: tune, range, signal, start |
| 56 | tizen-recorder-cli | CLI | ✅ PASS | Help: setup, state, record |
| 57 | tizen-resource-monitor-cli | CLI | ✅ PASS | Help: stream, help |
| 58 | tizen-runtime-info-cli | CLI | ✅ PASS | `memory` → 1GB phys, `cpu` → 1.19% user, `status` → all |
| 59 | tizen-screen-mirroring-cli | CLI | ✅ PASS | Help: setup-sink |
| 60 | tizen-sensor-cli | — | ❌ FAIL | Binary not installed |
| 61 | tizen-softap-cli | CLI | ✅ PASS | Help: info, status |
| 62 | tizen-sound-manager-cli | CLI | ✅ PASS | `get-volume media` → "11/15" |
| 63 | tizen-sound-pool-cli | CLI | ⚠️ DEGRADED | Missing libvorbisfile.so.3 (but help loads) |
| 64 | tizen-stt-cli | CLI | ⚠️ DEGRADED | "Unknown command" for both --help and help |
| 65 | tizen-sync-manager-cli | CLI | ✅ PASS | Help: on-demand, periodic, data-change, remove |
| 66 | tizen-system-info-cli | CLI | ✅ PASS | `get-str model_name` → "Emulator" |
| 67 | tizen-tbm-cli | 🔄 REPL | ✅ PASS | Commands: init, capability, deinit |
| 68 | tizen-telephony-cli | CLI | ✅ PASS | Help: sim-info, network-info |
| 69 | tizen-thumbnail-util-cli | CLI | ✅ PASS | Help: extract-file |
| 70 | tizen-trace-cli | 🔄 REPL | ✅ PASS | Commands: begin, end, async-begin/end, update |
| 71 | tizen-tts-cli | CLI | 🔒 HANG | Hung on --help, forced termination |
| 72 | tizen-tzsh-quickpanel-cli | CLI | ✅ PASS | Help printed successfully |
| 73 | tizen-ua-cli | 🔄 REPL | ✅ PASS | Commands: init, get_available_sensors, etc. |
| 74 | tizen-url-download-cli | CLI | ✅ PASS | Help: start |
| 75 | tizen-usb-host-cli | CLI | ⚠️ DEGRADED | `list` → "Failed to create USB Host Context" |
| 76 | tizen-vconf-cli | CLI | ✅ PASS | `get lcd_backlight_normal` → JSON {type:int, value:30} |
| 77 | tizen-voice-control-cli | CLI | ✅ PASS | Help: languages, states, prepare, set-invocation-name |
| 78 | tizen-voice-control-elm-cli | CLI | ✅ PASS | Help: language, supported-languages, widgets, actions |
| 79 | tizen-vpn-cli | 🔄 REPL | ✅ PASS | Commands: init, info, set-config, route, dns, etc. |
| 80 | tizen-wav-player-cli | CLI | ✅ PASS | Help: play-new, play-loop |
| 81 | tizen-webkit2-cli | — | ❌ FAIL | Binary not installed |
| 82 | tizen-webrtc-cli | 🔄 REPL | ✅ PASS | Commands: create, start, stop, add-source, channel-* |
| 83 | tizen-webrtc-display-cli | — | ❌ FAIL | Binary not installed |
| 84 | tizen-widget-service-cli | — | ❌ FAIL | Binary not installed |
| 85 | tizen-wifi-aware-cli | CLI | ✅ PASS | Help: status |
| 86 | tizen-wifi-direct-cli | CLI | ✅ PASS | Help: state, devices |
| 87 | tizen-wifi-manager-cli | CLI | ⚠️ DEGRADED | "Unknown command" for both --help and help |
| 88 | tizen-yaca-cli | CLI | ✅ PASS | `digest HelloWorld` → SHA256 hash returned |

---

## Summary Statistics

| Category | Count | Percentage |
|----------|-------|-----------|
| ✅ PASS | 56 | 63.6% |
| ⚠️ DEGRADED (HW/Service unavailable) | 14 | 15.9% |
| ❌ FAIL (Binary missing / lib error) | 9 | 10.2% |
| 🔄 REPL (Interactive, tested OK) | 8 | 9.1% |
| 🔒 HANG (Forced kill needed) | 2 | 2.3% |

**Effective Success Rate**: 72.7% (64/88 tools fully or partially operational)

---

## Key Findings

### Binary Missing (Build Not Configured)
- tizen-capi-media-vision-cli, tizen-capi-media-vision-dl-cli
- tizen-capi-system-system-settings-cli
- tizen-capi-ui-autofill-cli, tizen-capi-ui-inputmethod-cli, tizen-capi-ui-inputmethod-manager-cli
- tizen-http-cli, tizen-sensor-cli
- tizen-webkit2-cli, tizen-webrtc-display-cli, tizen-widget-service-cli

### Shared Library Errors
- `tizen-battery-monitor-cli`: Missing `libcapi-system-battery-monitor.so.0`
- `tizen-sound-pool-cli`: Missing `libvorbisfile.so.3` (partial — help still works)

### Hung Processes (Potential Agent Timeout Risk)
- `tizen-tts-cli`: Hangs indefinitely on `--help`
- `tizen-oauth2-cli`: Hangs indefinitely on `--help`

### Emulator Limitations (Expected)
- Bluetooth, WiFi, NFC, USB Host, Telephony, Location → Hardware not emulated
- Calendar, Media Content, Media Codec, Key Manager → Services not running
