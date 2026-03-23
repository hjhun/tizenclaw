---
name: tizen-media-cli
description: "Query media content database, extract file metadata, and look up MIME types"
type: cli
command: "python3 /opt/usr/share/tizenclaw/tools/cli_py/tizen_media_cli.py"
---
# tizen-media-cli

**Description**: Query media content database, extract file metadata, and look up MIME types.

## Subcommands

| Subcommand | Options |
|---|---|
| `content` | `[--type image\|video\|sound\|music\|all] [--max N]` — List media files from the content DB |
| `metadata` | `--path <file_path>` — Extract metadata (duration, bitrate, artist, title, etc.) |
| `mime` | `--path <file_path>` — Get MIME type for a file |
| `mime-ext` | `--mime <mime_type>` — Get file extensions for a MIME type |

## Usage
```
tizen-media-cli content --type image --max 10
tizen-media-cli metadata --path /opt/usr/media/photo.jpg
tizen-media-cli mime --path /opt/usr/media/video.mp4
tizen-media-cli mime-ext --mime image/jpeg
```

## Output
All output is JSON.
