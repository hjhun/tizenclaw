# tizen-app-manager-cli
**Description**: Manage applications: list, terminate, launch via app_control, query package info.
## Subcommands
| Subcommand | Options |
|---|---|
| `list` | List installed UI apps (detailed info: app_id, label, icon, exec, package, type, component_type, nodisplay) |
| `list-all` | List all installed apps regardless of component type (detailed info: app_id, label, icon, exec, package, type, component_type, nodisplay) |
| `terminate` | `--app-id <id>` |
| `launch` | `--app-id <id> [--operation <op>] [--uri <uri>] [--mime <mime>]` |
| `package-info` | `--package-id <id>` |
