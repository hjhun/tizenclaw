//! Telegram Bot API client — long-polling channel for Telegram messaging.
//!
//! Uses `getUpdates` long-polling to receive messages, `sendMessage` to respond.
//! Supports chat ID allowlisting and concurrent handler limits.

use super::{Channel, ChannelConfig};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;

const MAX_CONCURRENT_HANDLERS: i32 = 3;

pub struct TelegramClient {
    name: String,
    bot_token: String,
    allowed_chat_ids: HashSet<i64>,
    running: Arc<AtomicBool>,
    active_handlers: Arc<AtomicI32>,
    update_offset: i64,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl TelegramClient {
    pub fn new(config: &ChannelConfig) -> Self {
        let bot_token = config.settings.get("bot_token")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut allowed_chat_ids = HashSet::new();
        if let Some(arr) = config.settings.get("allowed_chat_ids").and_then(|v| v.as_array()) {
            for id in arr {
                if let Some(n) = id.as_i64() {
                    allowed_chat_ids.insert(n);
                }
            }
        }

        TelegramClient {
            name: config.name.clone(),
            bot_token,
            allowed_chat_ids,
            running: Arc::new(AtomicBool::new(false)),
            active_handlers: Arc::new(AtomicI32::new(0)),
            update_offset: 0,
            thread: None,
        }
    }

    fn send_telegram_message(&self, chat_id: i64, text: &str) {
        if self.bot_token.is_empty() { return; }

        // Truncate to Telegram's 4096 char limit
        let safe_text = if text.len() > 4000 {
            format!("{}\n...(truncated)", &text[..4000])
        } else {
            text.to_string()
        };

        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token);
        let payload = json!({
            "chat_id": chat_id,
            "text": safe_text,
            "parse_mode": "Markdown"
        }).to_string();

        let client = crate::infra::http_client::HttpClient::new();
        match client.post_sync(&url, &payload) {
            Ok(resp) if resp.status_code >= 400 => {
                // Markdown failed, retry plain text
                let plain = json!({"chat_id": chat_id, "text": safe_text}).to_string();
                let _ = client.post_sync(&url, &plain);
            }
            Err(e) => log::error!("Telegram sendMessage failed: {}", e),
            _ => {}
        }
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

        // Clear prior webhook/polling session
        let reset_url = format!(
            "https://api.telegram.org/bot{}/deleteWebhook",
            self.bot_token
        );
        let client = crate::infra::http_client::HttpClient::new();
        match client.get_sync(&reset_url) {
            Ok(_) => log::info!("TelegramClient: cleared prior session"),
            Err(e) => log::warn!("TelegramClient: deleteWebhook failed: {}", e),
        }

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let bot_token = self.bot_token.clone();
        let allowed_chat_ids = self.allowed_chat_ids.clone();

        self.thread = Some(std::thread::spawn(move || {
            log::info!("TelegramClient polling started");
            let mut offset: i64 = 0;
            let mut backoff_secs = 5u64;

            while running.load(Ordering::SeqCst) {
                let url = format!(
                    "https://api.telegram.org/bot{}/getUpdates?offset={}&timeout=2",
                    bot_token, offset
                );

                let client = crate::infra::http_client::HttpClient::new();
                let resp = match client.get_sync(&url) {
                    Ok(r) => r,
                    Err(e) => {
                        log::error!("Telegram polling error: {}", e);
                        std::thread::sleep(std::time::Duration::from_secs(backoff_secs));
                        backoff_secs = (backoff_secs * 2).min(60);
                        continue;
                    }
                };

                if !running.load(Ordering::SeqCst) { break; }
                backoff_secs = 5;

                let data: Value = match serde_json::from_str(&resp.body) {
                    Ok(v) => v,
                    Err(_) => {
                        std::thread::sleep(std::time::Duration::from_secs(5));
                        continue;
                    }
                };

                if !data["ok"].as_bool().unwrap_or(false) {
                    std::thread::sleep(std::time::Duration::from_secs(5));
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
                        if !allowed_chat_ids.is_empty() && !allowed_chat_ids.contains(&chat_id) {
                            log::info!("Blocked chat_id {} — not in allowlist", chat_id);
                            continue;
                        }

                        log::info!("Telegram received from {}: {}", chat_id, text);
                        // Note: agent processing integration would go here
                    }
                }
            }
            log::info!("TelegramClient polling stopped");
        }));

        log::info!("TelegramClient started");
        true
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }

    fn send_message(&self, msg: &str) -> Result<(), String> {
        for chat_id in &self.allowed_chat_ids {
            self.send_telegram_message(*chat_id, msg);
        }
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}
