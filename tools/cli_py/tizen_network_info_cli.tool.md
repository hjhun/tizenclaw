---
name: tizen-network-info-cli
description: "Query network, Wi-Fi, Bluetooth status, scan devices, and data usage"
type: cli
command: "python3 /opt/usr/share/tizenclaw/tools/cli_py/tizen_network_info_cli.py"
---
# tizen-network-info-cli

**Description**: Query network, Wi-Fi, Bluetooth status, scan devices, and data usage.

## Subcommands

| Subcommand | Description |
|---|---|
| `network` | Connection type, IP, proxy |
| `wifi` | Wi-Fi activation state, ESSID |
| `wifi-scan` | Scan for Wi-Fi networks (SSID, RSSI, frequency, security) |
| `bluetooth` | BT adapter state, name, address |
| `bt-scan` | List bonded/paired Bluetooth devices (name, address, connected) |
| `data-usage` | Wi-Fi/cellular data statistics |

## Usage
```
tizen-network-info-cli network
tizen-network-info-cli wifi
tizen-network-info-cli wifi-scan
tizen-network-info-cli bluetooth
tizen-network-info-cli bt-scan
tizen-network-info-cli data-usage
```

## Output
All output is JSON. Examples:
```json
// network
{"status": "success", "type": "wifi", "ip": "192.168.1.100", "proxy": "none"}

// wifi
{"status": "success", "state": "connected", "essid": "MyNetwork", "rssi": -45}
```
