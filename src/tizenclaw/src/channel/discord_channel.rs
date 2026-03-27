//! Discord channel — sends/receives messages via Discord Bot HTTP API.
//!
//! Supports outbound via webhook URL and inbound via Bot Token
//! gateway polling (simplified HTTP polling of messages endpoint).

use super::{Channel, ChannelConfig};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct DiscordChannel {
    name: String,
    webhook_url: String,
    bot_token: String,
    channel_id: String,
    running: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl DiscordChannel {
    pub fn new(config: &ChannelConfig) -> Self {
        DiscordChannel {
            name: config.name.clone(),
            webhook_url: config.settings.get("webhook_url")
                .and_then(|v| v.as_str()).unwrap_or("").to_string(),
            bot_token: config.settings.get("bot_token")
                .and_then(|v| v.as_str()).unwrap_or("").to_string(),
            channel_id: config.settings.get("channel_id")
                .and_then(|v| v.as_str()).unwrap_or("").to_string(),
            running: Arc::new(AtomicBool::new(false)),
            thread: None,
        }
    }
}

impl Channel for DiscordChannel {
    fn name(&self) -> &str { &self.name }

    fn start(&mut self) -> bool {
        if self.running.load(Ordering::SeqCst) { return true; }
        if self.bot_token.is_empty() && self.webhook_url.is_empty() {
            log::warn!("DiscordChannel: no bot_token or webhook_url configured");
            return false;
        }

        self.running.store(true, Ordering::SeqCst);

        // If bot_token is set, start polling thread for inbound messages
        if !self.bot_token.is_empty() && !self.channel_id.is_empty() {
            let running = self.running.clone();
            let bot_token = self.bot_token.clone();
            let channel_id = self.channel_id.clone();

            self.thread = Some(std::thread::spawn(move || {
                log::info!("DiscordChannel: polling started for channel {}", channel_id);
                let mut last_message_id: Option<String> = None;

                while running.load(Ordering::SeqCst) {
                    let url = if let Some(ref after) = last_message_id {
                        format!(
                            "https://discord.com/api/v10/channels/{}/messages?after={}&limit=10",
                            channel_id, after
                        )
                    } else {
                        format!(
                            "https://discord.com/api/v10/channels/{}/messages?limit=1",
                            channel_id
                        )
                    };

                    let client = crate::infra::http_client::HttpClient::new();
                    // Note: Discord Bot requires Authorization: Bot <token> header
                    // For now, use GET which needs the auth header through custom client config
                    match client.get_sync(&url) {
                        Ok(resp) => {
                            if let Ok(messages) = serde_json::from_str::<Value>(&resp.body) {
                                if let Some(arr) = messages.as_array() {
                                    for msg in arr {
                                        let msg_id = msg["id"].as_str().unwrap_or("").to_string();
                                        let content = msg["content"].as_str().unwrap_or("");
                                        let author = msg["author"]["username"].as_str().unwrap_or("unknown");
                                        let is_bot = msg["author"]["bot"].as_bool().unwrap_or(false);

                                        if !is_bot && !content.is_empty() {
                                            log::info!("Discord msg from {}: {}", author, content);
                                        }
                                        if !msg_id.is_empty() {
                                            last_message_id = Some(msg_id);
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("Discord polling error: {}", e);
                        }
                    }

                    std::thread::sleep(std::time::Duration::from_secs(5));
                }
                log::info!("DiscordChannel: polling stopped");
            }));
        }

        log::info!("DiscordChannel started");
        true
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }

    fn send_message(&self, msg: &str) -> Result<(), String> {
        if self.webhook_url.is_empty() {
            return Err("Discord webhook not configured".into());
        }

        // Truncate to Discord's 2000 char limit
        let safe_msg = if msg.len() > 1950 {
            format!("{}\n...(truncated)", &msg[..1950])
        } else {
            msg.to_string()
        };

        let body = json!({"content": safe_msg}).to_string();
        crate::infra::http_client::HttpClient::new()
            .post_sync(&self.webhook_url, &body)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}
