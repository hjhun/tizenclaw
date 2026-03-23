---
name: tizen-notification-cli
description: "Send notifications and schedule alarms"
type: cli
command: "python3 /opt/usr/share/tizenclaw/tools/cli_py/tizen_notification_cli.py"
---
# tizen-notification-cli

**Description**: Send notifications and schedule alarms.

## Subcommands

| Subcommand | Options |
|---|---|
| `notify` | `--title <t> --body <b>` — Post a notification |
| `alarm` | `--app-id <id> --datetime <YYYY-MM-DDTHH:MM:SS>` — Schedule an alarm |

## Usage
```
tizen-notification-cli notify --title "Hello" --body "World"
tizen-notification-cli alarm --app-id org.example.app --datetime 2026-03-23T12:00:00
```

## Output
All output is JSON.
