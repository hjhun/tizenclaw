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

## LLM Agent Instructions
**CRITICAL**: You MUST provide exactly ONE subcommand as a positional argument. DO NOT pass subcommands as options or prefix them.
Example: `network`
Example: `wifi-scan`
