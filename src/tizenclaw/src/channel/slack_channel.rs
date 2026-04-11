//! Slack channel — outbound Slack incoming webhook sender.

use super::{Channel, ChannelConfig};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};

const HTTP_TIMEOUT_SECS: u64 = 10;

pub struct SlackChannel {
    name: String,
    webhook_url: String,
    channel: Option<String>,
    username: String,
    active: AtomicBool,
}

impl SlackChannel {
    pub fn from_config(config: &Value) -> Result<Self, String> {
        let webhook_url = config
            .get("webhook_url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "slack webhook_url is required".to_string())?;

        let channel = config
            .get("channel")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
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
                .unwrap_or("slack")
                .to_string(),
            webhook_url: webhook_url.to_string(),
            channel,
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
            channel: None,
            username: "TizenClaw".to_string(),
            active: AtomicBool::new(false),
        })
    }
}

impl Channel for SlackChannel {
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
            log::warn!("Slack channel '{}' is not configured", self.name);
            return Err("Slack webhook not configured".into());
        }

        let mut body = json!({
            "text": msg,
            "username": self.username,
        });
        if let Some(channel) = &self.channel {
            body["channel"] = Value::String(channel.clone());
        }

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
            });

        match response {
            Ok(response) if response.status().is_success() => Ok(()),
            Ok(response) => {
                let err = format!("HTTP {}", response.status().as_u16());
                log::warn!("Slack channel '{}' send failed: {}", self.name, err);
                Err(err)
            }
            Err(err) => {
                log::warn!("Slack channel '{}' send failed: {}", self.name, err);
                Err(err)
            }
        }
    }

    fn status(&self) -> Value {
        json!({
            "name": self.name(),
            "running": self.is_running(),
            "channel": self.channel,
            "username": self.username,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::SlackChannel;
    use crate::channel::Channel;
    use serde_json::json;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    fn spawn_http_recorder() -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind recorder");
        let addr = listener.local_addr().expect("local addr");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);

        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buffer = [0_u8; 8192];
            let size = stream.read(&mut buffer).unwrap_or(0);
            captured
                .lock()
                .unwrap()
                .push(String::from_utf8_lossy(&buffer[..size]).to_string());

            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok");
        });

        (format!("http://{}", addr), requests)
    }

    #[test]
    fn slack_send_includes_optional_channel_override() {
        let (url, requests) = spawn_http_recorder();
        let mut channel = SlackChannel::from_config(&json!({
            "webhook_url": url,
            "channel": "#ops",
            "username": "TizenClaw Bot"
        }))
        .expect("slack config");

        assert!(channel.start());
        channel.send_message("status ping").expect("send");

        let request_dump = requests.lock().unwrap().join("\n");
        assert!(request_dump.contains("\"channel\":\"#ops\""));
        assert!(request_dump.contains("\"username\":\"TizenClaw Bot\""));
        assert!(request_dump.contains("\"text\":\"status ping\""));
    }
}
