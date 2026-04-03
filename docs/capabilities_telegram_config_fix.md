# Telegram Configuration Fallback Support

## Objective
Support direct reading of `telegram_config.json` inside the `/opt/usr/share/tizenclaw/config/` directory.

## Background
The user injected an explicit `telegram_config.json` into the device's config directory to store the bot token and allowed chat IDs. However, the core daemon historically only parses `channel_config.json` via the `ChannelRegistry`. This caused the bot to have an invalid token and fail to start or respond.

## Execution Mode
- Action Module / Helper logic (Config overriding)

## Requirements
Modify `TelegramClient::new` to dynamically detect and parse `/opt/usr/share/tizenclaw/config/telegram_config.json`. If present, its values for `bot_token` and `allowed_chat_ids` will override the fallback configurations parsed from `channel_config.json`. This ensures backward compatibility while accommodating the user's manual config provisioning.
