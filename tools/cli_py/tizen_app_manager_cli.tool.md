---
name: tizen-app-manager-cli
description: "Manage applications: list, terminate, launch via app_control, query package info"
type: cli
command: "python3 /opt/usr/share/tizenclaw/tools/cli_py/tizen_app_manager_cli.py"
---
# tizen-app-manager-cli

**Description**: Manage applications: list, terminate, launch via app_control, query package info.

## Subcommands

| Subcommand | Options |
|---|---|
| `list` | List installed UI apps (app_id, label) |
| `list-all` | List all installed apps regardless of component type |
| `running` | List running UI apps (app_id, pid) |
| `running-all` | List all running apps regardless of component type |
| `terminate` | `--app-id <id>` — Terminate a running app |
| `launch` | `--app-id <id> [--operation <op>] [--uri <uri>] [--mime <mime>]` — Launch an app |
| `package-info` | `--package-id <id>` — Get package version and type |

## Usage
```
tizen-app-manager-cli list
tizen-app-manager-cli running
tizen-app-manager-cli terminate --app-id org.example.app
tizen-app-manager-cli launch --app-id org.example.app
tizen-app-manager-cli package-info --package-id org.example.app
```

## Output
All output is JSON. Examples:
```json
// list
{"status": "success", "apps": [{"app_id": "org.example.app", "label": "Example"}]}

// running
{"status": "success", "running_apps": [{"app_id": "org.example.app", "pid": 1234}]}

// terminate
{"status": "success", "code": 0}
```
