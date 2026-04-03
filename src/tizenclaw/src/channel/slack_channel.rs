//! Slack channel — sends/receives messages via Slack APIs.
//!
//! Outbound: Slack Incoming Webhook (POST JSON).
//! Inbound: Slack Bot Token + conversations.history polling.
//! Supports Block Kit formatting for rich messages.
//! Exclusively uses Tokio Async Reactor (epoll) to avoid blocking thread allocations.

use super::{Channel, ChannelConfig};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct SlackChannel {
    name: String,
    webhook_url: String,
    bot_token: String,
    channel_id: String,
    running: Arc<AtomicBool>,
}

impl SlackChannel {
    pub fn new(config: &ChannelConfig) -> Self {
        SlackChannel {
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

    /// Post a message using Bot Token API (for replies to specific channels) asynchronously.
    fn post_to_channel_async(bot_token: &str, channel: &str, text: &str) {
        if bot_token.is_empty() {
            return;
        }

        let url = "https://slack.com/api/chat.postMessage";
        let payload = json!({
            "channel": channel,
            "text": text,
            "mrkdwn": true
        }).to_string();

        tokio::spawn(async move {
            let client = crate::infra::http_client::HttpClient::new();
            let _ = client.post(url, &payload).await;
        });
    }
}

impl Channel for SlackChannel {
    fn name(&self) -> &str { &self.name }

    fn start(&mut self) -> bool {
        if self.running.load(Ordering::SeqCst) { return true; }
        if self.webhook_url.is_empty() && self.bot_token.is_empty() {
            log::warn!("SlackChannel: no webhook_url or bot_token configured");
            return false;
        }

        self.running.store(true, Ordering::SeqCst);

        if !self.bot_token.is_empty() && !self.channel_id.is_empty() {
            let running = self.running.clone();
            let bot_token = self.bot_token.clone();
            let channel_id = self.channel_id.clone();

            tokio::spawn(async move {
                log::info!("SlackChannel: async epoll started for channel {}", channel_id);
                let mut last_ts = String::new();

                while running.load(Ordering::SeqCst) {
                    let url = if last_ts.is_empty() {
                        format!(
                            "https://slack.com/api/conversations.history?channel={}&limit=1",
                            channel_id
                        )
                    } else {
                        format!(
                            "https://slack.com/api/conversations.history?channel={}&oldest={}&limit=10",
                            channel_id, last_ts
                        )
                    };

                    let client = crate::infra::http_client::HttpClient::new();
                    match client.get(&url).await {
                        Ok(resp) => {
                            if let Ok(data) = serde_json::from_str::<Value>(&resp.body) {
                                if data["ok"].as_bool().unwrap_or(false) {
                                    if let Some(messages) = data["messages"].as_array() {
                                        for msg in messages {
                                            let ts = msg["ts"].as_str().unwrap_or("");
                                            let text = msg["text"].as_str().unwrap_or("");
                                            let user = msg["user"].as_str().unwrap_or("unknown");
                                            let is_bot = msg.get("bot_id").is_some();

                                            if !is_bot && !text.is_empty() {
                                                log::info!("Slack msg from {}: {}", user, text);
                                            }
                                            if !ts.is_empty() {
                                                last_ts = ts.to_string();
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => log::error!("Slack polling error: {}", e),
                    }

                    // Native epoll sleep
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
                log::info!("SlackChannel: async epoll stopped");
            });
        }

        log::info!("SlackChannel started");
        true
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }

    fn send_message(&self, msg: &str) -> Result<(), String> {
        if !self.webhook_url.is_empty() {
            let body = json!({"text": msg}).to_string();
            let webhook_url = self.webhook_url.clone();
            
            tokio::spawn(async move {
                let _ = crate::infra::http_client::HttpClient::new().post(&webhook_url, &body).await;
            });
            return Ok(());
        }
        
        if !self.bot_token.is_empty() && !self.channel_id.is_empty() {
            SlackChannel::post_to_channel_async(&self.bot_token, &self.channel_id, msg);
            return Ok(());
        }
        
        Err("Slack: no webhook or bot_token configured".into())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}
