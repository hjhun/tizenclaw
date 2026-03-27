//! HTTP helper — curl-like C API backed by Rust `ureq`.
//!
//! Provides `tizenclaw_curl_*` functions that external plugins
//! can use for HTTP requests without needing to link libcurl.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

const OK: i32 = 0;
const EINVAL: i32 = -1;
const EIO: i32 = -4;

struct CurlInner {
    url: String,
    headers: Vec<(String, String)>,
    post_data: Option<String>,
    method_get: bool,
    connect_timeout: u64,
    request_timeout: u64,
    response_body: String,
    response_code: i64,
    error_message: String,
    chunk_cb: Option<ChunkCallback>,
    chunk_ud: *mut libc::c_void,
}

type ChunkCallback = unsafe extern "C" fn(*const c_char, *mut libc::c_void);

unsafe fn cstr(ptr: *const c_char) -> &'static str {
    if ptr.is_null() { return ""; }
    CStr::from_ptr(ptr).to_str().unwrap_or("")
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_curl_create(out: *mut *mut libc::c_void) -> i32 {
    if out.is_null() { return EINVAL; }
    let inner = Box::new(CurlInner {
        url: String::new(),
        headers: Vec::new(),
        post_data: None,
        method_get: false,
        connect_timeout: 10,
        request_timeout: 120,
        response_body: String::new(),
        response_code: 0,
        error_message: String::new(),
        chunk_cb: None,
        chunk_ud: std::ptr::null_mut(),
    });
    *out = Box::into_raw(inner) as *mut libc::c_void;
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_curl_destroy(h: *mut libc::c_void) -> i32 {
    if h.is_null() { return EINVAL; }
    drop(Box::from_raw(h as *mut CurlInner));
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_curl_set_url(h: *mut libc::c_void, url: *const c_char) -> i32 {
    if h.is_null() || url.is_null() { return EINVAL; }
    (&mut *(h as *mut CurlInner)).url = cstr(url).to_string();
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_curl_add_header(h: *mut libc::c_void, header: *const c_char) -> i32 {
    if h.is_null() || header.is_null() { return EINVAL; }
    let s = cstr(header);
    if let Some(pos) = s.find(':') {
        let key = s[..pos].trim().to_string();
        let val = s[pos + 1..].trim().to_string();
        (&mut *(h as *mut CurlInner)).headers.push((key, val));
    }
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_curl_set_post_data(h: *mut libc::c_void, data: *const c_char) -> i32 {
    if h.is_null() || data.is_null() { return EINVAL; }
    (&mut *(h as *mut CurlInner)).post_data = Some(cstr(data).to_string());
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_curl_set_method_get(h: *mut libc::c_void) -> i32 {
    if h.is_null() { return EINVAL; }
    (&mut *(h as *mut CurlInner)).method_get = true;
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_curl_set_timeout(h: *mut libc::c_void, connect: libc::c_long, request: libc::c_long) -> i32 {
    if h.is_null() { return EINVAL; }
    let inner = &mut *(h as *mut CurlInner);
    inner.connect_timeout = connect.max(1) as u64;
    inner.request_timeout = request.max(1) as u64;
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_curl_set_write_callback(
    h: *mut libc::c_void, cb: ChunkCallback, ud: *mut libc::c_void,
) -> i32 {
    if h.is_null() { return EINVAL; }
    let inner = &mut *(h as *mut CurlInner);
    inner.chunk_cb = Some(cb);
    inner.chunk_ud = ud;
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_curl_perform(h: *mut libc::c_void) -> i32 {
    if h.is_null() { return EINVAL; }
    let inner = &mut *(h as *mut CurlInner);

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(inner.connect_timeout))
        .timeout(std::time::Duration::from_secs(inner.request_timeout))
        .build();

    let mut req = if inner.method_get || inner.post_data.is_none() {
        agent.get(&inner.url)
    } else {
        agent.post(&inner.url)
    };

    for (k, v) in &inner.headers {
        req = req.set(k, v);
    }

    let result = if let Some(ref data) = inner.post_data {
        if !inner.method_get {
            req.send_string(data)
        } else {
            req.call()
        }
    } else {
        req.call()
    };

    match result {
        Ok(resp) => {
            inner.response_code = resp.status() as i64;
            inner.response_body = resp.into_string().unwrap_or_default();

            if let Some(cb) = inner.chunk_cb {
                if let Ok(cs) = CString::new(inner.response_body.as_str()) {
                    cb(cs.as_ptr(), inner.chunk_ud);
                }
            }
            OK
        }
        Err(ureq::Error::Status(code, resp)) => {
            inner.response_code = code as i64;
            inner.response_body = resp.into_string().unwrap_or_default();
            inner.error_message = format!("HTTP {}", code);
            OK  // Not a transport error — caller checks response_code
        }
        Err(e) => {
            inner.error_message = format!("{}", e);
            inner.response_code = 0;
            EIO
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_curl_get_response_code(h: *mut libc::c_void, out: *mut libc::c_long) -> i32 {
    if h.is_null() || out.is_null() { return EINVAL; }
    *out = (*(h as *mut CurlInner)).response_code as libc::c_long;
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_curl_get_error_message(h: *mut libc::c_void) -> *const c_char {
    if h.is_null() {
        return b"Unknown or no error\0".as_ptr() as *const c_char;
    }
    let inner = &*(h as *mut CurlInner);
    if inner.error_message.is_empty() {
        b"Unknown or no error\0".as_ptr() as *const c_char
    } else {
        // Store as CString in the struct to keep it alive
        // For simplicity, return a static-ish pointer
        inner.error_message.as_ptr() as *const c_char
    }
}
