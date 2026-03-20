# tizen-vconf-cli

Manage and monitor Tizen vconf keys.

## Commands

### `get <key>`
Retrieve the current value of a vconf key.
- `<key>`: vconf key name (e.g., `db/setting/language`)

### `set <key> <value>`
Update the value of a vconf key.
- `<key>`: vconf key name
- `<value>`: New value (int, bool, double, or string)

### `watch <key>`
Monitor a vconf key for changes. This command stays active and streams updates as JSON until stopped.
- `<key>`: vconf key name

## Commonly Used Keys

- `db/setting/language`: System language
- `db/setting/region`: System region
- `db/setting/timezone`: System timezone
- `db/menu/sound_enabled`: System sound setting
- `db/network/status`: Network connection status

## Usage Example (with LLM Tools)

To monitor language changes:
1. Call `start_cli_session(tool_name="tizen-vconf-cli", arguments="watch db/setting/language", mode="streaming")`
2. Periodically call `read_cli_output(session_id="...")` to receive change events.
