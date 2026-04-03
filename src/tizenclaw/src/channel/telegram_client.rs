//! Telegram Bot API client — async long-polling channel.
//!
//! Uses `getUpdates` long-polling to receive messages. Polls natively
//! on the Tokio async reactor (epoll) avoiding expensive thread allocation.

use super::{Channel, ChannelConfig};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;

const MAX_CONCURRENT_HANDLERS: i32 = 3;

pub struct TelegramClient {
    name: String,
    bot_token: String,
    allowed_chat_ids: Arc<HashSet<i64>>,
    running: Arc<AtomicBool>,
    active_handlers: Arc<AtomicI32>,
    agent: Option<Arc<crate::core::agent_core::AgentCore>>,
}

impl TelegramClient {
    pub fn new(config: &ChannelConfig, agent: Option<Arc<crate::core::agent_core::AgentCore>>) -> Self {
        let mut bot_token = config.settings.get("bot_token")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut allowed_ids = HashSet::new();
        if let Some(arr) = config.settings.get("allowed_chat_ids").and_then(|v| v.as_array()) {
            for id in arr {
                if let Some(n) = id.as_i64() {
                    allowed_ids.insert(n);
                }
            }
        }

        // Try load from unified config file
        if let Ok(content) = std::fs::read_to_string("/opt/usr/share/tizenclaw/config/telegram_config.json") {
            if let Ok(json) = serde_json::from_str::<Value>(&content) {
                if let Some(token) = json.get("bot_token").and_then(|v| v.as_str()) {
                    if !token.is_empty() {
                        bot_token = token.to_string();
                        log::info!("TelegramClient: loaded bot_token override");
                    }
                }
                if let Some(arr) = json.get("allowed_chat_ids").and_then(|v| v.as_array()) {
                    if !arr.is_empty() {
                        allowed_ids.clear();
                        for id in arr {
                            if let Some(n) = id.as_i64() {
                                allowed_ids.insert(n);
                            }
                        }
                    }
                }
            }
        }

        TelegramClient {
            name: config.name.clone(),
            bot_token,
            allowed_chat_ids: Arc::new(allowed_ids),
            running: Arc::new(AtomicBool::new(false)),
            active_handlers: Arc::new(AtomicI32::new(0)),
            agent,
        }
    }

    // Static so it can be called inside spawned async tasks easily
    fn send_telegram_message(bot_token: &str, chat_id: i64, text: &str) {
        if bot_token.is_empty() { return; }

        let safe_text = if text.len() > 4000 {
            format!("{}\n...(truncated)", &text[..4000])
        } else {
            text.to_string()
        };

        let url = format!("https://api.telegram.org/bot{}/sendMessage", bot_token);
        let payload = json!({
            "chat_id": chat_id,
            "text": safe_text,
            "parse_mode": "Markdown"
        }).to_string();

        let client = crate::infra::http_client::HttpClient::new();
        tokio::spawn(async move {
            match client.post(&url, &payload).await {
                Ok(resp) if resp.status_code >= 400 => {
                    let plain = json!({"chat_id": chat_id, "text": safe_text}).to_string();
                    let _ = client.post(&url, &plain).await;
                }
                Err(e) => log::error!("Telegram sendMessage failed: {}", e),
                _ => {}
            }
        });
    }
}

impl Channel for TelegramClient {
    fn name(&self) -> &str { &self.name }

    fn start(&mut self) -> bool {
        if self.running.load(Ordering::SeqCst) { return true; }
        if self.bot_token.is_empty() || self.bot_token == "YOUR_TELEGRAM_BOT_TOKEN_HERE" {
            log::warn!("TelegramClient: invalid bot token");
            return false;
        }

        // Clear Webhook (in case user had it previously configured from testing)
        let reset_url = format!("https://api.telegram.org/bot{}/deleteWebhook", self.bot_token);
        let client = crate::infra::http_client::HttpClient::new();
        let _ = client.get_sync(&reset_url);

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let bot_token = self.bot_token.clone();
        let allowed_ids = self.allowed_chat_ids.clone();
        let active_handlers = self.active_handlers.clone();
        let agent = self.agent.clone();

        tokio::spawn(async move {
            log::debug!("TelegramClient async epoll reactor started");
            let mut offset: i64 = 0;
            let mut backoff_secs = 5u64;

            while running.load(Ordering::SeqCst) {
                let url = format!(
                    "https://api.telegram.org/bot{}/getUpdates?offset={}&timeout=50",
                    bot_token, offset
                );

                let client = crate::infra::http_client::HttpClient::new();
                let resp = match client.get(&url).await {
                    Ok(r) => r,
                    Err(e) => {
                        log::error!("Telegram polling error: {}", e);
                        tokio::time::sleep(tokio::time::Duration::from_secs(backoff_secs)).await;
                        backoff_secs = (backoff_secs * 2).min(60);
                        continue;
                    }
                };

                if !running.load(Ordering::SeqCst) { break; }
                backoff_secs = 5;

                let data: Value = match serde_json::from_str(&resp.body) {
                    Ok(v) => v,
                    Err(_) => {
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                        continue;
                    }
                };

                if !data["ok"].as_bool().unwrap_or(false) {
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    continue;
                }

                if let Some(results) = data["result"].as_array() {
                    for item in results {
                        offset = item["update_id"].as_i64().unwrap_or(0) + 1;
                        let msg = match item.get("message") {
                            Some(m) => m,
                            None => continue,
                        };
                        let text = msg["text"].as_str().unwrap_or("");
                        let chat_id = msg["chat"]["id"].as_i64().unwrap_or(0);

                        if text.is_empty() || chat_id == 0 { continue; }
                        if !allowed_ids.is_empty() && !allowed_ids.contains(&chat_id) {
                            log::debug!("Blocked chat_id {} — not in allowlist", chat_id);
                            continue;
                        }

                        let current_handlers = active_handlers.load(Ordering::SeqCst);
                        if current_handlers >= MAX_CONCURRENT_HANDLERS {
                            log::warn!("Telegram dropping message: max concurrent handlers ({}) reached", current_handlers);
                            continue;
                        }

                        log::info!("Telegram received from {}: {}", chat_id, text);
                        
                        if let Some(agent_core) = agent.clone() {
                            active_handlers.fetch_add(1, Ordering::SeqCst);
                            let text_clone = text.to_string();
                            let bot_token_clone = bot_token.clone();
                            let session_id = format!("tg_{}", chat_id);
                            let active_handlers_clone = active_handlers.clone();
                            
                            tokio::spawn(async move {
                                let result = agent_core.process_prompt(&session_id, &text_clone, None).await;
                                TelegramClient::send_telegram_message(&bot_token_clone, chat_id, &result);
                                active_handlers_clone.fetch_sub(1, Ordering::SeqCst);
                            });
                        }
                    }
                }
            }
            log::debug!("TelegramClient async epoll reactor stopped");
        });

        log::info!("TelegramClient started");
        true
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }

    fn send_message(&self, msg: &str) -> Result<(), String> {
        for chat_id in self.allowed_chat_ids.iter() {
            Self::send_telegram_message(&self.bot_token, *chat_id, msg);
        }
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}
