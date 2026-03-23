---
name: tizen-file-manager-cli
description: "File operations: read, write, append, copy, move, remove, list, stat, mkdir, download"
type: cli
command: "python3 /opt/usr/share/tizenclaw/tools/cli_py/tizen_file_manager_cli.py"
---
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

## Usage
```
tizen-file-manager-cli list --path /opt/usr
tizen-file-manager-cli read --path /etc/hostname
tizen-file-manager-cli write --path /tmp/test.txt --content "hello"
tizen-file-manager-cli stat --path /opt/usr
tizen-file-manager-cli copy --src /tmp/a.txt --dst /tmp/b.txt
tizen-file-manager-cli download --url https://example.com/file --dest /tmp/file
```

## Output
All output is JSON.
