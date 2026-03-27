//! Channel factory — creates channel instances from config.

use super::{Channel, ChannelConfig};

pub fn create_channel(config: &ChannelConfig) -> Option<Box<dyn Channel + Send + Sync>> {
    match config.channel_type.as_str() {
        "web_dashboard" => {
            Some(Box::new(super::web_dashboard::WebDashboard::new(config)))
        }
        "webhook" => Some(Box::new(super::webhook_channel::WebhookChannel::new(config))),
        "telegram" => Some(Box::new(super::telegram_client::TelegramClient::new(config))),
        "discord" => Some(Box::new(super::discord_channel::DiscordChannel::new(config))),
        "slack" => Some(Box::new(super::slack_channel::SlackChannel::new(config))),
        "voice" => Some(Box::new(super::voice_channel::VoiceChannel::new(config))),
        "a2a" => Some(Box::new(super::a2a_handler::A2aHandler::new(config))),
        _ => {
            log::warn!("Unknown channel type: {}", config.channel_type);
            None
        }
    }
}
