//! Action bridge — integrates with Tizen Action Framework for device actions.

use libtizenclaw_core::tizen_sys::action::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const ACTIONS_DIR: &str = "/opt/usr/share/tizen-tools/actions";

#[derive(Clone, Debug)]
pub struct ActionSchema {
    pub action_id: String,
    pub display_name: String,
    pub description: String,
    pub parameters: Value,
    pub pkg_id: String,
}

struct BridgeState {
    actions: HashMap<String, ActionSchema>,
    on_change: Option<Box<dyn Fn() + Send + Sync>>,
    client: action_client_h,
    handler: action_event_handler_h,
    running: bool,
}

unsafe impl Send for BridgeState {}
unsafe impl Sync for BridgeState {}

pub struct ActionBridge {
    state: Arc<Mutex<BridgeState>>,
}

unsafe impl Send for ActionBridge {}
unsafe impl Sync for ActionBridge {}

impl Default for ActionBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl ActionBridge {
    pub fn new() -> Self {
        ActionBridge {
            state: Arc::new(Mutex::new(BridgeState {
                actions: HashMap::new(),
                on_change: None,
                client: std::ptr::null_mut(),
                handler: std::ptr::null_mut(),
                running: false,
            })),
        }
    }

    pub fn start(&mut self) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.running {
            return true;
        }

        unsafe {
            let ret = action_client_create(&mut state.client);
            if ret != ACTION_ERROR_NONE {
                log::error!("[TIZENCLAW] ActionBridge: failed to create action client: {}", ret);
                return false;
            }

            // Register event handler
            let state_ptr = Arc::into_raw(self.state.clone()) as *mut c_void;
            let ret2 = action_client_add_event_handler(
                state.client,
                on_action_event,
                state_ptr,
                &mut state.handler,
            );
            if ret2 != ACTION_ERROR_NONE {
                log::warn!("[TIZENCLAW] ActionBridge: failed to register event handler: {}", ret2);
            }
        }

        state.running = true;
        log::info!("ActionBridge started");
        
        let state_arc = self.state.clone();
        drop(state);
        
        // Initial sync
        Self::do_sync_action_schemas(&state_arc);
        
        true
    }

    pub fn stop(&mut self) {
        let mut state = self.state.lock().unwrap();
        if !state.running {
            return;
        }

        unsafe {
            if !state.handler.is_null() && !state.client.is_null() {
                action_client_remove_event_handler(state.client, state.handler);
                state.handler = std::ptr::null_mut();
            }
            if !state.client.is_null() {
                action_client_destroy(state.client);
                state.client = std::ptr::null_mut();
            }
        }

        state.running = false;
        log::info!("ActionBridge stopped");
    }

    pub fn set_change_callback(&mut self, cb: impl Fn() + Send + Sync + 'static) {
        let mut state = self.state.lock().unwrap();
        state.on_change = Some(Box::new(cb));
    }

    pub fn sync_action_schemas(&mut self) {
        Self::do_sync_action_schemas(&self.state);
    }

    pub fn sync_action_schemas_from(&mut self, _dir: &str) {
        Self::do_sync_action_schemas(&self.state);
    }

    fn do_sync_action_schemas(state_arc: &Arc<Mutex<BridgeState>>) {
        let state = state_arc.lock().unwrap();
        if state.client.is_null() {
            return;
        }

        // We use a temporary vector to collect data from C callback
        let mut schemas: Vec<Value> = Vec::new();
        let schemas_ptr = &mut schemas as *mut Vec<Value> as *mut c_void;

        unsafe {
            action_client_foreach_action(state.client, on_action_found, schemas_ptr);
        }

        // Drop the lock during filesystem operations
        drop(state);

        fs::create_dir_all(ACTIONS_DIR).ok();

        let mut actions_map = HashMap::new();
        let mut index_entries = Vec::new();

        for mut schema in schemas {
            // Convert v1 to v2 if necessary
            convert_v1_to_v2(&mut schema);

            let action_id = schema["name"].as_str().unwrap_or("").to_string();
            if action_id.is_empty() {
                continue;
            }

            let pkg_id = extract_pkg_id(&schema);
            let description = schema["description"].as_str().unwrap_or("").to_string();

            // Create markdown
            write_action_md(&pkg_id, &action_id, &schema);
            
            let md_path = if pkg_id.is_empty() {
                format!("{}/{}.md", ACTIONS_DIR, action_id)
            } else {
                format!("{}/{}/{}.md", ACTIONS_DIR, pkg_id, action_id)
            };
            
            index_entries.push(format!("| {} | {} | {} | [Link]({}) |", action_id, pkg_id, description, md_path));

            actions_map.insert(
                action_id.clone(),
                ActionSchema {
                    action_id: action_id.clone(),
                    display_name: action_id.clone(),
                    description,
                    parameters: schema["inputSchema"].clone(),
                    pkg_id,
                },
            );
        }

        // Write index.md
        write_index_md(&index_entries);

        let mut state = state_arc.lock().unwrap();
        let count = actions_map.len();
        state.actions = actions_map;
        log::debug!("ActionBridge: synced {} action schemas", count);
    }

    pub fn get_action_declarations(&self) -> Vec<crate::llm::backend::LlmToolDecl> {
        let state = self.state.lock().unwrap();
        state
            .actions
            .values()
            .map(|a| crate::llm::backend::LlmToolDecl {
                name: format!("action_{}", a.action_id),
                description: a.description.clone(),
                parameters: {
                    // Extract parameters from inputSchema JSON schema
                    // If inputSchema contains properties, just return it. 
                    // Otherwise return empty object schema.
                    a.parameters.clone()
                },
            })
            .collect()
    }

    pub fn execute_action(&self, action_id: &str, params: &Value) -> Value {
        let state = self.state.lock().unwrap();
        if state.client.is_null() {
            return json!({"error": "Action client not started"});
        }

        let schema = match state.actions.get(action_id) {
            Some(s) => s.clone(),
            None => return json!({"error": format!("Unknown action: {}", action_id)}),
        };

        // Execute action asynchronously
        let exec_id = 1; // dummy exec_id for now
        let model = json!({
            "id": exec_id,
            "params": {
                "name": action_id,
                "arguments": params
            }
        });

        let model_str = CString::new(model.to_string()).unwrap();

        // In a real async implementation, we should block or return a Future.
        // For right now, returning JSON with 'launched' status
        unsafe {
            // Note: action_client_execute needs a result callback. 
            // We pass it null or dummy for fire-and-forget, or wait for it.
            // tizenclaw-cpp sends result async via channel. 
            let _ret = action_client_execute(state.client, model_str.as_ptr(), on_action_result, std::ptr::null_mut());
        }

        log::debug!("ActionBridge: executing action '{}' for pkg '{}'", action_id, schema.pkg_id);
        json!({"status": "launched", "action_id": action_id, "pkg_id": &schema.pkg_id})
    }
}

