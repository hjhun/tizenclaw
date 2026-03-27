//! Package event adapter — monitors package install/uninstall/update events.
//!
//! Uses pkgmgr_client API to listen for package manager events on the device.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Package event type.
#[derive(Debug, Clone)]
pub enum PackageEventType {
    Install,
    Uninstall,
    Update,
    Unknown(String),
}

/// A package manager event.
#[derive(Debug, Clone)]
pub struct PackageEvent {
    pub pkg_id: String,
    pub event_type: PackageEventType,
    pub key: String,
    pub val: String,
}

/// Callback for package events.
pub type PackageEventCallback = Box<dyn Fn(PackageEvent) + Send + 'static>;

pub struct PackageEventAdapter {
    running: Arc<AtomicBool>,
    callback: Option<PackageEventCallback>,
}

impl PackageEventAdapter {
    pub fn new() -> Self {
        PackageEventAdapter {
            running: Arc::new(AtomicBool::new(false)),
            callback: None,
        }
    }

    /// Start listening for package events.
    pub fn start<F>(&mut self, callback: F) -> bool
    where
        F: Fn(PackageEvent) + Send + 'static,
    {
        if self.running.load(Ordering::SeqCst) {
            return true;
        }

        self.callback = Some(Box::new(callback));
        self.running.store(true, Ordering::SeqCst);

        // NOTE: Wire up to tizen_sys::pkgmgr::pkgmgr_client_listen_status
        // when ready for on-device testing.
        log::info!("PackageEventAdapter: started listening for package events");
        true
    }

    /// Stop listening.
    pub fn stop(&mut self) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }
        self.running.store(false, Ordering::SeqCst);
        self.callback = None;
        log::info!("PackageEventAdapter: stopped");
    }

    /// Process a raw event (called from FFI callback).
    pub fn handle_event(&self, pkg_name: &str, event_type: &str, key: &str, val: &str) {
        let evt = PackageEvent {
            pkg_id: pkg_name.to_string(),
            event_type: match event_type {
                "install" => PackageEventType::Install,
                "uninstall" => PackageEventType::Uninstall,
                "update" => PackageEventType::Update,
                other => PackageEventType::Unknown(other.to_string()),
            },
            key: key.to_string(),
            val: val.to_string(),
        };
        if let Some(cb) = &self.callback {
            cb(evt);
        }
    }
}

impl Drop for PackageEventAdapter {
    fn drop(&mut self) {
        self.stop();
    }
}
