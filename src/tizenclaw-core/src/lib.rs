//! TizenClaw Tizen Platform Plugin
//!
//! Builds as `libtizenclaw_plugin.so` — a dynamically loaded platform plugin
//! that provides Tizen-specific implementations for logging (dlog),
//! package management (pkgmgr), app control (app_control), and system info.
//!
//! Installed to: `/opt/usr/share/tizenclaw/plugins/libtizenclaw_plugin.so`
//!
//! ## C ABI Exports
//!
//! - `claw_plugin_info()` → JSON string with plugin metadata
//! - `claw_plugin_free_string()` → free strings returned by info

mod logging;
mod adapters;

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// Plugin metadata as JSON, conforming to the claw-platform ABI contract.
const PLUGIN_INFO_JSON: &str = r#"{
    "plugin_id": "tizen",
    "platform_name": "Tizen",
    "version": "1.0.0",
    "priority": 100,
    "capabilities": ["logging", "system_info", "package_manager", "app_control", "system_events"]
}"#;

/// C ABI: Return plugin metadata as a JSON C-string.
///
/// The caller must free the returned pointer with `claw_plugin_free_string()`.
#[no_mangle]
pub extern "C" fn claw_plugin_info() -> *const c_char {
    match CString::new(PLUGIN_INFO_JSON) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null(),
    }
}

/// C ABI: Free a string previously returned by `claw_plugin_info()`.
#[no_mangle]
pub unsafe extern "C" fn claw_plugin_free_string(s: *const c_char) {
    if !s.is_null() {
        // Reclaim the CString so it gets dropped properly
        let _ = CString::from_raw(s as *mut c_char);
    }
}
