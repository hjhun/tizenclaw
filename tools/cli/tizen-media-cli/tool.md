# tizen-media-cli
**Description**: Query media content database, extract file metadata, and look up MIME types.
## Subcommands
| Subcommand | Options |
|---|---|
| `content` | `[--type image\|video\|sound\|music\|all] [--max N]` — List media files from the content DB |
| `metadata` | `--path <file_path>` — Extract metadata (duration, bitrate, artist, title, etc.) |
| `mime` | `--path <file_path>` — Get MIME type for a file |
| `mime-ext` | `--mime <mime_type>` — Get file extensions for a MIME type |

## LLM Agent Instructions
**CRITICAL**: You MUST use the exact subcommand as the first position argument. DO NOT confuse subcommand and arguments.
Example: `content` or `content --type video` or `mime-ext --mime image/jpeg`
