//! LLM data type handles — C FFI for opaque message/tool/response objects.
//!
//! Each handle type is a heap-allocated Rust struct behind a `*mut c_void`.
//! Matches the C API in `tizenclaw_llm_backend.h`.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

const OK: i32 = 0;      // TIZENCLAW_ERROR_NONE
const EINVAL: i32 = -1;  // TIZENCLAW_ERROR_INVALID_PARAMETER

// ═══════════════════════════════════════════
//  Internal Rust types
// ═══════════════════════════════════════════

struct ToolCallInner {
    id: String,
    name: String,
    args_json: String,
}

struct MessageInner {
    role: String,
    text: String,
    tool_calls: Vec<*mut ToolCallInner>,
    tool_name: String,
    tool_call_id: String,
    tool_result_json: String,
}

struct ToolInner {
    name: String,
    description: String,
    parameters_json: String,
}

struct ResponseInner {
    success: bool,
    text: String,
    error_message: String,
    tool_calls: Vec<*mut ToolCallInner>,
    prompt_tokens: i32,
    completion_tokens: i32,
    total_tokens: i32,
    http_status: i32,
}

struct MessagesListInner {
    items: Vec<*mut MessageInner>,
}

struct ToolsListInner {
    items: Vec<*mut ToolInner>,
}

// ═══════════════════════════════════════════
//  Helpers
// ═══════════════════════════════════════════

unsafe fn set_str(dst: *mut *mut c_char, src: &str) -> i32 {
    if dst.is_null() { return EINVAL; }
    match CString::new(src) {
        Ok(cs) => { *dst = cs.into_raw(); OK }
        Err(_) => EINVAL,
    }
}

unsafe fn get_cstr<'a>(ptr: *const c_char) -> &'a str {
    if ptr.is_null() { return ""; }
    CStr::from_ptr(ptr).to_str().unwrap_or("")
}

