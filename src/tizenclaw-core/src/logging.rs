//! Tizen dlog-based logging implementation.
//!
//! Wraps `tizen_sys::dlog::dlog_print()` to provide PlatformLogger for Tizen.

use tizenclaw::{LogLevel, PlatformLogger};
use std::ffi::CString;

const TAG: &str = "TIZENCLAW";

/// Tizen dlog-based logger (Rust API).
pub struct DlogLogger;

impl PlatformLogger for DlogLogger {
    fn log(&self, level: LogLevel, tag: &str, msg: &str) {
        let prio = match level {
            LogLevel::Error => tizen_sys::dlog::DLOG_ERROR,
            LogLevel::Warn  => tizen_sys::dlog::DLOG_WARN,
            LogLevel::Info  => tizen_sys::dlog::DLOG_INFO,
            LogLevel::Debug => tizen_sys::dlog::DLOG_DEBUG,
        };

        let use_tag = if tag.is_empty() { TAG } else { tag };
        let escaped = msg.replace('%', "%%");
        if let (Ok(tag_c), Ok(msg_c)) = (CString::new(use_tag), CString::new(escaped)) {
            unsafe {
                tizen_sys::dlog::dlog_print(prio, tag_c.as_ptr(), msg_c.as_ptr());
            }
        }
    }
}

/// C ABI for platform plugin logger (called by the main daemon via dlopen)
#[no_mangle]
pub extern "C" fn claw_plugin_log(level: i32, tag: *const std::os::raw::c_char, msg: *const std::os::raw::c_char) {
    let prio = match level {
        0 => tizen_sys::dlog::DLOG_ERROR,
        1 => tizen_sys::dlog::DLOG_WARN,
        2 => tizen_sys::dlog::DLOG_INFO,
        _ => tizen_sys::dlog::DLOG_DEBUG,
    };
    unsafe {
        tizen_sys::dlog::dlog_print(prio, tag, msg);
    }
}
