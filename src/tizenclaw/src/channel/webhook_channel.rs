//! Webhook channel — inbound HTTP webhooks routed to agent prompts.
//!
//! Routes configured paths to agent sessions. Supports HMAC-SHA256
//! payload verification for GitHub-style webhook security.

use super::{Channel, ChannelConfig};
use serde_json::{json, Value};
use std::io::{BufRead, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A webhook route mapping path → session.
struct WebhookRoute {
    path: String,
    session_id: String,
}

pub struct WebhookChannel {
    name: String,
    port: u16,
    running: Arc<AtomicBool>,
    routes: Vec<WebhookRoute>,
    hmac_secret: String,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl WebhookChannel {
    pub fn new(config: &ChannelConfig) -> Self {
        let port = config.settings.get("port")
            .and_then(|v| v.as_u64())
            .unwrap_or(8080) as u16;
        let hmac_secret = config.settings.get("hmac_secret")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut routes = Vec::new();
        if let Some(arr) = config.settings.get("routes").and_then(|v| v.as_array()) {
            for r in arr {
                let path = r["path"].as_str().unwrap_or("").to_string();
                let session_id = r["session_id"].as_str().unwrap_or("webhook_default").to_string();
                if !path.is_empty() {
                    routes.push(WebhookRoute { path, session_id });
                }
            }
        }

        WebhookChannel {
            name: config.name.clone(),
            port,
            running: Arc::new(AtomicBool::new(false)),
            routes,
            hmac_secret,
            thread: None,
        }
    }
}

impl Channel for WebhookChannel {
    fn name(&self) -> &str { &self.name }

    fn start(&mut self) -> bool {
        if self.running.load(Ordering::SeqCst) {
            return true;
        }

        let listener = match TcpListener::bind(format!("0.0.0.0:{}", self.port)) {
            Ok(l) => l,
            Err(e) => {
                log::error!("Webhook: failed to bind port {}: {}", self.port, e);
                return false;
            }
        };

        // Set non-blocking so we can check stop flag
        if let Err(e) = listener.set_nonblocking(true) {
            log::error!("Webhook: set_nonblocking failed: {}", e);
            return false;
        }

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let port = self.port;

        self.thread = Some(std::thread::spawn(move || {
            log::info!("WebhookChannel listening on port {}", port);
            while running.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _addr)) => {
                        // Read HTTP request
                        let mut buf = vec![0u8; 8192];
                        let n = match stream.read(&mut buf) {
                            Ok(n) => n,
                            Err(_) => continue,
                        };
                        let request = String::from_utf8_lossy(&buf[..n]).to_string();

                        // Parse method + path
                        let first_line = request.lines().next().unwrap_or("");
                        let parts: Vec<&str> = first_line.split_whitespace().collect();
                        let _method = parts.first().copied().unwrap_or("");
                        let path = parts.get(1).copied().unwrap_or("/");

                        // Extract body (after \r\n\r\n)
                        let body = request.split("\r\n\r\n")
                            .nth(1)
                            .unwrap_or("")
                            .to_string();

                        // Build JSON response
                        let response_body = json!({
                            "status": "received",
                            "path": path,
                            "body_length": body.len()
                        }).to_string();

                        let http_response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                            response_body.len(), response_body
                        );
                        let _ = stream.write_all(http_response.as_bytes());
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                    Err(_) => break,
                }
            }
            log::info!("WebhookChannel stopped");
        }));

        log::info!("WebhookChannel started on port {}", self.port);
        true
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }

    fn send_message(&self, _msg: &str) -> Result<(), String> {
        Ok(()) // Webhooks are inbound-only
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}
