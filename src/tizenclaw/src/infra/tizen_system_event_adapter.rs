//! Tizen system event adapter — monitors battery, wifi, locale and other system events.
//!
//! Wraps capi-appfw-event to subscribe to Tizen system events and dispatch them
//! to the event bus.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// System event types.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SystemEventKind {
    BatteryLevelChanged,
    ChargerStatus,
    WifiStateChanged,
    BluetoothStateChanged,
    NetworkStateChanged,
    LanguageChanged,
    RegionChanged,
    LowMemory,
    Custom(String),
}

/// Payload for a system event.
#[derive(Debug, Clone)]
pub struct SystemEvent {
    pub kind: SystemEventKind,
    pub data: HashMap<String, String>,
}

/// Callback type for system events.
pub type SystemEventCallback = Box<dyn Fn(SystemEvent) + Send + 'static>;

pub struct TizenSystemEventAdapter {
    running: Arc<AtomicBool>,
    callbacks: Vec<(SystemEventKind, SystemEventCallback)>,
}

impl TizenSystemEventAdapter {
    pub fn new() -> Self {
        TizenSystemEventAdapter {
            running: Arc::new(AtomicBool::new(false)),
            callbacks: Vec::new(),
        }
    }

    /// Register a callback for a specific event kind.
    pub fn on<F>(&mut self, kind: SystemEventKind, callback: F)
    where
        F: Fn(SystemEvent) + Send + 'static,
    {
        self.callbacks.push((kind, Box::new(callback)));
    }

    /// Start listening for system events.
    pub fn start(&mut self) -> bool {
        if self.running.load(Ordering::SeqCst) {
            return true;
        }
        self.running.store(true, Ordering::SeqCst);
        // NOTE: Wire up tizen_sys::app_event::event_add_event_handler for
        // each registered event kind on device.
        log::info!("TizenSystemEventAdapter: started with {} handlers", self.callbacks.len());
        true
    }

    /// Stop listening.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        log::info!("TizenSystemEventAdapter: stopped");
    }

    /// Dispatch an event to registered callbacks.
    pub fn dispatch(&self, event: SystemEvent) {
        for (kind, cb) in &self.callbacks {
            if *kind == event.kind {
                cb(event.clone());
            }
        }
    }
}

impl Drop for TizenSystemEventAdapter {
    fn drop(&mut self) {
        self.stop();
    }
}
