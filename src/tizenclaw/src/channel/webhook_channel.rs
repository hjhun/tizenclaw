//! Webhook channel — generic outbound HTTP webhook sender.

use super::{Channel, ChannelConfig};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

const HTTP_TIMEOUT_SECS: u64 = 10;

pub struct WebhookChannel {
    name: String,
    url: String,
    method: String,
    headers: HashMap<String, String>,
    payload_template: String,
    active: AtomicBool,
}

impl WebhookChannel {
    pub fn from_config(config: &Value) -> Result<Self, String> {
        let url = config
            .get("url")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "webhook url is required".to_string())?;

        let method = config
            .get("method")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("POST")
            .to_uppercase();
        if method != "POST" && method != "PUT" {
            return Err("webhook method must be POST or PUT".to_string());
        }

        let headers = config
            .get("headers")
            .and_then(Value::as_object)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|(key, value)| {
                        value.as_str().map(|value| (key.clone(), value.to_string()))
                    })
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();

        let payload_template = config
            .get("payload_template")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| "{\"message\":\"{message}\"}".to_string());

        Ok(Self {
            name: config
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("webhook")
                .to_string(),
            url: url.to_string(),
            method,
            headers,
            payload_template,
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
            url: String::new(),
            method: "POST".to_string(),
            headers: HashMap::new(),
            payload_template: "{\"message\":\"{message}\"}".to_string(),
            active: AtomicBool::new(false),
        })
    }

    fn render_payload(&self, message: &str) -> String {
        let escaped = serde_json::to_string(message).unwrap_or_else(|_| "\"\"".to_string());
        let escaped = escaped.trim_matches('"');
        self.payload_template.replace("{message}", escaped)
    }
}

impl Channel for WebhookChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn start(&mut self) -> bool {
        let ready = !self.url.is_empty();
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
        if self.url.is_empty() {
            log::warn!("Webhook channel '{}' is not configured", self.name);
            return Err("Webhook URL not configured".into());
        }

        let payload = self.render_payload(msg);
        let response = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|err| err.to_string())?
            .block_on(async {
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
                    .build()
                    .map_err(|err| err.to_string())?;
                let method = reqwest::Method::from_bytes(self.method.as_bytes())
                    .map_err(|err| err.to_string())?;
                let mut request = client.request(method, &self.url);
                request = request.header("Content-Type", "application/json");
                for (key, value) in &self.headers {
                    request = request.header(key, value);
                }
                request
                    .body(payload)
                    .send()
                    .await
                    .map_err(|err| err.to_string())
            });

        match response {
            Ok(response) if response.status().is_success() => Ok(()),
            Ok(response) => {
                let err = format!("HTTP {}", response.status().as_u16());
                log::warn!("Webhook channel '{}' send failed: {}", self.name, err);
                Err(err)
            }
            Err(err) => {
                log::warn!("Webhook channel '{}' send failed: {}", self.name, err);
                Err(err)
            }
        }
    }

    fn status(&self) -> Value {
        json!({
            "name": self.name(),
            "running": self.is_running(),
            "method": self.method,
            "headers": self.headers.keys().collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::WebhookChannel;
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
    fn webhook_uses_configured_method_headers_and_payload() {
        let (url, requests) = spawn_http_recorder();
        let mut channel = WebhookChannel::from_config(&json!({
            "url": url,
            "method": "PUT",
            "headers": {
                "X-Test": "present"
            },
            "payload_template": "{\"wrapped\":\"{message}\"}"
        }))
        .expect("webhook config");

        assert!(channel.start());
        channel.send_message("hello world").expect("send");

        let request_dump = requests.lock().unwrap().join("\n");
        let request_dump_lower = request_dump.to_ascii_lowercase();
        assert!(request_dump.starts_with("PUT "));
        assert!(request_dump_lower.contains("x-test: present"));
        assert!(request_dump.contains("{\"wrapped\":\"hello world\"}"));
    }
}
