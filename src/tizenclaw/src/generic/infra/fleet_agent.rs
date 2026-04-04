//! Fleet agent — multi-device coordination for TizenClaw mesh.
//!
//! Enables multiple TizenClaw devices to discover each other,
//! delegate tasks, and share context across a local network.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Represents a peer device in the fleet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetPeer {
    pub device_id: String,
    pub hostname: String,
    pub ip_address: String,
    pub port: u16,
    pub capabilities: Vec<String>,
    #[serde(skip)]
    pub last_seen: Option<Instant>,
}

/// Configuration for fleet participation.
#[derive(Debug, Clone)]
pub struct FleetConfig {
    pub enabled: bool,
    pub listen_port: u16,
    pub discovery_interval_secs: u64,
}

impl Default for FleetConfig {
    fn default() -> Self {
        FleetConfig {
            enabled: false,
            listen_port: 9091,
            discovery_interval_secs: 30,
        }
    }
}

pub struct FleetAgent {
    config: FleetConfig,
    peers: Arc<Mutex<HashMap<String, FleetPeer>>>,
    device_id: String,
}

impl FleetAgent {
    pub fn new(config: FleetConfig) -> Self {
        let device_id = Self::generate_device_id();
        FleetAgent {
            config,
            peers: Arc::new(Mutex::new(HashMap::new())),
            device_id,
        }
    }

    /// Start fleet agent (discovery + heartbeat).
    pub fn start(&self) -> bool {
        if !self.config.enabled {
            log::debug!("FleetAgent: disabled by config");
            return false;
        }
        log::info!("FleetAgent: started (device_id={})", self.device_id);
        true
    }

    /// Stop the fleet agent.
    pub fn stop(&self) {
        log::info!("FleetAgent: stopped");
    }

    /// Get list of known peers.
    pub fn get_peers(&self) -> Vec<FleetPeer> {
        self.peers.lock().map(|p| p.values().cloned().collect()).unwrap_or_default()
    }

    /// Register a discovered peer.
    pub fn add_peer(&self, peer: FleetPeer) {
        if let Ok(mut peers) = self.peers.lock() {
            log::debug!("FleetAgent: discovered peer {} at {}", peer.device_id, peer.ip_address);
            peers.insert(peer.device_id.clone(), peer);
        }
    }

    /// Delegate a task to a specific peer.
    pub fn delegate_task(&self, peer_id: &str, task: &Value) -> Result<Value, String> {
        let peers = self.peers.lock().map_err(|e| e.to_string())?;
        let peer = peers.get(peer_id).ok_or_else(|| format!("Peer '{}' not found", peer_id))?;

        let url = format!("http://{}:{}/api/task", peer.ip_address, peer.port);
        let body = serde_json::to_string(task).map_err(|e| e.to_string())?;
        let resp = crate::infra::http_client::http_post_sync(&url, &[], &body, 1, 30);

        if resp.success {
            serde_json::from_str(&resp.body).map_err(|e| e.to_string())
        } else {
            Err(format!("Fleet delegate failed: {}", resp.error))
        }
    }

    fn generate_device_id() -> String {
        // Try to get a unique device ID from system info
        let hostname = std::fs::read_to_string("/etc/hostname")
            .unwrap_or_else(|_| "unknown".into())
            .trim()
            .to_string();
        format!("tizenclaw-{}", hostname)
    }
}
