//! libtizenclaw — C FFI layer for TizenClaw Agent.
//!
//! Exports `extern "C"` functions matching the declarations in
//! `include/tizenclaw.h`. Each function is `#[no_mangle]` so the
//! linker produces stable C-compatible symbols.
//!
//! Thread safety: the opaque handle wraps `Arc<Mutex<TizenClaw>>`,
//! so concurrent access from multiple C threads is safe.

// Suppress unused warnings during C++ → Rust migration.
// TODO: Remove once all API functions are fully wired.
#![allow(unused)]
#![allow(clippy::missing_safety_doc)]

pub mod api;

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::{Arc, Mutex};

// ── Error codes (must match tizenclaw.h) ───────

const TIZENCLAW_ERROR_NONE: i32 = 0;
const TIZENCLAW_ERROR_INVALID_PARAMETER: i32 = -1;
const TIZENCLAW_ERROR_OUT_OF_MEMORY: i32 = -2;
const TIZENCLAW_ERROR_NOT_INITIALIZED: i32 = -3;
const _TIZENCLAW_ERROR_ALREADY_INITIALIZED: i32 = -4;
const _TIZENCLAW_ERROR_IO: i32 = -5;
const TIZENCLAW_ERROR_LLM_FAILED: i32 = -6;
const TIZENCLAW_ERROR_TOOL_FAILED: i32 = -7;
const _TIZENCLAW_ERROR_NOT_SUPPORTED: i32 = -8;

// ── Thread-local last error ────────────────────

thread_local! {
    static LAST_ERROR: std::cell::RefCell<Option<CString>> = const { std::cell::RefCell::new(None) };
}

fn set_last_error(msg: &str) {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = CString::new(msg).ok();
    });
}

// ── Helper: C string conversion ────────────────

unsafe fn cstr_to_str<'a>(ptr: *const c_char) -> Option<&'a str> {
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok()
}

fn string_to_c(s: String) -> *mut c_char {
    match CString::new(s) {
        Ok(cs) => cs.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

// ── Opaque handle ──────────────────────────────

/// Internal representation of the opaque `tizenclaw_h` handle.
struct HandleInner {
    agent: api::TizenClaw,
}

type HandlePtr = *mut Arc<Mutex<HandleInner>>;

// ═══════════════════════════════════════════
//  Lifecycle
// ═══════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_create(handle: *mut *mut libc::c_void) -> i32 {
    if handle.is_null() {
        set_last_error("handle pointer is null");
        return TIZENCLAW_ERROR_INVALID_PARAMETER;
    }

    let inner = HandleInner {
        agent: api::TizenClaw::new(),
    };

    let arc = Arc::new(Mutex::new(inner));
    let boxed = Box::new(arc);
    let raw = Box::into_raw(boxed);

    *handle = raw as *mut libc::c_void;
    TIZENCLAW_ERROR_NONE
}

#[no_mangle]
pub extern "C" fn tizenclaw_initialize(handle: *mut libc::c_void) -> i32 {
    let ptr = handle as HandlePtr;
    if ptr.is_null() {
        set_last_error("handle is null");
        return TIZENCLAW_ERROR_INVALID_PARAMETER;
    }

    let arc = unsafe { &*ptr };
    match arc.lock() {
        Ok(mut inner) => match inner.agent.initialize() {
            Ok(()) => TIZENCLAW_ERROR_NONE,
            Err(e) => {
                set_last_error(&e);
                TIZENCLAW_ERROR_NOT_INITIALIZED
            }
        },
        Err(e) => {
            set_last_error(&format!("Lock poisoned: {}", e));
            TIZENCLAW_ERROR_NOT_INITIALIZED
        }
    }
}

#[no_mangle]
pub extern "C" fn tizenclaw_destroy(handle: *mut libc::c_void) {
    if handle.is_null() {
        return;
    }

    let ptr = handle as HandlePtr;
    // Reconstruct the Box to reclaim ownership.
    // Dropping the Box → drops Arc → when ref count reaches 0,
    // drops HandleInner → drops TizenClaw (whose Drop impl calls shutdown()).
    let _ = unsafe { Box::from_raw(ptr) };
}

// ═══════════════════════════════════════════
//  Prompt Processing
// ═══════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_process_prompt(
    handle: *mut libc::c_void,
    session_id: *const c_char,
    prompt: *const c_char,
) -> *mut c_char {
    let ptr = handle as HandlePtr;
    if ptr.is_null() {
        set_last_error("handle is null");
        return std::ptr::null_mut();
    }

    let sid = match cstr_to_str(session_id) {
        Some(s) => s,
        None => {
            set_last_error("session_id is null or invalid UTF-8");
            return std::ptr::null_mut();
        }
    };
    let p = match cstr_to_str(prompt) {
        Some(s) => s,
        None => {
            set_last_error("prompt is null or invalid UTF-8");
            return std::ptr::null_mut();
        }
    };

    let arc = &*ptr;
    match arc.lock() {
        Ok(inner) => match inner.agent.process_prompt(p, sid) {
            Ok(resp) => string_to_c(resp),
            Err(e) => {
                set_last_error(&e);
                std::ptr::null_mut()
            }
        },
        Err(e) => {
            set_last_error(&format!("Lock poisoned: {}", e));
            std::ptr::null_mut()
        }
    }
}

