//! LLM data type handles — C FFI for opaque message/tool/response objects.
//!
//! Each handle type is a heap-allocated Rust struct behind a `*mut c_void`.
//! Matches the C API in `tizenclaw_llm_backend.h`.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};

const OK: i32 = 0;      // TIZENCLAW_ERROR_NONE
const EINVAL: i32 = -1;  // TIZENCLAW_ERROR_INVALID_PARAMETER

// ═══════════════════════════════════════════
//  Internal Rust types
// ═══════════════════════════════════════════

#[derive(Clone)]
struct ToolCallInner {
    id: String,
    name: String,
    args_json: String,
}

#[derive(Clone)]
struct MessageInner {
    role: String,
    text: String,
    tool_calls: Vec<Box<ToolCallInner>>,
    tool_name: String,
    tool_call_id: String,
    tool_result_json: String,
}

#[derive(Clone)]
struct ToolInner {
    name: String,
    description: String,
    parameters_json: String,
}

struct ResponseInner {
    success: bool,
    text: String,
    error_message: String,
    tool_calls: Vec<Box<ToolCallInner>>,
    prompt_tokens: i32,
    completion_tokens: i32,
    total_tokens: i32,
    http_status: i32,
}

struct MessagesListInner {
    items: Vec<Box<MessageInner>>,
}

struct ToolsListInner {
    items: Vec<Box<ToolInner>>,
}

// ═══════════════════════════════════════════
//  Helpers
// ═══════════════════════════════════════════

unsafe fn set_str(dst: *mut *mut c_char, src: &str) -> i32 {
    if dst.is_null() { return EINVAL; }
    if src.is_empty() {
        *dst = std::ptr::null_mut();
        return OK;
    }
    match CString::new(src) {
        Ok(cs) => { 
            let ptr = libc::strdup(cs.as_ptr());
            if ptr.is_null() {
                return EINVAL;
            }
            *dst = ptr;
            OK
        }
        Err(_) => EINVAL,
    }
}

unsafe fn get_cstr<'a>(ptr: *const c_char) -> &'a str {
    if ptr.is_null() { return ""; }
    CStr::from_ptr(ptr).to_str().unwrap_or("")
}

fn new_message_inner() -> MessageInner {
    MessageInner {
        role: String::new(),
        text: String::new(),
        tool_calls: Vec::new(),
        tool_name: String::new(),
        tool_call_id: String::new(),
        tool_result_json: String::new(),
    }
}

fn new_response_inner() -> ResponseInner {
    ResponseInner {
        success: false,
        text: String::new(),
        error_message: String::new(),
        tool_calls: Vec::new(),
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
        http_status: 0,
    }
}

fn len_to_c_int(len: usize) -> Result<c_int, i32> {
    i32::try_from(len).map_err(|_| EINVAL)
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
    let inner = Box::new(new_message_inner());
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
    (&mut *(h as *mut MessageInner)).tool_calls.push(Box::new((*(tc as *mut ToolCallInner)).clone()));
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
        if !cb(tc.as_ref() as *const _ as *mut libc::c_void, ud) { break; }
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
    (&mut *(h as *mut MessagesListInner)).items.push(Box::new((*(msg as *mut MessageInner)).clone()));
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
        if !cb(m.as_ref() as *const _ as *mut libc::c_void, ud) { break; }
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
    (&mut *(h as *mut ToolsListInner)).items.push(Box::new((*(tool as *mut ToolInner)).clone()));
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
        if !cb(t.as_ref() as *const _ as *mut libc::c_void, ud) { break; }
    }
    OK
}

