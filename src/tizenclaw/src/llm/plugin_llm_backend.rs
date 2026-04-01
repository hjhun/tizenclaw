//! Plugin LLM backend — dynamically loaded LLM backend via shared library.
//!
//! Enables third-party LLM backends to be loaded at runtime from `.so` files.
//! The plugin must export the C API defined in `tizenclaw_llm_backend.h`:
//!   - `TIZENCLAW_LLM_BACKEND_INITIALIZE(config_json: *const c_char) -> bool`
//!   - `TIZENCLAW_LLM_BACKEND_GET_NAME() -> *const c_char`
//!   - `TIZENCLAW_LLM_BACKEND_CHAT(messages, tools, on_chunk, user_data, system_prompt) -> response_h`
//!   - `TIZENCLAW_LLM_BACKEND_SHUTDOWN()`

use serde_json::Value;
use std::ffi::{CStr, CString};
use std::path::Path;

use super::backend::{LlmBackend, LlmMessage, LlmResponse, LlmToolCall, LlmToolDecl};

// ─────────────────────────────────────────
//  C ABI callback type matching tizenclaw_llm_backend.h
// ─────────────────────────────────────────
type ChunkCb = unsafe extern "C" fn(*const libc::c_char, *mut libc::c_void);

// ─────────────────────────────────────────
//  Plugin function signatures (match tizenclaw_llm_backend.h exactly)
// ─────────────────────────────────────────
type PluginInitFn = unsafe extern "C" fn(*const libc::c_char) -> bool;
type PluginGetNameFn = unsafe extern "C" fn() -> *const libc::c_char;
type PluginChatFn = unsafe extern "C" fn(
    /* messages */ *mut libc::c_void,
    /* tools    */ *mut libc::c_void,
    /* on_chunk */ Option<ChunkCb>,
    /* user_data*/ *mut libc::c_void,
    /* system_prompt */ *const libc::c_char,
) -> *mut libc::c_void;
type PluginShutdownFn = unsafe extern "C" fn();

/// A plugin-based LLM backend loaded from a shared library.
pub struct PluginLlmBackend {
    name: String,
    plugin_path: String,
    base_config: Option<Value>,
    lib_handle: Option<*mut libc::c_void>,
}

// SAFETY: dlopen handles are process-global and the resolved function pointers
// are safe to call from any thread. We serialize all calls through `&self`
// (shared references), so there are no data races on the handle itself.
unsafe impl Send for PluginLlmBackend {}
unsafe impl Sync for PluginLlmBackend {}