/// Async callback type matching tizenclaw_response_cb in tizenclaw.h
type ResponseCallback = unsafe extern "C" fn(*const c_char, i32, *mut libc::c_void);

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_process_prompt_async(
    handle: *mut libc::c_void,
    session_id: *const c_char,
    prompt: *const c_char,
    callback: ResponseCallback,
    user_data: *mut libc::c_void,
) -> i32 {
    let ptr = handle as HandlePtr;
    if ptr.is_null() {
        set_last_error("handle is null");
        return TIZENCLAW_ERROR_INVALID_PARAMETER;
    }

    let sid = match cstr_to_str(session_id) {
        Some(s) => s.to_string(),
        None => {
            set_last_error("session_id is null or invalid UTF-8");
            return TIZENCLAW_ERROR_INVALID_PARAMETER;
        }
    };
    let p = match cstr_to_str(prompt) {
        Some(s) => s.to_string(),
        None => {
            set_last_error("prompt is null or invalid UTF-8");
            return TIZENCLAW_ERROR_INVALID_PARAMETER;
        }
    };

    let arc = (&*ptr).clone();
    // user_data is Send-unsafe but we need it in the thread
    let ud = user_data as usize;
    let cb = callback;

    std::thread::spawn(move || {
        let result = match arc.lock() {
            Ok(inner) => inner.agent.process_prompt(&p, &sid),
            Err(e) => Err(format!("Lock poisoned: {}", e)),
        };

        let ud_ptr = ud as *mut libc::c_void;
        match result {
            Ok(resp) => {
                if let Ok(cs) = CString::new(resp) {
                    cb(cs.as_ptr(), TIZENCLAW_ERROR_NONE, ud_ptr);
                } else {
                    cb(std::ptr::null(), TIZENCLAW_ERROR_LLM_FAILED, ud_ptr);
                }
            }
            Err(_) => {
                cb(std::ptr::null(), TIZENCLAW_ERROR_LLM_FAILED, ud_ptr);
            }
        }
    });

    TIZENCLAW_ERROR_NONE
}

// ═══════════════════════════════════════════
//  Session Management
// ═══════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_clear_session(
    handle: *mut libc::c_void,
    session_id: *const c_char,
) -> i32 {
    let ptr = handle as HandlePtr;
    if ptr.is_null() {
        set_last_error("handle is null");
        return TIZENCLAW_ERROR_INVALID_PARAMETER;
    }

    let sid = match cstr_to_str(session_id) {
        Some(s) => s,
        None => {
            set_last_error("session_id is null or invalid UTF-8");
            return TIZENCLAW_ERROR_INVALID_PARAMETER;
        }
    };

    let arc = &*ptr;
    match arc.lock() {
        Ok(inner) => match inner.agent.clear_session(sid) {
            Ok(()) => TIZENCLAW_ERROR_NONE,
            Err(e) => {
                set_last_error(&e);
                TIZENCLAW_ERROR_NOT_INITIALIZED
            }
        },
        Err(e) => {
            set_last_error(&format!("Lock poisoned: {}", e));
            TIZENCLAW_ERROR_NOT_INITIALIZED
        }
    }
}

// ═══════════════════════════════════════════
//  Monitoring
// ═══════════════════════════════════════════

