# tizen-file-manager-cli
**Description**: File operations: read, write, append, copy, move, remove, list, stat, mkdir, download.
## Subcommands
| Subcommand | Description | Required Args |
|------------|-------------|---------------|
| `read` | Read file contents | `--path` |
| `write` | Write/overwrite file | `--path --content` |
| `append` | Append to file | `--path --content` |
| `remove` | Remove file | `--path` |
| `mkdir` | Create directory | `--path` |
| `list` | List directory entries | `--path` |
| `stat` | Get file/dir metadata | `--path` |
| `copy` | Copy file | `--src --dst` |
| `move` | Move/rename file | `--src --dst` |
| `download` | Download file from URL | `--url --dest` |
## Output
All output is JSON.

## LLM Agent Instructions
**CRITICAL**: You MUST use the exact subcommand as the first positional argument, followed by the required path arguments.
Example: `list --path /tmp`
Example: `read --path /tmp/file.txt`
DO NOT forget the `--path` option!