// ═══════════════════════════════════════════
//  Response
// ═══════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_response_create(out: *mut *mut libc::c_void) -> i32 {
    if out.is_null() { return EINVAL; }
    let inner = Box::new(new_response_inner());
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
    (&mut *(h as *mut ResponseInner)).tool_calls.push(Box::new((*(tc as *mut ToolCallInner)).clone()));
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_llm_response_foreach_llm_tool_calls(
    h: *mut libc::c_void, cb: ToolCallCb, ud: *mut libc::c_void,
) -> i32 {
    if h.is_null() { return EINVAL; }
    for tc in &(*(h as *mut ResponseInner)).tool_calls {
        if !cb(tc.as_ref() as *const _ as *mut libc::c_void, ud) { break; }
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

// ═══════════════════════════════════════════
//  Compatibility C ABI
// ═══════════════════════════════════════════

#[no_mangle]
pub extern "C" fn tizenclaw_messages_list_new() -> *mut c_void {
    Box::into_raw(Box::new(MessagesListInner { items: Vec::new() })) as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_messages_list_free(ptr: *mut c_void) {
    if !ptr.is_null() {
        drop(Box::from_raw(ptr as *mut MessagesListInner));
    }
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_messages_list_add_message(
    ptr: *mut c_void,
    msg: *mut c_void,
) -> c_int {
    if ptr.is_null() || msg.is_null() {
        return EINVAL;
    }

    let list = &mut *(ptr as *mut MessagesListInner);
    let message = &*(msg as *mut MessageInner);
    list.items.push(Box::new(message.clone()));
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_messages_list_len(
    ptr: *mut c_void,
    out_len: *mut c_int,
) -> c_int {
    if ptr.is_null() || out_len.is_null() {
        return EINVAL;
    }

    let list = &*(ptr as *mut MessagesListInner);
    match len_to_c_int(list.items.len()) {
        Ok(len) => {
            *out_len = len;
            OK
        }
        Err(code) => code,
    }
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_messages_list_get(
    ptr: *mut c_void,
    index: c_int,
    out_msg: *mut *mut c_void,
) -> c_int {
    if ptr.is_null() || out_msg.is_null() || index < 0 {
        return EINVAL;
    }

    let list = &mut *(ptr as *mut MessagesListInner);
    let Some(message) = list.items.get_mut(index as usize) else {
        return EINVAL;
    };
    *out_msg = message.as_mut() as *mut MessageInner as *mut c_void;
    OK
}

#[no_mangle]
pub extern "C" fn tizenclaw_message_new() -> *mut c_void {
    Box::into_raw(Box::new(new_message_inner())) as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_message_free(ptr: *mut c_void) {
    if !ptr.is_null() {
        drop(Box::from_raw(ptr as *mut MessageInner));
    }
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_message_set_role(
    ptr: *mut c_void,
    role: *const c_char,
) -> c_int {
    if ptr.is_null() || role.is_null() {
        return EINVAL;
    }

    let message = &mut *(ptr as *mut MessageInner);
    message.role = get_cstr(role).to_string();
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_message_set_text(
    ptr: *mut c_void,
    text: *const c_char,
) -> c_int {
    if ptr.is_null() || text.is_null() {
        return EINVAL;
    }

    let message = &mut *(ptr as *mut MessageInner);
    message.text = get_cstr(text).to_string();
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_message_get_role(
    ptr: *const c_void,
    out: *mut *mut c_char,
) -> c_int {
    if ptr.is_null() {
        return EINVAL;
    }

    let message = &*(ptr as *const MessageInner);
    set_str(out, &message.role)
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_message_get_text(
    ptr: *const c_void,
    out: *mut *mut c_char,
) -> c_int {
    if ptr.is_null() {
        return EINVAL;
    }

    let message = &*(ptr as *const MessageInner);
    set_str(out, &message.text)
}

#[no_mangle]
pub extern "C" fn tizenclaw_response_new() -> *mut c_void {
    Box::into_raw(Box::new(new_response_inner())) as *mut c_void
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_response_free(ptr: *mut c_void) {
    if !ptr.is_null() {
        drop(Box::from_raw(ptr as *mut ResponseInner));
    }
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_response_is_success(
    ptr: *const c_void,
    out: *mut c_int,
) -> c_int {
    if ptr.is_null() || out.is_null() {
        return EINVAL;
    }

    let response = &*(ptr as *const ResponseInner);
    *out = if response.success { 1 } else { 0 };
    OK
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_response_get_text(
    ptr: *const c_void,
    out: *mut *mut c_char,
) -> c_int {
    if ptr.is_null() {
        return EINVAL;
    }

    let response = &*(ptr as *const ResponseInner);
    set_str(out, &response.text)
}

#[no_mangle]
pub unsafe extern "C" fn tizenclaw_response_get_tool_calls_count(
    ptr: *const c_void,
    out: *mut c_int,
) -> c_int {
    if ptr.is_null() || out.is_null() {
        return EINVAL;
    }

    let response = &*(ptr as *const ResponseInner);
    match len_to_c_int(response.tool_calls.len()) {
        Ok(len) => {
            *out = len;
            OK
        }
        Err(code) => code,
    }
}

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
            let mut count: i32 = 0;
            unsafe extern "C" fn counter(_msg: *mut libc::c_void, ud: *mut libc::c_void) -> bool {
                *(ud as *mut i32) += 1;
                true
            }
            tizenclaw_llm_messages_foreach(list, counter, &mut count as *mut _ as *mut libc::c_void);
            assert_eq!(count, 3);

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

            let mut tool_count: i32 = 0;
            unsafe extern "C" fn tcounter(_t: *mut libc::c_void, ud: *mut libc::c_void) -> bool {
                *(ud as *mut i32) += 1;
                true
            }
            tizenclaw_llm_tools_foreach(list, tcounter, &mut tool_count as *mut _ as *mut libc::c_void);
            assert_eq!(tool_count, 2);

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

    #[test]
    fn test_compat_message_roundtrip() {
        unsafe {
            let msg = tizenclaw_message_new();
            assert!(!msg.is_null());

            let role = CString::new("user").unwrap();
            let text = CString::new("hello").unwrap();
            assert_eq!(tizenclaw_message_set_role(msg, role.as_ptr()), OK);
            assert_eq!(tizenclaw_message_set_text(msg, text.as_ptr()), OK);

            let mut out_role: *mut c_char = std::ptr::null_mut();
            let mut out_text: *mut c_char = std::ptr::null_mut();
            assert_eq!(tizenclaw_message_get_role(msg, &mut out_role), OK);
            assert_eq!(tizenclaw_message_get_text(msg, &mut out_text), OK);
            assert_eq!(CStr::from_ptr(out_role).to_str().unwrap(), "user");
            assert_eq!(CStr::from_ptr(out_text).to_str().unwrap(), "hello");

            libc::free(out_role as *mut libc::c_void);
            libc::free(out_text as *mut libc::c_void);
            tizenclaw_message_free(msg);
        }
    }

    #[test]
    fn test_compat_messages_list_len_and_get() {
        unsafe {
            let list = tizenclaw_messages_list_new();
            let msg = tizenclaw_message_new();
            let text = CString::new("hello").unwrap();
            assert_eq!(tizenclaw_message_set_text(msg, text.as_ptr()), OK);

            assert_eq!(tizenclaw_messages_list_add_message(list, msg), OK);

            let mut len = 0;
            assert_eq!(tizenclaw_messages_list_len(list, &mut len), OK);
            assert_eq!(len, 1);

            let mut out_msg: *mut c_void = std::ptr::null_mut();
            assert_eq!(tizenclaw_messages_list_get(list, 0, &mut out_msg), OK);

            let mut out_text: *mut c_char = std::ptr::null_mut();
            assert_eq!(tizenclaw_message_get_text(out_msg, &mut out_text), OK);
            assert_eq!(CStr::from_ptr(out_text).to_str().unwrap(), "hello");

            libc::free(out_text as *mut libc::c_void);
            tizenclaw_message_free(msg);
            tizenclaw_messages_list_free(list);
        }
    }

    #[test]
    fn test_compat_response_defaults() {
        unsafe {
            let response = tizenclaw_response_new();
            assert!(!response.is_null());

            let mut success = -1;
            let mut tool_calls = -1;
            assert_eq!(tizenclaw_response_is_success(response, &mut success), OK);
            assert_eq!(tizenclaw_response_get_tool_calls_count(response, &mut tool_calls), OK);
            assert_eq!(success, 0);
            assert_eq!(tool_calls, 0);

            tizenclaw_response_free(response);
        }
    }

    #[test]
    fn test_compat_null_inputs_return_einval() {
        unsafe {
            let mut out_len = 0;
            let mut out_msg: *mut c_void = std::ptr::null_mut();
            let mut out_success = 0;
            let mut out_text: *mut c_char = std::ptr::null_mut();

            assert_eq!(tizenclaw_messages_list_add_message(std::ptr::null_mut(), std::ptr::null_mut()), EINVAL);
            assert_eq!(tizenclaw_messages_list_len(std::ptr::null_mut(), &mut out_len), EINVAL);
            assert_eq!(tizenclaw_messages_list_get(std::ptr::null_mut(), 0, &mut out_msg), EINVAL);
            assert_eq!(tizenclaw_message_set_role(std::ptr::null_mut(), std::ptr::null()), EINVAL);
            assert_eq!(tizenclaw_message_get_text(std::ptr::null(), &mut out_text), EINVAL);
            assert_eq!(tizenclaw_response_is_success(std::ptr::null(), &mut out_success), EINVAL);
            assert_eq!(tizenclaw_response_get_tool_calls_count(std::ptr::null(), &mut out_len), EINVAL);
        }
    }
}
