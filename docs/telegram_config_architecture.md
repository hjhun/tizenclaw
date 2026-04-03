# Architecture Design: Telegram Config Override

## Module Modifications
- **Target:** `src/tizenclaw/src/channel/telegram_client.rs`
- **Hook Point:** `TelegramClient::new`

## Design Details
During instantiation, the client first reads arguments from the primary `ChannelConfig` derived from `channel_config.json`. 
Immediately after, it performs a non-blocking synchronous check `std::fs::read_to_string("/opt/usr/share/tizenclaw/config/telegram_config.json")`.
If successful, it parses the JSON content and structurally overwrites:
1. `bot_token`
2. `allowed_chat_ids`

## Concurrency
Since this happens during `AgentCore` initialization (inside `main.rs` daemon boot), synchronous filesystem APIs (`std::fs`) are safe and will not block the main async reactor or polling threads, ensuring robust initialization.
