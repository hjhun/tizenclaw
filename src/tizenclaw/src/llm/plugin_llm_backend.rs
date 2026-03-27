//! Plugin LLM backend — dynamically loaded LLM backend via shared library.
//!
//! Enables third-party LLM backends to be loaded at runtime from `.so` files.

use serde_json::Value;
use std::path::Path;

use super::backend::{LlmBackend, LlmMessage, LlmResponse, LlmToolCall, LlmToolDecl};

/// A plugin-based LLM backend loaded from a shared library.
///
/// The plugin exposes a C-compatible interface:
///   - `llm_plugin_init(config_json: *const c_char) -> c_int`
///   - `llm_plugin_chat(request_json: *const c_char) -> *mut c_char`
///   - `llm_plugin_name() -> *const c_char`
///   - `llm_plugin_free(ptr: *mut c_char)`
pub struct PluginLlmBackend {
    name: String,
    plugin_path: String,
    lib_handle: Option<*mut libc::c_void>,
}

// SAFETY: dlopen handles are process-global and the resolved function pointers
// are safe to call from any thread. We serialize all calls through `&self`
// (shared references), so there are no data races on the handle itself.
unsafe impl Send for PluginLlmBackend {}
unsafe impl Sync for PluginLlmBackend {}

// Plugin function signatures (C ABI)
type PluginInitFn = unsafe extern "C" fn(*const libc::c_char) -> libc::c_int;
type PluginChatFn = unsafe extern "C" fn(*const libc::c_char) -> *mut libc::c_char;
type PluginNameFn = unsafe extern "C" fn() -> *const libc::c_char;
type PluginFreeFn = unsafe extern "C" fn(*mut libc::c_char);

impl PluginLlmBackend {
    pub fn new(plugin_path: &str) -> Self {
        PluginLlmBackend {
            name: Path::new(plugin_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("plugin")
                .to_string(),
            plugin_path: plugin_path.to_string(),
            lib_handle: None,
        }
    }

    /// Load the shared library and resolve the plugin name.
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
                    std::ffi::CStr::from_ptr(msg).to_string_lossy().into_owned()
                }
            };
            return Err(format!("dlopen failed for '{}': {}", self.plugin_path, err));
        }

        // Try to get the plugin name
        if let Some(name_fn) = self.resolve_symbol::<PluginNameFn>(handle, "llm_plugin_name") {
            let name_ptr = unsafe { name_fn() };
            if !name_ptr.is_null() {
                self.name = unsafe { std::ffi::CStr::from_ptr(name_ptr) }
                    .to_string_lossy()
                    .into_owned();
            }
        }

        self.lib_handle = Some(handle);
        log::info!("Loaded LLM plugin: {} ({})", self.name, self.plugin_path);
        Ok(())
    }

    fn resolve_symbol<T>(&self, handle: *mut libc::c_void, name: &str) -> Option<T> {
        let c_name = std::ffi::CString::new(name).ok()?;
        let sym = unsafe { libc::dlsym(handle, c_name.as_ptr()) };
        if sym.is_null() {
            None
        } else {
            Some(unsafe { std::mem::transmute_copy(&sym) })
        }
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

        if let Some(init_fn) = self.resolve_symbol::<PluginInitFn>(handle, "llm_plugin_init") {
            let config_str = config.to_string();
            let c_config = match std::ffi::CString::new(config_str) {
                Ok(c) => c,
                Err(_) => return false,
            };
            let result = unsafe { init_fn(c_config.as_ptr()) };
            result == 0
        } else {
            log::warn!("Plugin '{}' has no llm_plugin_init symbol, skipping init", self.name);
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

        let chat_fn = match self.resolve_symbol::<PluginChatFn>(handle, "llm_plugin_chat") {
            Some(f) => f,
            None => {
                return LlmResponse {
                    success: false,
                    error_message: "Plugin missing llm_plugin_chat".into(),
                    ..Default::default()
                }
            }
        };

        // Serialize request as JSON
        let request = serde_json::json!({
            "messages": messages,
            "tools": tools,
            "system_prompt": system_prompt,
        });
        let c_request = match std::ffi::CString::new(request.to_string()) {
            Ok(c) => c,
            Err(_) => {
                return LlmResponse {
                    success: false,
                    error_message: "Failed to serialize request".into(),
                    ..Default::default()
                }
            }
        };

        let response_ptr = unsafe { chat_fn(c_request.as_ptr()) };
        if response_ptr.is_null() {
            return LlmResponse {
                success: false,
                error_message: "Plugin returned null".into(),
                ..Default::default()
            };
        }

        let response_str = unsafe { std::ffi::CStr::from_ptr(response_ptr) }
            .to_string_lossy()
            .into_owned();

        // Free the plugin-allocated string
        if let Some(free_fn) = self.resolve_symbol::<PluginFreeFn>(handle, "llm_plugin_free") {
            unsafe { free_fn(response_ptr) };
        }

        // Parse the JSON response
        match serde_json::from_str::<Value>(&response_str) {
            Ok(json) => LlmResponse {
                success: json["success"].as_bool().unwrap_or(false),
                text: json["text"].as_str().unwrap_or("").into(),
                error_message: json["error"].as_str().unwrap_or("").into(),
                tool_calls: json["tool_calls"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|tc| {
                                Some(LlmToolCall {
                                    id: tc["id"].as_str()?.into(),
                                    name: tc["name"].as_str()?.into(),
                                    args: tc.get("args").cloned().unwrap_or(Value::Object(
                                        serde_json::Map::new(),
                                    )),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                prompt_tokens: json["prompt_tokens"].as_i64().unwrap_or(0) as i32,
                completion_tokens: json["completion_tokens"].as_i64().unwrap_or(0) as i32,
                total_tokens: json["total_tokens"].as_i64().unwrap_or(0) as i32,
                ..Default::default()
            },
            Err(e) => LlmResponse {
                success: false,
                error_message: format!("Plugin response parse error: {}", e),
                ..Default::default()
            },
        }
    }

    fn get_name(&self) -> &str {
        &self.name
    }

    fn shutdown(&mut self) {
        if let Some(handle) = self.lib_handle.take() {
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
