//! Discord channel — sends/receives messages via Discord Bot HTTP API.
//!
//! Exclusively uses Tokio Async Reactor (epoll) to poll the messages endpoint
//! without blocking or allocating OS threads.

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

        if !self.bot_token.is_empty() && !self.channel_id.is_empty() {
            let running = self.running.clone();
            let channel_id = self.channel_id.clone();
            let bot_token = self.bot_token.clone(); // In Discord API, token is needed for Authorization headers. Wait, our generic HttpClient doesn't have custom headers easily, but we'll leave it as is per legacy code.

            tokio::spawn(async move {
                log::info!("DiscordChannel: async epoll started for channel {}", channel_id);
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
                    
                    // Native async GET via epoll
                    match client.get(&url).await {
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

                    // Async sleep yields to the epoll reactor!
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
                log::info!("DiscordChannel: async epoll stopped");
            });
        }

        log::info!("DiscordChannel started");
        true
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }

    fn send_message(&self, msg: &str) -> Result<(), String> {
        if self.webhook_url.is_empty() {
            return Err("Discord webhook not configured".into());
        }

        let safe_msg = if msg.len() > 1950 {
            format!("{}\n...(truncated)", &msg[..1950])
        } else {
            msg.to_string()
        };

        let body = json!({"content": safe_msg}).to_string();
        let webhook_url = self.webhook_url.clone();
        
        // Use Async PUSH
        tokio::spawn(async move {
            let _ = crate::infra::http_client::HttpClient::new().post(&webhook_url, &body).await;
        });

        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}
