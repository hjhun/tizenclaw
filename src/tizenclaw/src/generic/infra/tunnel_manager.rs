//! Tunnel manager — manages ngrok/reverse tunnels for external access.
//!
//! Ported from C++ TunnelManager: spawns an ngrok process, monitors the local
//! API to discover the public URL, and stops the process on shutdown.

use serde_json::Value;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub struct TunnelManager {
    config_file: String,
    provider: String,
    auth_token: String,
    custom_domain: String,
    local_port: u16,
    running: Arc<AtomicBool>,
    public_url: Arc<Mutex<String>>,
    child: Option<Child>,
    monitor_handle: Option<std::thread::JoinHandle<()>>,
}

impl TunnelManager {
    pub fn new(config_file: &str) -> Self {
        let mut mgr = TunnelManager {
            config_file: config_file.to_string(),
            provider: "none".into(),
            auth_token: String::new(),
            custom_domain: String::new(),
            local_port: 0,
            running: Arc::new(AtomicBool::new(false)),
            public_url: Arc::new(Mutex::new(String::new())),
            child: None,
            monitor_handle: None,
        };
        mgr.load_config();
        mgr
    }

    fn load_config(&mut self) -> bool {
        let content = match std::fs::read_to_string(&self.config_file) {
            Ok(c) => c,
            Err(_) => {
                log::warn!("Tunnel config not found: {}", self.config_file);
                return false;
            }
        };
        match serde_json::from_str::<Value>(&content) {
            Ok(j) => {
                self.provider = j["provider"].as_str().unwrap_or("none").to_string();
                self.auth_token = j["auth_token"].as_str().unwrap_or("").to_string();
                self.custom_domain = j["custom_domain"].as_str().unwrap_or("").to_string();
                log::info!("Loaded tunnel config. Provider: {}", self.provider);
                true
            }
            Err(e) => {
                log::error!("Failed to parse tunnel config: {}", e);
                false
            }
        }
    }

    /// Start the tunnel process for the given local port.
    pub fn start(&mut self, local_port: u16) -> bool {
        if self.running.load(Ordering::SeqCst) {
            return true;
        }

        self.local_port = local_port;

        if self.provider != "ngrok" {
            if self.provider != "none" {
                log::warn!("Tunnel provider '{}' is not supported yet.", self.provider);
            }
            return false;
        }

        if self.auth_token.is_empty() {
            log::warn!("ngrok auth token is empty. Tunnel may not start or have rate limits.");
        }

        // Build ngrok command
        let mut args = vec![
            "http".to_string(),
            local_port.to_string(),
        ];

        if !self.auth_token.is_empty() {
            args.push("--authtoken".into());
            args.push(self.auth_token.clone());
        }
        if !self.custom_domain.is_empty() {
            args.push("--domain".into());
            args.push(self.custom_domain.clone());
        }
        args.push("--log=stdout".into());
        args.push("--log-format=default".into());

        log::info!("Starting ngrok tunnel for port {}", local_port);

        match Command::new("ngrok")
            .args(&args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => {
                self.child = Some(child);
                self.running.store(true, Ordering::SeqCst);

                // Start monitor thread to discover public URL
                let running = self.running.clone();
                let url = self.public_url.clone();
                let port = self.local_port;
                self.monitor_handle = Some(std::thread::spawn(move || {
                    Self::monitor_tunnel(running, url, port);
                }));

                true
            }
            Err(e) => {
                log::error!("Failed to spawn ngrok: {}. Is it in PATH?", e);
                false
            }
        }
    }

    /// Stop the tunnel process.
    pub fn stop(&mut self) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }

        self.running.store(false, Ordering::SeqCst);

        if let Some(mut child) = self.child.take() {
            log::info!("Stopping ngrok tunnel (PID: {:?})", child.id());
            let _ = child.kill();
            let _ = child.wait();
        }

        if let Ok(mut url) = self.public_url.lock() {
            url.clear();
        }

        if let Some(handle) = self.monitor_handle.take() {
            let _ = handle.join();
        }
    }

    /// Get the public URL of the tunnel.
    pub fn get_public_url(&self) -> String {
        self.public_url.lock().map(|u| u.clone()).unwrap_or_default()
    }

    /// Monitor thread: polls ngrok local API to discover the public URL.
    fn monitor_tunnel(running: Arc<AtomicBool>, url: Arc<Mutex<String>>, port: u16) {
        let api_url = "http://127.0.0.1:4040/api/tunnels";

        for _ in 0..15 {
            if !running.load(Ordering::SeqCst) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));

            let resp = crate::infra::http_client::http_get_sync(api_url, &[], 0, 5);
            if resp.success && resp.status_code == 200 && !resp.body.is_empty() {
                if let Ok(j) = serde_json::from_str::<Value>(&resp.body) {
                    if let Some(tunnels) = j["tunnels"].as_array() {
                        if let Some(first) = tunnels.first() {
                            if let Some(pub_url) = first["public_url"].as_str() {
                                if !pub_url.is_empty() {
                                    log::debug!("========================================");
                                    log::debug!("Secure Tunnel Established!");
                                    log::debug!("Public URL: {}", pub_url);
                                    log::debug!("Routing to: localhost:{}", port);
                                    log::debug!("========================================");
                                    if let Ok(mut u) = url.lock() {
                                        *u = pub_url.to_string();
                                    }
                                    return;
                                }
                            }
                        }
                    }
                }
            }
        }

        if running.load(Ordering::SeqCst) {
            log::warn!("ngrok tunnel started but could not retrieve public URL after 15s.");
        }
    }
}

impl Drop for TunnelManager {
    fn drop(&mut self) {
        self.stop();
    }
}
