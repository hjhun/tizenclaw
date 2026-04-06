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
    fn configure(&mut self, _settings: &Value) -> Result<(), String> {
        Ok(())
    }
}

/// Registry of active channels.
///
/// Each channel entry tracks an `auto_start` flag derived from
/// `enabled` in `channel_config.json`.  `start_all()` respects this
/// flag; `start_channel()` / `stop_channel()` ignore it and can be
/// called at any time (e.g. via IPC from the CLI).
pub struct ChannelRegistry {
    channels: Vec<Box<dyn Channel>>,
    auto_start: Vec<bool>,
}

impl Default for ChannelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ChannelRegistry {
    pub fn new() -> Self {
        ChannelRegistry {
            channels: vec![],
            auto_start: vec![],
        }
    }

    /// Register a channel.  `auto_start` controls whether
    /// `start_all()` will start it automatically on daemon boot.
    pub fn register(&mut self, channel: Box<dyn Channel>, auto_start: bool) {
        self.channels.push(channel);
        self.auto_start.push(auto_start);
    }

    /// Start all channels whose `auto_start` flag is true.
    pub fn start_all(&mut self) {
        for (ch, &auto) in self.channels.iter_mut().zip(self.auto_start.iter()) {
            if !auto || ch.is_running() {
                continue;
            }
            if ch.start() {
                log::info!("Channel '{}' started", ch.name());
            } else {
                log::warn!("Channel '{}' failed to start", ch.name());
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

    /// Start a specific channel by name regardless of its auto_start flag.
    pub fn start_channel(&mut self, name: &str, settings: Option<&Value>) -> Result<(), String> {
        for ch in &mut self.channels {
            if ch.name() == name {
                if ch.is_running() && settings.is_none() {
                    return Ok(());
                }
                if ch.is_running() {
                    ch.stop();
                }
                if let Some(settings) = settings {
                    ch.configure(settings)?;
                }
                if ch.start() {
                    log::info!("Channel '{}' started on demand", name);
                    return Ok(());
                }
                return Err(format!("Channel '{}' failed to start", name));
            }
        }
        Err(format!("Channel '{}' not registered", name))
    }

    /// Stop a specific channel by name.
    pub fn stop_channel(&mut self, name: &str) -> Result<(), String> {
        for ch in &mut self.channels {
            if ch.name() == name {
                if ch.is_running() {
                    ch.stop();
                    log::info!("Channel '{}' stopped on demand", name);
                }
                return Ok(());
            }
        }
        Err(format!("Channel '{}' not registered", name))
    }

    /// Returns Some(is_running) if the channel is registered, None otherwise.
    pub fn channel_status(&self, name: &str) -> Option<bool> {
        self.channels
            .iter()
            .find(|c| c.name() == name)
            .map(|c| c.is_running())
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
        Err(format!(
            "Channel '{}' not found or not running",
            channel_name
        ))
    }

    pub fn has_channel(&self, name: &str) -> bool {
        self.channels.iter().any(|c| c.name() == name)
    }

    pub fn load_config(
        &mut self,
        config_path: &str,
        agent: Option<std::sync::Arc<crate::core::agent_core::AgentCore>>,
    ) {
        let content = std::fs::read_to_string(config_path).unwrap_or_else(|_| "{}".to_string());
        let config: Value = serde_json::from_str(&content).unwrap_or(serde_json::json!({}));

        let mut telegram_loaded = false;

        if let Some(channels) = config["channels"].as_array() {
            for ch in channels {
                let enabled = ch["enabled"].as_bool().unwrap_or(true);
                let cfg = ChannelConfig {
                    name: ch["name"].as_str().unwrap_or("").to_string(),
                    channel_type: ch["type"].as_str().unwrap_or("").to_string(),
                    enabled,
                    settings: ch.get("settings").cloned().unwrap_or(Value::Null),
                };
                if cfg.channel_type == "telegram" {
                    telegram_loaded = true;
                }
                if let Some(channel) = channel_factory::create_channel(&cfg, agent.clone()) {
                    self.register(channel, enabled);
                }
            }
        }

        if !telegram_loaded {
            let tg_config_path = std::path::Path::new(config_path)
                .parent()
                .unwrap_or(std::path::Path::new(""))
                .join("telegram_config.json");
            if tg_config_path.exists() {
                log::debug!("ChannelRegistry: Autodiscovered telegram_config.json");
                let cfg = ChannelConfig {
                    name: "telegram".into(),
                    channel_type: "telegram".into(),
                    enabled: true,
                    settings: serde_json::json!({}),
                };
                if let Some(channel) = channel_factory::create_channel(&cfg, agent) {
                    self.register(channel, true);
                }
            }
        }

        log::info!("ChannelRegistry: loaded {} channels", self.channels.len());
    }
}

pub mod a2a_handler;
pub mod channel_factory;
pub mod discord_channel;
pub mod mcp_client;
pub mod mcp_server;
pub mod slack_channel;
pub mod telegram_client;
pub mod voice_channel;
pub mod web_dashboard;
pub mod webhook_channel;
