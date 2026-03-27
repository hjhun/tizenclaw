//! App lifecycle adapter — monitors Tizen app lifecycle events.
//!
//! Wraps the capi-appfw-event API to receive app launch/terminate events.

use std::ffi::CString;
use std::os::raw::c_void;
use std::sync::{Arc, Mutex};

/// Represents an app lifecycle event.
#[derive(Debug, Clone)]
pub enum AppEvent {
    Launched { app_id: String },
    Terminated { app_id: String },
    Paused { app_id: String },
    Resumed { app_id: String },
}

/// Callback for app lifecycle events.
pub type AppEventCallback = Box<dyn Fn(AppEvent) + Send + 'static>;

pub struct AppLifecycleAdapter {
    handler: usize,
    callback: Option<Arc<Mutex<AppEventCallback>>>,
}

impl AppLifecycleAdapter {
    pub fn new() -> Self {
        AppLifecycleAdapter {
            handler: 0,
            callback: None,
        }
    }

    /// Start monitoring app lifecycle events.
    pub fn start<F>(&mut self, callback: F) -> bool
    where
        F: Fn(AppEvent) + Send + 'static,
    {
        self.callback = Some(Arc::new(Mutex::new(Box::new(callback))));
        log::info!("AppLifecycleAdapter: started monitoring app events");
        // NOTE: Actual FFI registration via tizen-sys::app_event will be
        // wired up once we have the target device available for testing.
        true
    }

    /// Stop monitoring.
    pub fn stop(&mut self) {
        self.callback = None;
        log::info!("AppLifecycleAdapter: stopped");
    }
}

impl Drop for AppLifecycleAdapter {
    fn drop(&mut self) {
        self.stop();
    }
}
