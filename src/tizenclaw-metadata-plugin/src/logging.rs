/*
 * Copyright (c) 2026 Samsung Electronics Co., Ltd.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! Tizen dlog-based logging for metadata plugins.

#[cfg(tizen_native)]
use std::ffi::{c_int, CString};

/// dlog priority levels from `<dlog.h>`.
#[cfg(tizen_native)]
const DLOG_INFO: c_int = 4;
#[cfg(tizen_native)]
const DLOG_ERROR: c_int = 6;

/// Log tag for all TizenClaw metadata plugins.
#[cfg(tizen_native)]
const TAG: &[u8] = b"TIZENCLAW_METADATA_PLUGIN\0";

/// Printf format string for a single string argument.
#[cfg(tizen_native)]
const FMT_STR: &[u8] = b"%s\0";

#[cfg(tizen_native)]
extern "C" {
    fn dlog_print(prio: c_int, tag: *const u8, fmt: *const u8, ...) -> c_int;
}

#[macro_export]
macro_rules! plugin_log_info {
    ($($arg:tt)*) => {
        let filepath = file!();
        let filename = filepath.rsplit('/').next().unwrap_or(filepath).rsplit('\\').next().unwrap_or(filepath);
        let msg = format!("{}:{} {}", filename, line!(), format_args!($($arg)*));
        $crate::logging::log_info_internal(&msg);
    }
}

#[macro_export]
macro_rules! plugin_log_error {
    ($($arg:tt)*) => {
        let filepath = file!();
        let filename = filepath.rsplit('/').next().unwrap_or(filepath).rsplit('\\').next().unwrap_or(filepath);
        let msg = format!("{}:{} {}", filename, line!(), format_args!($($arg)*));
        $crate::logging::log_error_internal(&msg);
    }
}

/// Log an informational message to Tizen dlog (internal dispatch).
pub fn log_info_internal(msg: &str) {
    #[cfg(not(tizen_native))]
    {
        eprintln!("{}", msg);
        return;
    }

    #[cfg(tizen_native)]
    if let Ok(c_msg) = CString::new(msg) {
        unsafe {
            dlog_print(DLOG_INFO, TAG.as_ptr(), FMT_STR.as_ptr(), c_msg.as_ptr());
        }
    }
}

/// Log an error message to Tizen dlog and stderr (internal dispatch).
pub fn log_error_internal(msg: &str) {
    eprintln!("{}", msg);
    #[cfg(not(tizen_native))]
    {
        return;
    }

    #[cfg(tizen_native)]
    if let Ok(c_msg) = CString::new(msg) {
        unsafe {
            dlog_print(DLOG_ERROR, TAG.as_ptr(), FMT_STR.as_ptr(), c_msg.as_ptr());
        }
    }
}
