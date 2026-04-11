//! Channel factory — creates channel instances from config.

use super::{channel_settings_from_entry, Channel, ChannelConfig, ChannelRegistry};
use serde_json::Value;

pub fn create_channel(
    config: &ChannelConfig,
    agent: Option<std::sync::Arc<crate::core::agent_core::AgentCore>>,
) -> Option<Box<dyn Channel + Send + Sync>> {
    match config.channel_type.as_str() {
        "web_dashboard" => Some(Box::new(super::web_dashboard::WebDashboard::new(config))),
        "webhook" => super::webhook_channel::WebhookChannel::from_config(&config.settings)
            .ok()
            .map(|_| Box::new(super::webhook_channel::WebhookChannel::new(config)) as _),
        "telegram" => {
            let settings_empty = config
                .settings
                .as_object()
                .map(|settings| settings.is_empty())
                .unwrap_or(true);
            if settings_empty
                || super::telegram_client::TelegramClient::from_config(&config.settings).is_ok()
            {
                Some(Box::new(super::telegram_client::TelegramClient::new(
                    config,
                    agent.clone(),
                )))
            } else {
                None
            }
        }
        "discord" => super::discord_channel::DiscordChannel::from_config(&config.settings)
            .ok()
            .map(|_| Box::new(super::discord_channel::DiscordChannel::new(config)) as _),
        "slack" => super::slack_channel::SlackChannel::from_config(&config.settings)
            .ok()
            .map(|_| Box::new(super::slack_channel::SlackChannel::new(config)) as _),
        "voice" => Some(Box::new(super::voice_channel::VoiceChannel::new(config))),
        "a2a" => {
            if let Some(a) = agent {
                Some(Box::new(super::a2a_handler::A2aHandler::new(config, a)))
            } else {
                log::warn!("AgentCore missing for a2a channel instantiation");
                None
            }
        }
        _ => {
            log::warn!("Unknown channel type: {}", config.channel_type);
            None
        }
    }
}

pub fn build_channel_registry_from_config(config: &Value) -> ChannelRegistry {
    let mut registry = ChannelRegistry::new();

    let Some(channels) = config.get("channels").and_then(Value::as_array) else {
        return registry;
    };

    for (index, entry) in channels.iter().enumerate() {
        let Some(channel_type) = entry.get("type").and_then(Value::as_str) else {
            log::warn!(
                "Skipping channel entry {} because required field 'type' is missing",
                index
            );
            continue;
        };

        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| channel_type.to_string());
        let enabled = entry.get("enabled").and_then(Value::as_bool).unwrap_or(true);
        let settings = channel_settings_from_entry(entry);
        let cfg = ChannelConfig {
            name,
            channel_type: channel_type.to_string(),
            enabled,
            settings,
        };

        match create_channel(&cfg, None) {
            Some(channel) => {
                let auto_start = if cfg.channel_type == "web_dashboard" {
                    false
                } else {
                    enabled
                };
                registry.register(channel, auto_start);
            }
            None => {
                log::warn!(
                    "Skipping channel '{}' because configuration is incomplete",
                    cfg.channel_type
                );
            }
        }
    }

    registry
}

#[cfg(test)]
mod tests {
    use super::build_channel_registry_from_config;
    use serde_json::json;

    #[test]
    fn skips_channels_with_missing_type() {
        let registry = build_channel_registry_from_config(&json!({
            "channels": [
                {"name": "missing-type"},
                {"type": "voice", "name": "voice", "stt_engine": "none", "tts_engine": "none"}
            ]
        }));

        assert!(registry.has_channel("voice"));
        assert!(!registry.has_channel("missing-type"));
    }
}
