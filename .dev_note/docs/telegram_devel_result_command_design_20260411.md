# Telegram Devel Result Command Design

## Scope

Add a telegram `/devel_result` command that reads the newest file from
`~/.tizenclaw/devel/result` and returns its content in chat.

## Ownership Boundary

- `core/devel_mode.rs` owns devel prompt/result directory resolution and
  latest-result file lookup.
- `core/ipc_server.rs` exposes the lookup result as a JSON-RPC method so
  `tizenclaw-tests` can verify the daemon-visible contract.
- `channel/telegram_client.rs` only formats the shared result for
  Telegram and registers the bot command.

## Persistence Boundary

The change is read-only. It reuses the existing devel result directory
and does not introduce new state, config, or registry files.

## IPC Contract

`get_devel_result` returns:

- `status`
- `result_dir`
- `available`
- `latest_result_path`
- `content`

When no result file exists, `available` is `false` and `content` is
empty.

## Verification Plan

- Update `tests/system/devel_mode_prompt_flow.json` to assert the new
  `get_devel_result` response shape.
- Add unit coverage for latest-result lookup and telegram command
  routing.
- Validate with `./deploy_host.sh -b`, `./deploy_host.sh`,
  `tizenclaw-tests scenario --file tests/system/devel_mode_prompt_flow.json`,
  and `./deploy_host.sh --test`.
