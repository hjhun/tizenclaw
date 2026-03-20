# tizen-app-manager-cli
**Description**: Manage applications: list, terminate, launch via app_control, query package info, recent apps.
## Subcommands
| Subcommand | Options |
|---|---|
| `list` | List installed UI apps (detailed info: app_id, label, icon, exec, package, type, component_type, nodisplay) |
| `list-all` | List all installed apps regardless of component type (detailed info: app_id, label, icon, exec, package, type, component_type, nodisplay) |
| `running` | List running UI apps (detailed info: app_id, label, pid, state, icon, exec, package, type, component_type, nodisplay) |
| `running-all` | List all running apps regardless of component type (detailed info: app_id, label, pid, state, icon, exec, package, type, component_type, nodisplay) |
| `recent` | List recently used apps via RUA (info: app_id, label, icon, app_path, launch_time, launch_time_str, instance_id, instance_name, component_id, uri, image, managed_by_task_manager) |
| `recent-detail` | `--app-id <id>` — Get launch args and context for a specific recent app (info: app_id, args, uri, instance_id, component_id, launch_time, launch_time_str). Use this to build a bundle for re-launching. |
| `terminate` | `--app-id <id>` |
| `launch` | `--app-id <id> [--operation <op>] [--uri <uri>] [--mime <mime>]` |
| `package-info` | `--package-id <id>` |