impl PluginLlmBackend {
    pub fn new(plugin_path: &str, base_config: Option<Value>) -> Self {
        PluginLlmBackend {
            name: Path::new(plugin_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("plugin")
                .to_string(),
            plugin_path: plugin_path.to_string(),
            base_config,
            lib_handle: None,
        }
    }

    /// Load the shared library and resolve the plugin name via
    /// `TIZENCLAW_LLM_BACKEND_GET_NAME`.
    fn load_library(&mut self) -> Result<(), String> {
        let c_path = std::ffi::CString::new(self.plugin_path.as_str())
            .map_err(|e| format!("Invalid path: {}", e))?;

        let handle = unsafe { libc::dlopen(c_path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            let err = unsafe {
                let msg = libc::dlerror();
                if msg.is_null() {
                    "Unknown dlopen error".to_string()
                } else {
                    CStr::from_ptr(msg).to_string_lossy().into_owned()
                }
            };
            return Err(format!("dlopen failed for '{}': {}", self.plugin_path, err));
        }

        // Resolve plugin name via TIZENCLAW_LLM_BACKEND_GET_NAME
        if let Some(name_fn) = Self::resolve_symbol_static::<PluginGetNameFn>(handle, "TIZENCLAW_LLM_BACKEND_GET_NAME") {
            let name_ptr = unsafe { name_fn() };
            if !name_ptr.is_null() {
                self.name = unsafe { CStr::from_ptr(name_ptr) }
                    .to_string_lossy()
                    .into_owned();
            }
        }

        self.lib_handle = Some(handle);
        log::info!("Loaded LLM plugin: {} ({})", self.name, self.plugin_path);
        Ok(())
    }

    fn resolve_symbol<T>(&self, handle: *mut libc::c_void, name: &str) -> Option<T> {
        Self::resolve_symbol_static(handle, name)
    }

    fn resolve_symbol_static<T>(handle: *mut libc::c_void, name: &str) -> Option<T> {
        let c_name = CString::new(name).ok()?;
        let sym = unsafe { libc::dlsym(handle, c_name.as_ptr()) };
        if sym.is_null() {
            None
        } else {
            Some(unsafe { std::mem::transmute_copy(&sym) })
        }
    }

    // ─────────────────────────────────────────
    //  Helpers: Convert Rust types → C API handles
    // ─────────────────────────────────────────

    /// Build a `tizenclaw_llm_messages_h` from Rust `LlmMessage` slice.
    unsafe fn build_messages_handle(messages: &[LlmMessage]) -> *mut libc::c_void {
        let mut messages_h: *mut libc::c_void = std::ptr::null_mut();
        libtizenclaw_core::llm_types::tizenclaw_llm_messages_create(&mut messages_h);

        for msg in messages {
            let mut msg_h: *mut libc::c_void = std::ptr::null_mut();
            libtizenclaw_core::llm_types::tizenclaw_llm_message_create(&mut msg_h);

            if let Ok(c_role) = CString::new(msg.role.as_str()) {
                libtizenclaw_core::llm_types::tizenclaw_llm_message_set_role(msg_h, c_role.as_ptr());
            }
            if let Ok(c_text) = CString::new(msg.text.as_str()) {
                libtizenclaw_core::llm_types::tizenclaw_llm_message_set_text(msg_h, c_text.as_ptr());
            }

            // Tool calls (assistant messages with function calls)
            for tc in &msg.tool_calls {
                let mut tc_h: *mut libc::c_void = std::ptr::null_mut();
                libtizenclaw_core::llm_types::tizenclaw_llm_tool_call_create(&mut tc_h);
                if let Ok(c_id) = CString::new(tc.id.as_str()) {
                    libtizenclaw_core::llm_types::tizenclaw_llm_tool_call_set_id(tc_h, c_id.as_ptr());
                }
                if let Ok(c_name) = CString::new(tc.name.as_str()) {
                    libtizenclaw_core::llm_types::tizenclaw_llm_tool_call_set_name(tc_h, c_name.as_ptr());
                }
                let args_str = tc.args.to_string();
                if let Ok(c_args) = CString::new(args_str.as_str()) {
                    libtizenclaw_core::llm_types::tizenclaw_llm_tool_call_set_args_json(tc_h, c_args.as_ptr());
                }
                libtizenclaw_core::llm_types::tizenclaw_llm_message_add_tool_call(msg_h, tc_h);
            }

            // Tool result message fields
            if !msg.tool_name.is_empty() {
                if let Ok(c_tn) = CString::new(msg.tool_name.as_str()) {
                    libtizenclaw_core::llm_types::tizenclaw_llm_message_set_tool_name(msg_h, c_tn.as_ptr());
                }
            }
            if !msg.tool_call_id.is_empty() {
                if let Ok(c_tid) = CString::new(msg.tool_call_id.as_str()) {
                    libtizenclaw_core::llm_types::tizenclaw_llm_message_set_tool_call_id(msg_h, c_tid.as_ptr());
                }
            }
            if !msg.tool_result.is_null() {
                let tr_str = msg.tool_result.to_string();
                if let Ok(c_tr) = CString::new(tr_str.as_str()) {
                    libtizenclaw_core::llm_types::tizenclaw_llm_message_set_tool_result_json(msg_h, c_tr.as_ptr());
                }
            }

            libtizenclaw_core::llm_types::tizenclaw_llm_messages_add(messages_h, msg_h);
        }

        messages_h
    }

    /// Build a `tizenclaw_llm_tools_h` from Rust `LlmToolDecl` slice.
    unsafe fn build_tools_handle(tools: &[LlmToolDecl]) -> *mut libc::c_void {
        let mut tools_h: *mut libc::c_void = std::ptr::null_mut();
        libtizenclaw_core::llm_types::tizenclaw_llm_tools_create(&mut tools_h);

        for tool in tools {
            let mut tool_h: *mut libc::c_void = std::ptr::null_mut();
            libtizenclaw_core::llm_types::tizenclaw_llm_tool_create(&mut tool_h);

            if let Ok(c_name) = CString::new(tool.name.as_str()) {
                libtizenclaw_core::llm_types::tizenclaw_llm_tool_set_name(tool_h, c_name.as_ptr());
            }
            if let Ok(c_desc) = CString::new(tool.description.as_str()) {
                libtizenclaw_core::llm_types::tizenclaw_llm_tool_set_description(tool_h, c_desc.as_ptr());
            }
            let params_str = tool.parameters.to_string();
            if let Ok(c_params) = CString::new(params_str.as_str()) {
                libtizenclaw_core::llm_types::tizenclaw_llm_tool_set_parameters_json(tool_h, c_params.as_ptr());
            }

            libtizenclaw_core::llm_types::tizenclaw_llm_tools_add(tools_h, tool_h);
        }

        tools_h
    }

    /// Extract an `LlmResponse` from a `tizenclaw_llm_response_h` handle.
    unsafe fn extract_response(response_h: *mut libc::c_void) -> LlmResponse {
        let mut resp = LlmResponse::default();

        if response_h.is_null() {
            resp.error_message = "Plugin returned null response".into();
            return resp;
        }

        // success
        let mut success = false;
        libtizenclaw_core::llm_types::tizenclaw_llm_response_is_success(response_h, &mut success);
        resp.success = success;

        // text
        let mut text_ptr: *mut libc::c_char = std::ptr::null_mut();
        libtizenclaw_core::llm_types::tizenclaw_llm_response_get_text(response_h, &mut text_ptr);
        if !text_ptr.is_null() {
            resp.text = CStr::from_ptr(text_ptr).to_string_lossy().into_owned();
            libc::free(text_ptr as *mut libc::c_void);
        }

        // error_message
        let mut err_ptr: *mut libc::c_char = std::ptr::null_mut();
        libtizenclaw_core::llm_types::tizenclaw_llm_response_get_error_message(response_h, &mut err_ptr);
        if !err_ptr.is_null() {
            resp.error_message = CStr::from_ptr(err_ptr).to_string_lossy().into_owned();
            libc::free(err_ptr as *mut libc::c_void);
        }

        // tokens
        let mut prompt_tokens: i32 = 0;
        let mut completion_tokens: i32 = 0;
        let mut total_tokens: i32 = 0;
        let mut http_status: i32 = 0;
        libtizenclaw_core::llm_types::tizenclaw_llm_response_get_prompt_tokens(response_h, &mut prompt_tokens);
        libtizenclaw_core::llm_types::tizenclaw_llm_response_get_completion_tokens(response_h, &mut completion_tokens);
        libtizenclaw_core::llm_types::tizenclaw_llm_response_get_total_tokens(response_h, &mut total_tokens);
        libtizenclaw_core::llm_types::tizenclaw_llm_response_get_http_status(response_h, &mut http_status);
        resp.prompt_tokens = prompt_tokens;
        resp.completion_tokens = completion_tokens;
        resp.total_tokens = total_tokens;
        resp.http_status = http_status as u16;

        // tool calls
        unsafe extern "C" fn collect_tool_call(tc_h: *mut libc::c_void, ud: *mut libc::c_void) -> bool {
            let calls = &mut *(ud as *mut Vec<LlmToolCall>);
            let mut id_ptr: *mut libc::c_char = std::ptr::null_mut();
            let mut name_ptr: *mut libc::c_char = std::ptr::null_mut();
            let mut args_ptr: *mut libc::c_char = std::ptr::null_mut();

            libtizenclaw_core::llm_types::tizenclaw_llm_tool_call_get_id(tc_h, &mut id_ptr);
            libtizenclaw_core::llm_types::tizenclaw_llm_tool_call_get_name(tc_h, &mut name_ptr);
            libtizenclaw_core::llm_types::tizenclaw_llm_tool_call_get_args_json(tc_h, &mut args_ptr);

            let id = if id_ptr.is_null() { String::new() }
                     else { let s = CStr::from_ptr(id_ptr).to_string_lossy().into_owned(); libc::free(id_ptr as *mut _); s };
            let name = if name_ptr.is_null() { String::new() }
                       else { let s = CStr::from_ptr(name_ptr).to_string_lossy().into_owned(); libc::free(name_ptr as *mut _); s };
            let args = if args_ptr.is_null() { serde_json::Value::Object(serde_json::Map::new()) }
                       else {
                           let s = CStr::from_ptr(args_ptr).to_string_lossy().into_owned();
                           libc::free(args_ptr as *mut _);
                           serde_json::from_str(&s).unwrap_or(serde_json::Value::Object(serde_json::Map::new()))
                       };

            calls.push(LlmToolCall { id, name, args });
            true
        }

        let mut tool_calls: Vec<LlmToolCall> = Vec::new();
        libtizenclaw_core::llm_types::tizenclaw_llm_response_foreach_llm_tool_calls(
            response_h,
            collect_tool_call,
            &mut tool_calls as *mut _ as *mut libc::c_void,
        );
        resp.tool_calls = tool_calls;

        // Destroy the plugin-allocated response handle
        libtizenclaw_core::llm_types::tizenclaw_llm_response_destroy(response_h);

        resp
    }
}

#[async_trait::async_trait]
impl LlmBackend for PluginLlmBackend {
    fn initialize(&mut self, config: &Value) -> bool {
        if let Err(e) = self.load_library() {
            log::error!("Plugin init failed: {}", e);
            return false;
        }

        let handle = match self.lib_handle {
            Some(h) => h,
            None => return false,
        };

        if let Some(init_fn) = self.resolve_symbol::<PluginInitFn>(handle, "TIZENCLAW_LLM_BACKEND_INITIALIZE") {
            let final_config = match self.base_config.as_ref() {
                Some(bc) => {
                    let mut merged = bc.clone();
                    if let Value::Object(ref mut map) = merged {
                        if let Value::Object(ref c) = config {
                            for (k, v) in c {
                                map.insert(k.clone(), v.clone());
                            }
                        }
                    }
                    merged
                },
                None => config.clone()
            };

            let config_str = final_config.to_string();
            let c_config = match CString::new(config_str) {
                Ok(c) => c,
                Err(_) => return false,
            };
            let result = unsafe { init_fn(c_config.as_ptr()) };
            if !result {
                log::error!("Plugin '{}' TIZENCLAW_LLM_BACKEND_INITIALIZE returned false", self.name);
            }
            result
        } else {
            log::warn!("Plugin '{}' has no TIZENCLAW_LLM_BACKEND_INITIALIZE symbol, skipping init", self.name);
            true
        }
    }

    async fn chat(
        &self,
        messages: &[LlmMessage],
        tools: &[LlmToolDecl],
        _on_chunk: Option<&(dyn Fn(&str) + Send + Sync)>,
        system_prompt: &str,
    ) -> LlmResponse {
        let handle = match self.lib_handle {
            Some(h) => h,
            None => {
                return LlmResponse {
                    success: false,
                    error_message: "Plugin not loaded".into(),
                    ..Default::default()
                }
            }
        };

        let chat_fn = match self.resolve_symbol::<PluginChatFn>(handle, "TIZENCLAW_LLM_BACKEND_CHAT") {
            Some(f) => f,
            None => {
                return LlmResponse {
                    success: false,
                    error_message: "Plugin missing TIZENCLAW_LLM_BACKEND_CHAT symbol".into(),
                    ..Default::default()
                }
            }
        };

        // Build C API handles from Rust types
        let messages_h = unsafe { Self::build_messages_handle(messages) };
        let tools_h = unsafe { Self::build_tools_handle(tools) };

        let c_system_prompt = CString::new(system_prompt).unwrap_or_else(|_| CString::new("").unwrap());

        // TODO: Wire up on_chunk callback via ChunkCb if streaming is needed.
        // For now, pass None to indicate no streaming callback.
        let response_h = unsafe {
            chat_fn(
                messages_h,
                tools_h,
                None,
                std::ptr::null_mut(),
                c_system_prompt.as_ptr(),
            )
        };

        // Extract response from the C handle into Rust LlmResponse
        let resp = unsafe { Self::extract_response(response_h) };

        // Cleanup: destroy the messages and tools handles
        unsafe {
            libtizenclaw_core::llm_types::tizenclaw_llm_messages_destroy(messages_h);
            libtizenclaw_core::llm_types::tizenclaw_llm_tools_destroy(tools_h);
        }

        resp
    }

    fn get_name(&self) -> &str {
        &self.name
    }

    fn shutdown(&mut self) {
        if let Some(handle) = self.lib_handle.take() {
            // Call TIZENCLAW_LLM_BACKEND_SHUTDOWN before closing the library
            if let Some(shutdown_fn) = Self::resolve_symbol_static::<PluginShutdownFn>(handle, "TIZENCLAW_LLM_BACKEND_SHUTDOWN") {
                unsafe { shutdown_fn() };
                log::info!("Called TIZENCLAW_LLM_BACKEND_SHUTDOWN for plugin '{}'", self.name);
            }
            unsafe { libc::dlclose(handle) };
            log::info!("Unloaded LLM plugin: {}", self.name);
        }
    }
}

impl Drop for PluginLlmBackend {
    fn drop(&mut self) {
        self.shutdown();
    }
}
