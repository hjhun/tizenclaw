//! mDNS Discovery — scanning and registering zero-config endpoints.

use mdns_sd::{ServiceDaemon, ServiceEvent};
use std::sync::{Arc, RwLock};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Peer {
    pub id: String,
    pub address: String,
    pub port: u16,
    pub last_seen: u64,
}

pub struct MdnsScanner {
    pub peers: Arc<RwLock<HashMap<String, Peer>>>,
}

impl Default for MdnsScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl MdnsScanner {
    pub fn new() -> Self {
        Self {
            peers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn start(&self) {
        let peers_clone = self.peers.clone();
        tokio::task::spawn_blocking(move || {
            let mdns = match ServiceDaemon::new() {
                Ok(m) => m,
                Err(e) => {
                    log::error!("mDNS initialization failed: {}. Fallback to isolated mode.", e);
                    return;
                }
            };

            let service_type = "_tizenclaw._tcp.local.";
            let receiver = match mdns.browse(service_type) {
                Ok(r) => r,
                Err(e) => {
                    log::error!("mDNS browse failed: {}. Fallback to isolated mode.", e);
                    return;
                }
            };

            log::info!("mDNS Scanner started, searching for {}", service_type);

            while let Ok(event) = receiver.recv() {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        let id = info.get_fullname().to_string();
                        let addresses: Vec<String> = info.get_addresses().iter().map(|ip| ip.to_string()).collect();
                        let addr = addresses.first().cloned().unwrap_or_else(|| "".to_string());
                        let port = info.get_port();
                        
                        log::debug!("Discovered TizenClaw peer: {} at {}:{}", id, addr, port);
                        
                        if let Ok(mut peers) = peers_clone.write() {
                            peers.insert(id.clone(), Peer {
                                id,
                                address: addr,
                                port,
                                last_seen: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs(),
                            });
                        }
                    }
                    ServiceEvent::ServiceRemoved(_service_type, fullname) => {
                        log::debug!("TizenClaw peer removed: {}", fullname);
                        if let Ok(mut peers) = peers_clone.write() {
                            peers.remove(&fullname);
                        }
                    }
                    _ => {}
                }
            }
        });
    }
}
