//! Tizen dlog-based logging implementation.
//!
//! Wraps `tizen_sys::dlog::dlog_print()` to provide PlatformLogger for Tizen.

use libtizenclaw::{LogLevel, PlatformLogger};
use std::ffi::CString;

const TAG: &str = "TIZENCLAW";

/// Tizen dlog-based logger.
pub struct DlogLogger;

impl PlatformLogger for DlogLogger {
    fn log(&self, level: LogLevel, tag: &str, msg: &str) {
        let prio = match level {
            LogLevel::Error => tizen_sys::dlog::DLOG_ERROR,
            LogLevel::Warn  => tizen_sys::dlog::DLOG_WARN,
            LogLevel::Info  => tizen_sys::dlog::DLOG_INFO,
            LogLevel::Debug => tizen_sys::dlog::DLOG_DEBUG,
        };

        // Use provided tag or default to TIZENCLAW
        let use_tag = if tag.is_empty() { TAG } else { tag };

        // Escape '%' to prevent format string attacks in dlog
        let escaped = msg.replace('%', "%%");
        if let (Ok(tag_c), Ok(msg_c)) = (CString::new(use_tag), CString::new(escaped)) {
            unsafe {
                tizen_sys::dlog::dlog_print(prio, tag_c.as_ptr(), msg_c.as_ptr());
            }
        }
    }
}
