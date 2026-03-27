//! Channel module — abstract channel interface and implementations.

use serde_json::Value;

/// Channel configuration from channel_config.json.
pub struct ChannelConfig {
    pub name: String,
    pub channel_type: String,
    pub enabled: bool,
    pub settings: Value,
}

/// A message received from a channel.
pub struct ChannelMessage {
    pub channel_name: String,
    pub sender: String,
    pub text: String,
    pub session_id: String,
    pub metadata: Value,
}

/// Abstract channel interface.
pub trait Channel: Send {
    fn name(&self) -> &str;
    fn start(&mut self) -> bool;
    fn stop(&mut self);
    fn is_running(&self) -> bool;
    fn send_message(&self, text: &str) -> Result<(), String>;
}

/// Registry of active channels.
pub struct ChannelRegistry {
    channels: Vec<Box<dyn Channel>>,
}

impl ChannelRegistry {
    pub fn new() -> Self {
        ChannelRegistry { channels: vec![] }
    }

    pub fn register(&mut self, channel: Box<dyn Channel>) {
        self.channels.push(channel);
    }

    pub fn start_all(&mut self) {
        for ch in &mut self.channels {
            if !ch.is_running() {
                if ch.start() {
                    log::info!("Channel '{}' started", ch.name());
                } else {
                    log::warn!("Channel '{}' failed to start", ch.name());
                }
            }
        }
    }

    pub fn stop_all(&mut self) {
        for ch in &mut self.channels {
            if ch.is_running() {
                ch.stop();
                log::info!("Channel '{}' stopped", ch.name());
            }
        }
    }

    pub fn broadcast(&self, text: &str) {
        for ch in &self.channels {
            if ch.is_running() {
                let _ = ch.send_message(text);
            }
        }
    }

    pub fn send_to(&self, channel_name: &str, text: &str) -> Result<(), String> {
        for ch in &self.channels {
            if ch.name() == channel_name && ch.is_running() {
                return ch.send_message(text);
            }
        }
        Err(format!("Channel '{}' not found or not running", channel_name))
    }

    pub fn has_channel(&self, name: &str) -> bool {
        self.channels.iter().any(|c| c.name() == name)
    }

    pub fn load_config(&mut self, config_path: &str) {
        let content = match std::fs::read_to_string(config_path) {
            Ok(c) => c,
            Err(_) => return,
        };
        let config: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return,
        };
        if let Some(channels) = config["channels"].as_array() {
            for ch in channels {
                let cfg = ChannelConfig {
                    name: ch["name"].as_str().unwrap_or("").to_string(),
                    channel_type: ch["type"].as_str().unwrap_or("").to_string(),
                    enabled: ch["enabled"].as_bool().unwrap_or(true),
                    settings: ch.get("settings").cloned().unwrap_or(Value::Null),
                };
                if let Some(channel) = channel_factory::create_channel(&cfg) {
                    self.register(channel);
                }
            }
        }
        log::info!("ChannelRegistry: loaded {} channels", self.channels.len());
    }
}

pub mod web_dashboard;
pub mod channel_factory;
pub mod webhook_channel;
pub mod telegram_client;
pub mod discord_channel;
pub mod slack_channel;
pub mod voice_channel;
pub mod a2a_handler;
pub mod mcp_client;
pub mod mcp_server;