#[no_mangle]
pub extern "C" fn tizenclaw_get_status(handle: *mut libc::c_void) -> *mut c_char {
    let ptr = handle as HandlePtr;
    if ptr.is_null() {
        set_last_error("handle is null");
        return std::ptr::null_mut();
    }

    let arc = unsafe { &*ptr };
    match arc.lock() {
        Ok(inner) => match inner.agent.get_status() {
            Ok(s) => string_to_c(s),
            Err(e) => {
                set_last_error(&e);
                std::ptr::null_mut()
            }
        },
        Err(e) => {
            set_last_error(&format!("Lock poisoned: {}", e));
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn tizenclaw_get_metrics(handle: *mut libc::c_void) -> *mut c_char {
    let ptr = handle as HandlePtr;
    if ptr.is_null() {
        set_last_error("handle is null");
        return std::ptr::null_mut();
    }

    let arc = unsafe { &*ptr };
    match arc.lock() {
        Ok(inner) => match inner.agent.get_metrics() {
            Ok(s) => string_to_c(s),
            Err(e) => {
                set_last_error(&e);
                std::ptr::null_mut()
            }
        },
        Err(e) => {
            set_last_error(&format!("Lock poisoned: {}", e));
            std::ptr::null_mut()
        }
    }
}

// ═══════════════════════════════════════════
//  Tools & Skills
// ═══════════════════════════════════════════

#[no_mangle]
pub extern "C" fn tizenclaw_get_tools(handle: *mut libc::c_void) -> *mut c_char {
    let ptr = handle as HandlePtr;
    if ptr.is_null() {
        set_last_error("handle is null");
        return std::ptr::null_mut();
    }

    let arc = unsafe { &*ptr };
    match arc.lock() {
        Ok(inner) => match inner.agent.get_tools() {
            Ok(s) => string_to_c(s),
            Err(e) => {
                set_last_error(&e);
                std::ptr::null_mut()
            }
        },
        Err(e) => {
            set_last_error(&format!("Lock poisoned: {}", e));
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_execute_tool(
    handle: *mut libc::c_void,
    tool_name: *const c_char,
    args_json: *const c_char,
) -> *mut c_char {
    let ptr = handle as HandlePtr;
    if ptr.is_null() {
        set_last_error("handle is null");
        return std::ptr::null_mut();
    }

    let name = match cstr_to_str(tool_name) {
        Some(s) => s,
        None => {
            set_last_error("tool_name is null or invalid UTF-8");
            return std::ptr::null_mut();
        }
    };
    let args = match cstr_to_str(args_json) {
        Some(s) => s,
        None => {
            set_last_error("args_json is null or invalid UTF-8");
            return std::ptr::null_mut();
        }
    };

    let arc = &*ptr;
    match arc.lock() {
        Ok(inner) => match inner.agent.execute_tool(name, args) {
            Ok(s) => string_to_c(s),
            Err(e) => {
                set_last_error(&e);
                std::ptr::null_mut()
            }
        },
        Err(e) => {
            set_last_error(&format!("Lock poisoned: {}", e));
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "C" fn tizenclaw_reload_skills(handle: *mut libc::c_void) -> i32 {
    let ptr = handle as HandlePtr;
    if ptr.is_null() {
        set_last_error("handle is null");
        return TIZENCLAW_ERROR_INVALID_PARAMETER;
    }

    let arc = unsafe { &*ptr };
    match arc.lock() {
        Ok(mut inner) => match inner.agent.reload_skills() {
            Ok(()) => TIZENCLAW_ERROR_NONE,
            Err(e) => {
                set_last_error(&e);
                TIZENCLAW_ERROR_TOOL_FAILED
            }
        },
        Err(e) => {
            set_last_error(&format!("Lock poisoned: {}", e));
            TIZENCLAW_ERROR_TOOL_FAILED
        }
    }
}

// ═══════════════════════════════════════════
//  Web Dashboard
// ═══════════════════════════════════════════

#[no_mangle]
pub extern "C" fn tizenclaw_start_dashboard(handle: *mut libc::c_void, port: u16) -> i32 {
    let ptr = handle as HandlePtr;
    if ptr.is_null() {
        set_last_error("handle is null");
        return TIZENCLAW_ERROR_INVALID_PARAMETER;
    }

    let arc = unsafe { &*ptr };
    match arc.lock() {
        Ok(inner) => match inner.agent.start_dashboard((port > 0).then_some(port)) {
            Ok(_) => TIZENCLAW_ERROR_NONE,
            Err(e) => {
                set_last_error(&e);
                TIZENCLAW_ERROR_TOOL_FAILED
            }
        },
        Err(e) => {
            set_last_error(&format!("Lock poisoned: {}", e));
            TIZENCLAW_ERROR_TOOL_FAILED
        }
    }
}

#[no_mangle]
pub extern "C" fn tizenclaw_stop_dashboard(handle: *mut libc::c_void) -> i32 {
    let ptr = handle as HandlePtr;
    if ptr.is_null() {
        set_last_error("handle is null");
        return TIZENCLAW_ERROR_INVALID_PARAMETER;
    }

    let arc = unsafe { &*ptr };
    match arc.lock() {
        Ok(inner) => match inner.agent.stop_dashboard() {
            Ok(_) => TIZENCLAW_ERROR_NONE,
            Err(e) => {
                set_last_error(&e);
                TIZENCLAW_ERROR_TOOL_FAILED
            }
        },
        Err(e) => {
            set_last_error(&format!("Lock poisoned: {}", e));
            TIZENCLAW_ERROR_TOOL_FAILED
        }
    }
}

// ═══════════════════════════════════════════
//  Utility
// ═══════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        // Reconstruct CString so it drops properly
        drop(CString::from_raw(ptr));
    }
}

#[no_mangle]
pub extern "C" fn tizenclaw_last_error() -> *const c_char {
    LAST_ERROR.with(|e| match &*e.borrow() {
        Some(cs) => cs.as_ptr(),
        None => std::ptr::null(),
    })
}