// ═══════════════════════════════════════════
//  ToolCall
// ═══════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_tool_call_create(out: *mut *mut libc::c_void) -> i32 {
    if out.is_null() { return EINVAL; }
    let inner = Box::new(ToolCallInner { id: String::new(), name: String::new(), args_json: String::new() });
    *out = Box::into_raw(inner) as *mut libc::c_void;
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_tool_call_destroy(h: *mut libc::c_void) -> i32 {
    if h.is_null() { return EINVAL; }
    drop(Box::from_raw(h as *mut ToolCallInner));
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_tool_call_set_id(h: *mut libc::c_void, id: *const c_char) -> i32 {
    if h.is_null() || id.is_null() { return EINVAL; }
    let inner = &mut *(h as *mut ToolCallInner);
    inner.id = get_cstr(id).to_string();
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_tool_call_get_id(h: *mut libc::c_void, out: *mut *mut c_char) -> i32 {
    if h.is_null() { return EINVAL; }
    let inner = &*(h as *mut ToolCallInner);
    set_str(out, &inner.id)
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_tool_call_set_name(h: *mut libc::c_void, name: *const c_char) -> i32 {
    if h.is_null() || name.is_null() { return EINVAL; }
    let inner = &mut *(h as *mut ToolCallInner);
    inner.name = get_cstr(name).to_string();
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_tool_call_get_name(h: *mut libc::c_void, out: *mut *mut c_char) -> i32 {
    if h.is_null() { return EINVAL; }
    let inner = &*(h as *mut ToolCallInner);
    set_str(out, &inner.name)
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_tool_call_set_args_json(h: *mut libc::c_void, j: *const c_char) -> i32 {
    if h.is_null() || j.is_null() { return EINVAL; }
    let inner = &mut *(h as *mut ToolCallInner);
    inner.args_json = get_cstr(j).to_string();
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_tool_call_get_args_json(h: *mut libc::c_void, out: *mut *mut c_char) -> i32 {
    if h.is_null() { return EINVAL; }
    let inner = &*(h as *mut ToolCallInner);
    set_str(out, &inner.args_json)
}

// ═══════════════════════════════════════════
//  Message
// ═══════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_message_create(out: *mut *mut libc::c_void) -> i32 {
    if out.is_null() { return EINVAL; }
    let inner = Box::new(MessageInner {
        role: String::new(), text: String::new(),
        tool_calls: Vec::new(), tool_name: String::new(),
        tool_call_id: String::new(), tool_result_json: String::new(),
    });
    *out = Box::into_raw(inner) as *mut libc::c_void;
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_message_destroy(h: *mut libc::c_void) -> i32 {
    if h.is_null() { return EINVAL; }
    drop(Box::from_raw(h as *mut MessageInner));
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_message_set_role(h: *mut libc::c_void, v: *const c_char) -> i32 {
    if h.is_null() || v.is_null() { return EINVAL; }
    (&mut *(h as *mut MessageInner)).role = get_cstr(v).to_string(); OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_message_get_role(h: *mut libc::c_void, out: *mut *mut c_char) -> i32 {
    if h.is_null() { return EINVAL; }
    set_str(out, &(*(h as *mut MessageInner)).role)
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_message_set_text(h: *mut libc::c_void, v: *const c_char) -> i32 {
    if h.is_null() || v.is_null() { return EINVAL; }
    (&mut *(h as *mut MessageInner)).text = get_cstr(v).to_string(); OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_message_get_text(h: *mut libc::c_void, out: *mut *mut c_char) -> i32 {
    if h.is_null() { return EINVAL; }
    set_str(out, &(*(h as *mut MessageInner)).text)
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_message_add_tool_call(h: *mut libc::c_void, tc: *mut libc::c_void) -> i32 {
    if h.is_null() || tc.is_null() { return EINVAL; }
    (&mut *(h as *mut MessageInner)).tool_calls.push(tc as *mut ToolCallInner);
    OK
}

type ToolCallCb = unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void) -> bool;

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_message_foreach_tool_calls(
    h: *mut libc::c_void, cb: ToolCallCb, ud: *mut libc::c_void,
) -> i32 {
    if h.is_null() { return EINVAL; }
    let inner = &*(h as *mut MessageInner);
    for tc in &inner.tool_calls {
        if !cb(*tc as *mut libc::c_void, ud) { break; }
    }
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_message_set_tool_name(h: *mut libc::c_void, v: *const c_char) -> i32 {
    if h.is_null() || v.is_null() { return EINVAL; }
    (&mut *(h as *mut MessageInner)).tool_name = get_cstr(v).to_string(); OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_message_get_tool_name(h: *mut libc::c_void, out: *mut *mut c_char) -> i32 {
    if h.is_null() { return EINVAL; }
    set_str(out, &(*(h as *mut MessageInner)).tool_name)
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_message_set_tool_call_id(h: *mut libc::c_void, v: *const c_char) -> i32 {
    if h.is_null() || v.is_null() { return EINVAL; }
    (&mut *(h as *mut MessageInner)).tool_call_id = get_cstr(v).to_string(); OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_message_get_tool_call_id(h: *mut libc::c_void, out: *mut *mut c_char) -> i32 {
    if h.is_null() { return EINVAL; }
    set_str(out, &(*(h as *mut MessageInner)).tool_call_id)
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_message_set_tool_result_json(h: *mut libc::c_void, v: *const c_char) -> i32 {
    if h.is_null() || v.is_null() { return EINVAL; }
    (&mut *(h as *mut MessageInner)).tool_result_json = get_cstr(v).to_string(); OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_message_get_tool_result_json(h: *mut libc::c_void, out: *mut *mut c_char) -> i32 {
    if h.is_null() { return EINVAL; }
    set_str(out, &(*(h as *mut MessageInner)).tool_result_json)
}

// ═══════════════════════════════════════════
//  MessagesList
// ═══════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_messages_create(out: *mut *mut libc::c_void) -> i32 {
    if out.is_null() { return EINVAL; }
    let inner = Box::new(MessagesListInner { items: Vec::new() });
    *out = Box::into_raw(inner) as *mut libc::c_void;
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_messages_destroy(h: *mut libc::c_void) -> i32 {
    if h.is_null() { return EINVAL; }
    drop(Box::from_raw(h as *mut MessagesListInner));
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_messages_add(h: *mut libc::c_void, msg: *mut libc::c_void) -> i32 {
    if h.is_null() || msg.is_null() { return EINVAL; }
    (&mut *(h as *mut MessagesListInner)).items.push(msg as *mut MessageInner);
    OK
}

type MessageCb = unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void) -> bool;

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_messages_foreach(
    h: *mut libc::c_void, cb: MessageCb, ud: *mut libc::c_void,
) -> i32 {
    if h.is_null() { return EINVAL; }
    let inner = &*(h as *mut MessagesListInner);
    for m in &inner.items {
        if !cb(*m as *mut libc::c_void, ud) { break; }
    }
    OK
}

// ═══════════════════════════════════════════
//  Tool (declaration)
// ═══════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_tool_create(out: *mut *mut libc::c_void) -> i32 {
    if out.is_null() { return EINVAL; }
    let inner = Box::new(ToolInner { name: String::new(), description: String::new(), parameters_json: String::new() });
    *out = Box::into_raw(inner) as *mut libc::c_void;
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_tool_destroy(h: *mut libc::c_void) -> i32 {
    if h.is_null() { return EINVAL; }
    drop(Box::from_raw(h as *mut ToolInner));
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_tool_set_name(h: *mut libc::c_void, v: *const c_char) -> i32 {
    if h.is_null() || v.is_null() { return EINVAL; }
    (&mut *(h as *mut ToolInner)).name = get_cstr(v).to_string(); OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_tool_get_name(h: *mut libc::c_void, out: *mut *mut c_char) -> i32 {
    if h.is_null() { return EINVAL; }
    set_str(out, &(*(h as *mut ToolInner)).name)
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_tool_set_description(h: *mut libc::c_void, v: *const c_char) -> i32 {
    if h.is_null() || v.is_null() { return EINVAL; }
    (&mut *(h as *mut ToolInner)).description = get_cstr(v).to_string(); OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_tool_get_description(h: *mut libc::c_void, out: *mut *mut c_char) -> i32 {
    if h.is_null() { return EINVAL; }
    set_str(out, &(*(h as *mut ToolInner)).description)
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_tool_set_parameters_json(h: *mut libc::c_void, v: *const c_char) -> i32 {
    if h.is_null() || v.is_null() { return EINVAL; }
    (&mut *(h as *mut ToolInner)).parameters_json = get_cstr(v).to_string(); OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_tool_get_parameters_json(h: *mut libc::c_void, out: *mut *mut c_char) -> i32 {
    if h.is_null() { return EINVAL; }
    set_str(out, &(*(h as *mut ToolInner)).parameters_json)
}

// ═══════════════════════════════════════════
//  ToolsList
// ═══════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_tools_create(out: *mut *mut libc::c_void) -> i32 {
    if out.is_null() { return EINVAL; }
    let inner = Box::new(ToolsListInner { items: Vec::new() });
    *out = Box::into_raw(inner) as *mut libc::c_void;
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_tools_destroy(h: *mut libc::c_void) -> i32 {
    if h.is_null() { return EINVAL; }
    drop(Box::from_raw(h as *mut ToolsListInner));
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_tools_add(h: *mut libc::c_void, tool: *mut libc::c_void) -> i32 {
    if h.is_null() || tool.is_null() { return EINVAL; }
    (&mut *(h as *mut ToolsListInner)).items.push(tool as *mut ToolInner);
    OK
}

type ToolCb = unsafe extern "C" fn(*mut libc::c_void, *mut libc::c_void) -> bool;

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_tools_foreach(
    h: *mut libc::c_void, cb: ToolCb, ud: *mut libc::c_void,
) -> i32 {
    if h.is_null() { return EINVAL; }
    let inner = &*(h as *mut ToolsListInner);
    for t in &inner.items {
        if !cb(*t as *mut libc::c_void, ud) { break; }
    }
    OK
}

// ═══════════════════════════════════════════
//  Response
// ═══════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_response_create(out: *mut *mut libc::c_void) -> i32 {
    if out.is_null() { return EINVAL; }
    let inner = Box::new(ResponseInner {
        success: false, text: String::new(), error_message: String::new(),
        tool_calls: Vec::new(), prompt_tokens: 0, completion_tokens: 0,
        total_tokens: 0, http_status: 0,
    });
    *out = Box::into_raw(inner) as *mut libc::c_void;
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_response_destroy(h: *mut libc::c_void) -> i32 {
    if h.is_null() { return EINVAL; }
    drop(Box::from_raw(h as *mut ResponseInner));
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_response_set_success(h: *mut libc::c_void, v: bool) -> i32 {
    if h.is_null() { return EINVAL; }
    (&mut *(h as *mut ResponseInner)).success = v; OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_response_is_success(h: *mut libc::c_void, out: *mut bool) -> i32 {
    if h.is_null() || out.is_null() { return EINVAL; }
    *out = (*(h as *mut ResponseInner)).success; OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_response_set_text(h: *mut libc::c_void, v: *const c_char) -> i32 {
    if h.is_null() || v.is_null() { return EINVAL; }
    (&mut *(h as *mut ResponseInner)).text = get_cstr(v).to_string(); OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_response_get_text(h: *mut libc::c_void, out: *mut *mut c_char) -> i32 {
    if h.is_null() { return EINVAL; }
    set_str(out, &(*(h as *mut ResponseInner)).text)
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_response_set_error_message(h: *mut libc::c_void, v: *const c_char) -> i32 {
    if h.is_null() || v.is_null() { return EINVAL; }
    (&mut *(h as *mut ResponseInner)).error_message = get_cstr(v).to_string(); OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_response_get_error_message(h: *mut libc::c_void, out: *mut *mut c_char) -> i32 {
    if h.is_null() { return EINVAL; }
    set_str(out, &(*(h as *mut ResponseInner)).error_message)
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_response_add_llm_tool_call(h: *mut libc::c_void, tc: *mut libc::c_void) -> i32 {
    if h.is_null() || tc.is_null() { return EINVAL; }
    (&mut *(h as *mut ResponseInner)).tool_calls.push(tc as *mut ToolCallInner);
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_response_foreach_llm_tool_calls(
    h: *mut libc::c_void, cb: ToolCallCb, ud: *mut libc::c_void,
) -> i32 {
    if h.is_null() { return EINVAL; }
    for tc in &(*(h as *mut ResponseInner)).tool_calls {
        if !cb(*tc as *mut libc::c_void, ud) { break; }
    }
    OK
}

macro_rules! response_int_accessor {
    ($set_fn:ident, $get_fn:ident, $field:ident) => {
        #[no_mangle]
        pub unsafe extern "C" fn $set_fn(h: *mut libc::c_void, v: i32) -> i32 {
            if h.is_null() { return EINVAL; }
            (&mut *(h as *mut ResponseInner)).$field = v; OK
        }

        #[no_mangle]
        pub unsafe extern "C" fn $get_fn(h: *mut libc::c_void, out: *mut i32) -> i32 {
            if h.is_null() || out.is_null() { return EINVAL; }
            *out = (*(h as *mut ResponseInner)).$field; OK
        }
    };
}

response_int_accessor!(tizenclaw_llm_response_set_prompt_tokens, tizenclaw_llm_response_get_prompt_tokens, prompt_tokens);
response_int_accessor!(tizenclaw_llm_response_set_completion_tokens, tizenclaw_llm_response_get_completion_tokens, completion_tokens);
response_int_accessor!(tizenclaw_llm_response_set_total_tokens, tizenclaw_llm_response_get_total_tokens, total_tokens);
response_int_accessor!(tizenclaw_llm_response_set_http_status, tizenclaw_llm_response_get_http_status, http_status);

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_tool_call_create_destroy() {
        unsafe {
            let mut h: *mut libc::c_void = std::ptr::null_mut();
            assert_eq!(tizenclaw_llm_tool_call_create(&mut h), OK);
            assert!(!h.is_null());
            assert_eq!(tizenclaw_llm_tool_call_destroy(h), OK);
        }
    }

    #[test]
    fn test_tool_call_null_returns_einval() {
        unsafe {
            assert_eq!(tizenclaw_llm_tool_call_create(std::ptr::null_mut()), EINVAL);
            assert_eq!(tizenclaw_llm_tool_call_destroy(std::ptr::null_mut()), EINVAL);
        }
    }

    #[test]
    fn test_tool_call_set_get_name() {
        unsafe {
            let mut h: *mut libc::c_void = std::ptr::null_mut();
            tizenclaw_llm_tool_call_create(&mut h);

            let name = CString::new("get_weather").unwrap();
            assert_eq!(tizenclaw_llm_tool_call_set_name(h, name.as_ptr()), OK);

            let mut out: *mut c_char = std::ptr::null_mut();
            assert_eq!(tizenclaw_llm_tool_call_get_name(h, &mut out), OK);
            assert!(!out.is_null());
            let result = CStr::from_ptr(out).to_str().unwrap();
            assert_eq!(result, "get_weather");

            // Cleanup
            libc::free(out as *mut libc::c_void);
            tizenclaw_llm_tool_call_destroy(h);
        }
    }

    #[test]
    fn test_tool_call_set_get_args_json() {
        unsafe {
            let mut h: *mut libc::c_void = std::ptr::null_mut();
            tizenclaw_llm_tool_call_create(&mut h);

            let args = CString::new(r#"{"city":"Seoul"}"#).unwrap();
            tizenclaw_llm_tool_call_set_args_json(h, args.as_ptr());

            let mut out: *mut c_char = std::ptr::null_mut();
            tizenclaw_llm_tool_call_get_args_json(h, &mut out);
            let result = CStr::from_ptr(out).to_str().unwrap();
            assert_eq!(result, r#"{"city":"Seoul"}"#);

            libc::free(out as *mut libc::c_void);
            tizenclaw_llm_tool_call_destroy(h);
        }
    }

    #[test]
    fn test_message_create_set_role_text() {
        unsafe {
            let mut h: *mut libc::c_void = std::ptr::null_mut();
            tizenclaw_llm_message_create(&mut h);

            let role = CString::new("assistant").unwrap();
            let text = CString::new("Hello!").unwrap();
            tizenclaw_llm_message_set_role(h, role.as_ptr());
            tizenclaw_llm_message_set_text(h, text.as_ptr());

            let mut r_out: *mut c_char = std::ptr::null_mut();
            let mut t_out: *mut c_char = std::ptr::null_mut();
            tizenclaw_llm_message_get_role(h, &mut r_out);
            tizenclaw_llm_message_get_text(h, &mut t_out);

            assert_eq!(CStr::from_ptr(r_out).to_str().unwrap(), "assistant");
            assert_eq!(CStr::from_ptr(t_out).to_str().unwrap(), "Hello!");

            libc::free(r_out as *mut libc::c_void);
            libc::free(t_out as *mut libc::c_void);
            tizenclaw_llm_message_destroy(h);
        }
    }

    #[test]
    fn test_tool_create_set_get() {
        unsafe {
            let mut h: *mut libc::c_void = std::ptr::null_mut();
            tizenclaw_llm_tool_create(&mut h);

            let name = CString::new("calculator").unwrap();
            let desc = CString::new("Solves math problems").unwrap();
            let params = CString::new(r#"{"expression":"string"}"#).unwrap();

            tizenclaw_llm_tool_set_name(h, name.as_ptr());
            tizenclaw_llm_tool_set_description(h, desc.as_ptr());
            tizenclaw_llm_tool_set_parameters_json(h, params.as_ptr());

            let mut n_out: *mut c_char = std::ptr::null_mut();
            let mut d_out: *mut c_char = std::ptr::null_mut();
            let mut p_out: *mut c_char = std::ptr::null_mut();
            tizenclaw_llm_tool_get_name(h, &mut n_out);
            tizenclaw_llm_tool_get_description(h, &mut d_out);
            tizenclaw_llm_tool_get_parameters_json(h, &mut p_out);

            assert_eq!(CStr::from_ptr(n_out).to_str().unwrap(), "calculator");
            assert_eq!(CStr::from_ptr(d_out).to_str().unwrap(), "Solves math problems");
            assert_eq!(CStr::from_ptr(p_out).to_str().unwrap(), r#"{"expression":"string"}"#);

            libc::free(n_out as *mut libc::c_void);
            libc::free(d_out as *mut libc::c_void);
            libc::free(p_out as *mut libc::c_void);
            tizenclaw_llm_tool_destroy(h);
        }
    }

    #[test]
    fn test_response_success_tokens() {
        unsafe {
            let mut h: *mut libc::c_void = std::ptr::null_mut();
            tizenclaw_llm_response_create(&mut h);

            tizenclaw_llm_response_set_success(h, true);
            tizenclaw_llm_response_set_prompt_tokens(h, 100);
            tizenclaw_llm_response_set_completion_tokens(h, 50);
            tizenclaw_llm_response_set_total_tokens(h, 150);
            tizenclaw_llm_response_set_http_status(h, 200);

            let mut success = false;
            let mut prompt = 0i32;
            let mut completion = 0i32;
            let mut total = 0i32;
            let mut status = 0i32;

            tizenclaw_llm_response_is_success(h, &mut success);
            tizenclaw_llm_response_get_prompt_tokens(h, &mut prompt);
            tizenclaw_llm_response_get_completion_tokens(h, &mut completion);
            tizenclaw_llm_response_get_total_tokens(h, &mut total);
            tizenclaw_llm_response_get_http_status(h, &mut status);

            assert!(success);
            assert_eq!(prompt, 100);
            assert_eq!(completion, 50);
            assert_eq!(total, 150);
            assert_eq!(status, 200);

            tizenclaw_llm_response_destroy(h);
        }
    }

    #[test]
    fn test_messages_list_add_foreach() {
        unsafe {
            let mut list: *mut libc::c_void = std::ptr::null_mut();
            tizenclaw_llm_messages_create(&mut list);

            // Add 3 messages
            for _ in 0..3 {
                let mut msg: *mut libc::c_void = std::ptr::null_mut();
                tizenclaw_llm_message_create(&mut msg);
                tizenclaw_llm_messages_add(list, msg);
            }

            // Count via foreach
            static mut COUNT: i32 = 0;
            unsafe extern "C" fn counter(_msg: *mut libc::c_void, _ud: *mut libc::c_void) -> bool {
                COUNT += 1;
                true
            }
            COUNT = 0;
            tizenclaw_llm_messages_foreach(list, counter, std::ptr::null_mut());
            assert_eq!(COUNT, 3);

            tizenclaw_llm_messages_destroy(list);
        }
    }

    #[test]
    fn test_tools_list_add_foreach() {
        unsafe {
            let mut list: *mut libc::c_void = std::ptr::null_mut();
            tizenclaw_llm_tools_create(&mut list);

            for _ in 0..2 {
                let mut tool: *mut libc::c_void = std::ptr::null_mut();
                tizenclaw_llm_tool_create(&mut tool);
                tizenclaw_llm_tools_add(list, tool);
            }

            static mut TOOL_COUNT: i32 = 0;
            unsafe extern "C" fn tcounter(_t: *mut libc::c_void, _ud: *mut libc::c_void) -> bool {
                TOOL_COUNT += 1;
                true
            }
            TOOL_COUNT = 0;
            tizenclaw_llm_tools_foreach(list, tcounter, std::ptr::null_mut());
            assert_eq!(TOOL_COUNT, 2);

            tizenclaw_llm_tools_destroy(list);
        }
    }

    #[test]
    fn test_response_error_message() {
        unsafe {
            let mut h: *mut libc::c_void = std::ptr::null_mut();
            tizenclaw_llm_response_create(&mut h);

            let err = CString::new("API rate limit exceeded").unwrap();
            tizenclaw_llm_response_set_error_message(h, err.as_ptr());

            let mut out: *mut c_char = std::ptr::null_mut();
            tizenclaw_llm_response_get_error_message(h, &mut out);
            assert_eq!(CStr::from_ptr(out).to_str().unwrap(), "API rate limit exceeded");

            libc::free(out as *mut libc::c_void);
            tizenclaw_llm_response_destroy(h);
        }
    }

    #[test]
    fn test_response_default_success_is_false() {
        unsafe {
            let mut h: *mut libc::c_void = std::ptr::null_mut();
            tizenclaw_llm_response_create(&mut h);

            let mut success = true;
            tizenclaw_llm_response_is_success(h, &mut success);
            assert!(!success);

            tizenclaw_llm_response_destroy(h);
        }
    }
}

