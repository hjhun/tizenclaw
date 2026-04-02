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
    client: *mut std::ffi::c_void,
}

// Ensure the struct can be sent across threads (since pointers are not Send/Sync by default)
unsafe impl Send for PackageEventAdapter {}
unsafe impl Sync for PackageEventAdapter {}

impl Default for PackageEventAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl PackageEventAdapter {
    pub fn new() -> Self {
        PackageEventAdapter {
            running: Arc::new(AtomicBool::new(false)),
            callback: None,
            client: std::ptr::null_mut(),
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

        unsafe {
            use libtizenclaw_core::tizen_sys::pkgmgr::*;
            // Create a pkgmgr client for listening
            self.client = pkgmgr_client_new(PC_LISTENING) as *mut std::ffi::c_void;
            if self.client.is_null() {
                log::error!("[TIZENCLAW] PackageEventAdapter: failed to create pkgmgr_client");
                return false;
            }

            // Set status type to all
            if pkgmgr_client_set_status_type(self.client as *mut _, PKGMGR_CLIENT_STATUS_ALL) != 0 {
                log::error!("[TIZENCLAW] PackageEventAdapter: failed to set status type");
                pkgmgr_client_free(self.client as *mut _);
                self.client = std::ptr::null_mut();
                return false;
            }

            // C callback
            unsafe extern "C" fn handler(
                _req_id: u32,
                _status: std::os::raw::c_int,
                _pkg_type: *const std::os::raw::c_char,
                pkgid: *const std::os::raw::c_char,
                key: *const std::os::raw::c_char,
                val: *const std::os::raw::c_char,
                _msg: *const std::os::raw::c_void,
                user_data: *mut std::os::raw::c_void,
            ) -> std::os::raw::c_int {
                let adapter = &*(user_data as *const PackageEventAdapter);
                
                let pkgid_str = if pkgid.is_null() { "" } else { std::ffi::CStr::from_ptr(pkgid).to_str().unwrap_or("") };
                let key_str = if key.is_null() { "" } else { std::ffi::CStr::from_ptr(key).to_str().unwrap_or("") };
                let val_str = if val.is_null() { "" } else { std::ffi::CStr::from_ptr(val).to_str().unwrap_or("") };

                log::info!("[TIZENCLAW] Package event received - pkgid: {}, key: {}, val: {}", pkgid_str, key_str, val_str);

                adapter.handle_event(pkgid_str, key_str, key_str, val_str);
                0
            }

            // Listen
            if pkgmgr_client_listen_status(
                self.client as *mut _,
                handler,
                self as *mut _ as *mut std::os::raw::c_void,
            ) != 0 {
                log::error!("[TIZENCLAW] PackageEventAdapter: failed to listen status");
                pkgmgr_client_free(self.client as *mut _);
                self.client = std::ptr::null_mut();
                return false;
            }
        }

        self.running.store(true, Ordering::SeqCst);
        log::info!("[TIZENCLAW] PackageEventAdapter: started listening for package events");
        true
    }

    /// Stop listening.
    pub fn stop(&mut self) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }

        unsafe {
            use libtizenclaw_core::tizen_sys::pkgmgr::*;
            if !self.client.is_null() {
                pkgmgr_client_free(self.client as *mut _);
                self.client = std::ptr::null_mut();
            }
        }

        self.running.store(false, Ordering::SeqCst);
        self.callback = None;
        log::info!("[TIZENCLAW] PackageEventAdapter: stopped");
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