unsafe extern "C" fn on_action_found(action: action_h, user_data: *mut c_void) -> bool {
    let vec_ptr = user_data as *mut Vec<Value>;
    let schemas = &mut *vec_ptr;

    let mut c_name: *mut c_char = std::ptr::null_mut();
    let mut c_schema: *mut c_char = std::ptr::null_mut();

    action_get_name(action, &mut c_name);
    action_get_schema(action, &mut c_schema);

    let mut schema_val = json!({});
    if !c_name.is_null() {
        let name_str = CStr::from_ptr(c_name).to_string_lossy().to_string();
        schema_val["name"] = Value::String(name_str);
        libc::free(c_name as *mut c_void);
    }
    
    if !c_schema.is_null() {
        let schema_str = CStr::from_ptr(c_schema).to_string_lossy().to_string();
        if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&schema_str) {
            let obj = schema_val.as_object_mut().unwrap();
            for (k, v) in map {
                obj.insert(k, v);
            }
        }
        libc::free(c_schema as *mut c_void);
    }

    schemas.push(schema_val);

    true
}

unsafe extern "C" fn on_action_event(action_name: *const c_char, event_type: action_event_type_e, user_data: *mut c_void) {
    let _state_ptr = Arc::from_raw(user_data as *mut Mutex<BridgeState>);
    
    let name = if action_name.is_null() {
        String::new()
    } else {
        CStr::from_ptr(action_name).to_string_lossy().to_string()
    };

    let evt_str = match event_type {
        action_event_type_e::ACTION_EVENT_TYPE_INSTALL => "INSTALL",
        action_event_type_e::ACTION_EVENT_TYPE_UNINSTALL => "UNINSTALL",
        action_event_type_e::ACTION_EVENT_TYPE_UPDATE => "UPDATE",
    };

    log::debug!("[TIZENCLAW] ActionBridge event: {} for '{}'", evt_str, name);

    // Instead of doing full sync here, trigger full sync manually for simplicity
    ActionBridge::do_sync_action_schemas(&_state_ptr);

    // Invoke user callback if present
    {
        let state = _state_ptr.lock().unwrap();
        if let Some(cb) = &state.on_change {
            cb();
        }
    }
    
    // Leak the Arc again because we borrowed it from raw
    let _ = Arc::into_raw(_state_ptr);
}

