//! Discord channel — outbound Discord webhook sender.

use super::{split_message_chunks, Channel, ChannelConfig};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};

const DISCORD_MAX_MESSAGE_CHARS: usize = 2000;
const HTTP_TIMEOUT_SECS: u64 = 10;

pub struct DiscordChannel {
    name: String,
    webhook_url: String,
    username: String,
    active: AtomicBool,
}

impl DiscordChannel {
    pub fn from_config(config: &Value) -> Result<Self, String> {
        let webhook_url = config
            .get("webhook_url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "discord webhook_url is required".to_string())?;

        let username = config
            .get("username")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("TizenClaw");

        Ok(Self {
            name: config
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("discord")
                .to_string(),
            webhook_url: webhook_url.to_string(),
            username: username.to_string(),
            active: AtomicBool::new(true),
        })
    }

    pub fn new(config: &ChannelConfig) -> Self {
        let mut settings = config.settings.clone();
        if let Some(object) = settings.as_object_mut() {
            object
                .entry("name".to_string())
                .or_insert_with(|| Value::String(config.name.clone()));
        }

        Self::from_config(&settings).unwrap_or_else(|_| Self {
            name: config.name.clone(),
            webhook_url: String::new(),
            username: "TizenClaw".to_string(),
            active: AtomicBool::new(false),
        })
    }

    fn send_chunk(&self, chunk: &str) -> Result<(), String> {
        let body = json!({
            "content": chunk,
            "username": self.username,
        });

        let response = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| err.to_string())?
            .block_on(async {
                reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
                    .build()
                    .map_err(|err| err.to_string())?
                    .post(&self.webhook_url)
                    .header("Content-Type", "application/json")
                    .body(body.to_string())
                    .send()
                    .await
                    .map_err(|err| err.to_string())
            })?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!("HTTP {}", response.status().as_u16()))
        }
    }
}

impl Channel for DiscordChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn start(&mut self) -> bool {
        let ready = !self.webhook_url.is_empty();
        self.active.store(ready, Ordering::SeqCst);
        ready
    }

    fn stop(&mut self) {
        self.active.store(false, Ordering::SeqCst);
    }

    fn is_running(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    fn send_message(&self, msg: &str) -> Result<(), String> {
        if self.webhook_url.is_empty() {
            log::warn!("Discord channel '{}' is not configured", self.name);
            return Err("Discord webhook not configured".into());
        }

        let mut errors = Vec::new();
        for chunk in split_message_chunks(msg, DISCORD_MAX_MESSAGE_CHARS) {
            if let Err(err) = self.send_chunk(&chunk) {
                log::warn!("Discord channel '{}' send failed: {}", self.name, err);
                errors.push(err);
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn status(&self) -> Value {
        json!({
            "name": self.name(),
            "running": self.is_running(),
            "username": self.username,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::DiscordChannel;
    use crate::channel::{split_message_chunks, Channel};
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    fn spawn_http_recorder(response_body: &'static str) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind recorder");
        let addr = listener.local_addr().expect("local addr");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);

        std::thread::spawn(move || {
            for _ in 0..4 {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let mut buffer = [0_u8; 8192];
                let size = stream.read(&mut buffer).unwrap_or(0);
                captured
                    .lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&buffer[..size]).to_string());

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });

        (format!("http://{}", addr), requests)
    }

    #[test]
    fn discord_messages_split_at_word_boundaries() {
        let chunk = "word ".repeat(600);
        let parts = split_message_chunks(&chunk, 2000);

        assert!(parts.len() > 1);
        assert!(parts.iter().all(|part| part.chars().count() <= 2000));
        assert!(parts.iter().all(|part| !part.ends_with(' ')));
    }

    #[test]
    fn send_message_posts_username_and_content() {
        let (url, requests) = spawn_http_recorder("{}");
        let mut channel = DiscordChannel::from_config(&json!({
            "name": "discord",
            "webhook_url": url,
            "username": "Bridge",
        }))
        .expect("discord config");

        assert!(channel.start());
        channel.send_message("hello discord").expect("send");

        let request_dump = requests.lock().unwrap().join("\n");
        assert!(request_dump.contains("\"content\":\"hello discord\""));
        assert!(request_dump.contains("\"username\":\"Bridge\""));
    }
}