unsafe extern "C" fn on_action_result(execution_id: c_int, json_result: *const c_char, _user_data: *mut c_void) {
    let result_str = if json_result.is_null() {
        "".to_string()
    } else {
        CStr::from_ptr(json_result).to_string_lossy().to_string()
    };
    log::debug!("[TIZENCLAW] ActionBridge result for exec_id={}: {}", execution_id, result_str);
}

fn convert_v1_to_v2(schema: &mut Value) {
    if !schema.is_object() {
        return;
    }
    let map = schema.as_object_mut().unwrap();

    // Check version 
    let version = map.get("version").and_then(|v| v.as_str()).unwrap_or("");
    if version != "v1" {
        // If it already has inputSchema, it's likely v2
        if map.contains_key("inputSchema") {
            return;
        }
    }

    // Convert desc -> description
    if let Some(desc) = map.remove("desc") {
        map.insert("description".to_string(), desc);
    }

    // Convert params -> inputSchema
    if let Some(params) = map.remove("params") {
        let mut input_schema = json!({
            "type": "object",
            "properties": {},
            "required": []
        });

        if let Some(params_map) = params.as_object() {
            let mut required_fields = Vec::new();
            let mut props = serde_json::Map::new();

            for (k, v) in params_map {
                let mut prop = json!({});
                if let Some(t) = v.get("type") {
                    prop["type"] = t.clone();
                }
                if let Some(desc) = v.get("desc") {
                    prop["description"] = desc.clone();
                }
                if let Some(is_req) = v.get("isRequired") {
                    if is_req.as_bool().unwrap_or(false) {
                        required_fields.push(json!(k.clone()));
                    }
                }
                props.insert(k.clone(), prop);
            }

            input_schema["properties"] = Value::Object(props);
            if !required_fields.is_empty() {
                input_schema["required"] = Value::Array(required_fields);
            }
        }
        map.insert("inputSchema".to_string(), input_schema);
    }
}

fn extract_pkg_id(schema: &Value) -> String {
    if let Some(details) = schema.get("details") {
        if let Some(pkgid) = details.get("providerPkgid").and_then(|v| v.as_str()) {
            return pkgid.to_string();
        }
        if let Some(appid) = details.get("appid").and_then(|v| v.as_str()) {
            return appid.to_string();
        }
    }
    // Fallback if structured differently
    schema.get("package_id")
        .or_else(|| schema.get("package"))
        .or_else(|| schema.get("app_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn write_action_md(pkg_id: &str, action_id: &str, schema: &Value) {
    let dir_path = if pkg_id.is_empty() {
        PathBuf::from(ACTIONS_DIR)
    } else {
        let dir = Path::new(ACTIONS_DIR).join(pkg_id);
        fs::create_dir_all(&dir).ok();
        dir
    };

    let md_path = dir_path.join(format!("{}.md", action_id));

    let description = schema.get("description").and_then(|v| v.as_str()).unwrap_or("");
    let category = schema.get("category").and_then(|v| v.as_str()).unwrap_or("");

    let mut md_content = format!("# Action: {}\n\n", action_id);
    if !description.is_empty() {
        md_content.push_str(&format!("*Description*: {}\n\n", description));
    }
    if !category.is_empty() {
        md_content.push_str(&format!("*Category*: {}\n\n", category));
    }

    if let Some(req_privs) = schema.get("requiredPrivileges").and_then(|v| v.as_array()) {
        if !req_privs.is_empty() {
            md_content.push_str("## Required Privileges\n");
            for p in req_privs {
                md_content.push_str(&format!("- {}\n", p.as_str().unwrap_or("")));
            }
            md_content.push('\n');
        }
    }

    if let Some(input_schema) = schema.get("inputSchema") {
        md_content.push_str("## Input Schema / Parameters\n\n```json\n");
        md_content.push_str(&serde_json::to_string_pretty(input_schema).unwrap_or_default());
        md_content.push_str("\n```\n\n");
    }

    md_content.push_str("## Raw Details\n\n```json\n");
    md_content.push_str(&serde_json::to_string_pretty(schema).unwrap_or_default());
    md_content.push_str("\n```\n");

    let _ = fs::write(&md_path, md_content);
}

fn write_index_md(entries: &[String]) {
    let mut index_content = String::from("# Tizen Action Framework Index\n\n");
    index_content.push_str("This index provides quick access to all currently registered action schemas.\n\n");
    index_content.push_str("| Action ID | Application ID | Description | Details Path |\n");
    index_content.push_str("| --- | --- | --- | --- |\n");

    for entry in entries {
        index_content.push_str(entry);
        index_content.push('\n');
    }

    let index_path = Path::new(ACTIONS_DIR).join("index.md");
    let _ = fs::write(&index_path, index_content);
}
