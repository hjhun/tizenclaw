impl AgentCore {
    pub fn new(platform: Arc<libtizenclaw_core::framework::PlatformContext>) -> Self {
        let keys_dir = platform.paths.config_dir.join("keys");
        AgentCore {
            platform,
            provider_registry: tokio::sync::RwLock::new(
                crate::core::provider_selection::ProviderRegistry::default(),
            ),
            session_store: Mutex::new(None),
            tool_dispatcher: tokio::sync::RwLock::new(ToolDispatcher::new()),
            safety_guard: Arc::new(Mutex::new(SafetyGuard::new())),
            context_engine: Arc::new(SizedContextEngine::new()),
            event_bus: Arc::new(EventBus::new()),
            key_store: Mutex::new(KeyStore::new(&keys_dir)),
            system_prompt: RwLock::new(String::new()),
            soul_content: RwLock::new(None),
            llm_config: Mutex::new(LlmConfig::default()),
            circuit_breakers: RwLock::new(std::collections::HashMap::new()),
            action_bridge: Mutex::new(crate::core::action_bridge::ActionBridge::new()),
            tool_policy: Mutex::new(crate::core::tool_policy::ToolPolicy::new()),
            memory_store: Mutex::new(None),
            workflow_engine: tokio::sync::RwLock::new(
                crate::core::workflow_engine::WorkflowEngine::new(),
            ),
            agent_roles: RwLock::new(AgentRoleRegistry::new()),
            session_profiles: Mutex::new(HashMap::new()),
            prompt_hash: tokio::sync::RwLock::new(0),
        }
    }

    /// Compute a fast 64-bit hash of an arbitrary string slice.
    /// Used to detect system_prompt changes without storing the full text.
    fn hash_str(s: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        s.hash(&mut h);
        h.finish()
    }

    fn role_file_path(&self) -> PathBuf {
        self.platform.paths.config_dir.join("agent_roles.json")
    }

    fn safety_guard_path(&self) -> PathBuf {
        self.platform.paths.config_dir.join("safety_guard.json")
    }

    fn reload_safety_guard(&self) {
        let guard_path = self.safety_guard_path();
        if let Ok(mut safety_guard) = self.safety_guard.lock() {
            *safety_guard = SafetyGuard::new();
            safety_guard.load_config(&guard_path.to_string_lossy());
        }
    }

    fn publish_runtime_event(&self, event_name: &str, data: Value) {
        self.event_bus.publish(SystemEvent {
            event_type: EventType::Custom(event_name.to_string()),
            source: "agent_core".to_string(),
            data,
            timestamp: 0,
        });
    }

    fn persist_compacted_messages(&self, session_id: &str, messages: &[LlmMessage]) {
        if let Ok(ss) = self.session_store.lock() {
            if let Some(store) = ss.as_ref() {
                use crate::storage::session_store::SessionMessage;
                let session_msgs: Vec<SessionMessage> = messages
                    .iter()
                    .map(SessionMessage::from_llm_message)
                    .collect();
                if let Err(err) = store.save_compacted_structured(session_id, &session_msgs) {
                    log::warn!(
                        "[ContextEngine] Failed to save compacted structured snapshot: {}",
                        err
                    );
                }
                if let Err(err) = store.save_compacted(session_id, &session_msgs) {
                    log::warn!("[ContextEngine] Failed to save compacted.md: {}", err);
                }
            }
        }
    }

    fn check_context_message_limit(
        &self,
        session_id: &str,
        messages: &[LlmMessage],
        loop_state: &mut AgentLoopState,
    ) -> Result<(), String> {
        if messages.len() <= MAX_CONTEXT_MESSAGES {
            return Ok(());
        }

        let error = format!(
            "Context message limit exceeded for session '{}': {} > {}",
            session_id,
            messages.len(),
            MAX_CONTEXT_MESSAGES
        );
        log::error!("[AgentLoop] {}", error);
        loop_state.last_error = Some(error.clone());
        loop_state.mark_terminal(
            LoopTransitionReason::RoundLimitReached,
            format!(
                "message count {} exceeded {}",
                messages.len(),
                MAX_CONTEXT_MESSAGES
            ),
        );
        self.persist_loop_snapshot(loop_state);
        Err(error)
    }

    fn resolve_session_profile(&self, session_id: &str) -> Option<SessionPromptProfile> {
        self.session_profiles
            .lock()
            .ok()
            .and_then(|profiles| profiles.get(session_id).cloned())
    }

    fn role_registry_snapshot(&self) -> Vec<AgentRole> {
        self.agent_roles
            .read()
            .map(|registry| {
                registry
                    .get_role_names()
                    .into_iter()
                    .filter_map(|name| registry.get_role(&name).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn bridge_builtin_tools() -> Vec<backend::LlmToolDecl> {
        let mut builtins = Vec::new();
        crate::core::tool_declaration_builder::ToolDeclarationBuilder::append_builtin_tools(
            &mut builtins,
            "workflow memory search",
        );
        builtins.push(backend::LlmToolDecl {
            name: "execute_cli".into(),
            description: "Execute a registered CLI tool by name for Bridge API compatibility."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tool_name": {
                        "type": "string",
                        "description": "Registered CLI tool name such as tizen-device-info-cli"
                    },
                    "arguments": {
                        "description": "Optional CLI arguments as a string or object",
                        "oneOf": [
                            {"type": "string"},
                            {"type": "object"}
                        ]
                    }
                },
                "required": ["tool_name"]
            }),
        });
        let supported = [
            "execute_cli",
            "generate_web_app",
            "remember",
            "recall",
            "forget",
            "web_search",
            "validate_web_search",
        ];
        builtins.retain(|tool| supported.iter().any(|name| tool.name == *name));
        builtins
    }

    pub async fn get_bridge_tool_declarations(
        &self,
        allowed_tools: &[String],
    ) -> Vec<backend::LlmToolDecl> {
        let mut tools = self.tool_dispatcher.read().await.get_tool_declarations();
        tools.extend(Self::bridge_builtin_tools());
        if let Ok(bridge) = self.action_bridge.lock() {
            tools.extend(bridge.get_action_declarations());
        }

        let mut seen = std::collections::HashSet::new();
        tools.retain(|tool| seen.insert(tool.name.clone()));

        if !allowed_tools.is_empty() {
            tools.retain(|tool| allowed_tools.iter().any(|name| name == &tool.name));
        }

        tools.sort_by(|left, right| left.name.cmp(&right.name));
        tools
    }

    async fn send_outbound_message(&self, args: &Value, default_session_id: Option<&str>) -> Value {
        let channels = args
            .get("channels")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(|item| item.trim().to_string()))
                    .filter(|item| !item.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let message = args
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim();
        let title = args
            .get("title")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let session_id = args
            .get("session_id")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or(default_session_id);

        if channels.is_empty() {
            return json!({"error": "At least one outbound channel is required"});
        }
        if message.is_empty() {
            return json!({"error": "Outbound message cannot be empty"});
        }

        let mut results = Vec::new();
        for channel in channels {
            match channel.as_str() {
                "web_dashboard" => match append_dashboard_outbound_message(
                    &self.platform.paths.data_dir,
                    title,
                    message,
                    session_id,
                ) {
                    Ok(record) => results.push(json!({
                        "channel": "web_dashboard",
                        "status": "sent",
                        "record": record,
                    })),
                    Err(err) => results.push(json!({
                        "channel": "web_dashboard",
                        "status": "error",
                        "error": err,
                    })),
                },
                "telegram" => {
                    results.push(self.send_telegram_outbound_message(title, message).await);
                }
                other => results.push(json!({
                    "channel": other,
                    "status": "error",
                    "error": "Unsupported outbound channel",
                })),
            }
        }

        let success = results
            .iter()
            .any(|item| item.get("status").and_then(|value| value.as_str()) == Some("sent"));

        json!({
            "status": if success { "success" } else { "error" },
            "message": message,
            "session_id": session_id,
            "results": results,
        })
    }

    async fn send_telegram_outbound_message(&self, title: Option<&str>, message: &str) -> Value {
        let config_path = self.platform.paths.config_dir.join("telegram_config.json");
        let content = match std::fs::read_to_string(&config_path) {
            Ok(content) => content,
            Err(err) => {
                return json!({
                    "channel": "telegram",
                    "status": "error",
                    "error": format!("Failed to read telegram config: {}", err),
                });
            }
        };

        let config: Value = match serde_json::from_str(&content) {
            Ok(config) => config,
            Err(err) => {
                return json!({
                    "channel": "telegram",
                    "status": "error",
                    "error": format!("Invalid telegram config JSON: {}", err),
                });
            }
        };

        let bot_token = config
            .get("bot_token")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if bot_token.is_empty() || bot_token == "YOUR_TELEGRAM_BOT_TOKEN_HERE" {
            return json!({
                "channel": "telegram",
                "status": "error",
                "error": "Telegram bot token is not configured",
            });
        }

        let chat_ids = config
            .get("allowed_chat_ids")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_i64())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if chat_ids.is_empty() {
            return json!({
                "channel": "telegram",
                "status": "error",
                "error": "No allowed_chat_ids configured for Telegram",
            });
        }

        let composed = if let Some(title) = title {
            format!("{}\n\n{}", title, message)
        } else {
            message.to_string()
        };
        let safe_text = if composed.chars().count() > MAX_TELEGRAM_OUTBOUND_CHARS {
            format!(
                "{}\n...(truncated)",
                utf8_safe_preview(&composed, MAX_TELEGRAM_OUTBOUND_CHARS)
            )
        } else {
            composed
        };

        let client = crate::infra::http_client::HttpClient::new();
        let url = format!("https://api.telegram.org/bot{}/sendMessage", bot_token);
        let mut delivered = Vec::new();
        let mut errors = Vec::new();

        for chat_id in chat_ids {
            let payload = json!({
                "chat_id": chat_id,
                "text": safe_text.clone(),
            })
            .to_string();
            match client.post(&url, &payload).await {
                Ok(resp) if resp.status_code < 400 => delivered.push(chat_id),
                Ok(resp) => errors.push(format!(
                    "chat {} returned HTTP {}",
                    chat_id, resp.status_code
                )),
                Err(err) => errors.push(format!("chat {} failed: {}", chat_id, err)),
            }
        }

        json!({
            "channel": "telegram",
            "status": if delivered.is_empty() { "error" } else { "sent" },
            "delivered_chat_ids": delivered,
            "errors": errors,
        })
    }

    async fn generate_web_app(&self, args: &Value) -> Value {
        let app_id = args
            .get("app_id")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim();
        let title = args
            .get("title")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim();
        let html = args
            .get("html")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let css = args
            .get("css")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let js = args
            .get("js")
            .and_then(|value| value.as_str())
            .unwrap_or("");

        if app_id.is_empty() || app_id.len() > 64 {
            return json!({"error": "app_id is required (max 64 chars)"});
        }
        if !app_id
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
        {
            return json!({"error": "app_id must be lowercase alphanumeric + underscore only"});
        }
        if title.is_empty() {
            return json!({"error": "title is required"});
        }
        if html.is_empty() {
            return json!({"error": "html is required"});
        }

        let html = Self::inject_generated_web_app_assets(html, !css.is_empty(), !js.is_empty());

        let apps_dir = self.platform.paths.web_root.join("apps");
        let app_dir = apps_dir.join(app_id);
        if let Err(err) = std::fs::create_dir_all(&app_dir) {
            return json!({"error": format!("Failed to create app directory: {}", err)});
        }

        if let Err(err) = std::fs::write(app_dir.join("index.html"), html) {
            return json!({"error": format!("Failed to write index.html: {}", err)});
        }
        if !css.is_empty() {
            if let Err(err) = std::fs::write(app_dir.join("style.css"), css) {
                return json!({"error": format!("Failed to write style.css: {}", err)});
            }
        }
        if !js.is_empty() {
            if let Err(err) = std::fs::write(app_dir.join("app.js"), js) {
                return json!({"error": format!("Failed to write app.js: {}", err)});
            }
        }

        let mut downloaded_assets = Vec::new();
        if let Some(assets) = args.get("assets").and_then(|value| value.as_array()) {
            let client = reqwest::Client::new();
            for asset in assets {
                let url = asset
                    .get("url")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .trim();
                let filename = asset
                    .get("filename")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .trim();

                if url.is_empty() || filename.is_empty() {
                    continue;
                }
                if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
                    downloaded_assets.push(json!({
                        "filename": filename,
                        "status": "failed",
                        "error": "Unsafe filename",
                    }));
                    continue;
                }

                let target_path = app_dir.join(filename);
                let asset_result = async {
                    let response = client
                        .get(url)
                        .send()
                        .await
                        .map_err(|err| err.to_string())?;
                    if !response.status().is_success() {
                        return Err(format!("HTTP {}", response.status()));
                    }
                    if let Some(len) = response.content_length() {
                        if len > 10 * 1024 * 1024 {
                            return Err("Asset exceeds 10MB limit".to_string());
                        }
                    }
                    let bytes = response.bytes().await.map_err(|err| err.to_string())?;
                    if bytes.len() > 10 * 1024 * 1024 {
                        return Err("Asset exceeds 10MB limit".to_string());
                    }
                    std::fs::write(&target_path, &bytes).map_err(|err| err.to_string())?;
                    Ok::<(), String>(())
                }
                .await;

                match asset_result {
                    Ok(()) => downloaded_assets.push(json!({
                        "filename": filename,
                        "status": "ok",
                    })),
                    Err(err) => {
                        let _ = std::fs::remove_file(&target_path);
                        downloaded_assets.push(json!({
                            "filename": filename,
                            "status": "failed",
                            "error": err,
                        }));
                    }
                }
            }
        }

        let mut manifest = json!({
            "app_id": app_id,
            "title": title,
            "created_at": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            "has_css": !css.is_empty(),
            "has_js": !js.is_empty(),
            "assets": downloaded_assets,
        });

        if let Some(allowed_tools) = args.get("allowed_tools").and_then(|value| value.as_array()) {
            manifest["allowed_tools"] = Value::Array(
                allowed_tools
                    .iter()
                    .filter_map(|value| value.as_str().map(|item| Value::String(item.to_string())))
                    .collect(),
            );
        }

        let manifest_bytes = match serde_json::to_vec_pretty(&manifest) {
            Ok(bytes) => bytes,
            Err(err) => {
                return json!({"error": format!("Failed to encode manifest.json: {}", err)});
            }
        };
        if let Err(err) = std::fs::write(app_dir.join("manifest.json"), manifest_bytes) {
            return json!({"error": format!("Failed to write manifest.json: {}", err)});
        }

        let is_tizen = std::path::Path::new("/etc/tizen-release").exists();
        let dashboard_base_url = crate::core::runtime_paths::default_dashboard_base_url();
        let launched = self.launch_generated_web_app(app_id);
        let message = if is_tizen {
            if launched {
                format!(
                    "Web app created and launched at {}/apps/{}/",
                    dashboard_base_url, app_id
                )
            } else {
                format!(
                    "Web app created. Access at {}/apps/{}/",
                    dashboard_base_url, app_id
                )
            }
        } else {
            format!(
                "Web app created. Open {}/apps/{}/ on the host.",
                dashboard_base_url, app_id
            )
        };
        json!({
            "status": "ok",
            "app_id": app_id,
            "title": title,
            "url": format!("/apps/{}/", app_id),
            "webview_launched": launched,
            "message": message,
            "assets": manifest["assets"].clone(),
        })
    }

    fn inject_generated_web_app_assets(html: &str, has_css: bool, has_js: bool) -> String {
        let mut rendered = html.to_string();

        if has_css && !html.contains("style.css") {
            let link_tag = r#"<link rel="stylesheet" href="style.css">"#;
            if let Some(index) = rendered.find("</head>") {
                rendered.insert_str(index, &format!("    {}\n", link_tag));
            } else if let Some(index) = rendered.find("<body") {
                rendered.insert_str(index, &format!("{}\n", link_tag));
            } else {
                rendered.push_str(&format!("\n{}\n", link_tag));
            }
        }

        if has_js && !html.contains("app.js") {
            let script_tag = r#"<script src="app.js"></script>"#;
            if let Some(index) = rendered.rfind("</body>") {
                rendered.insert_str(index, &format!("    {}\n", script_tag));
            } else if let Some(index) = rendered.rfind("</html>") {
                rendered.insert_str(index, &format!("{}\n", script_tag));
            } else {
                rendered.push_str(&format!("\n{}\n", script_tag));
            }
        }

        rendered
    }

    fn launch_generated_web_app(&self, app_id: &str) -> bool {
        if !std::path::Path::new("/etc/tizen-release").exists() {
            return false;
        }

        let app_url = format!(
            "{}/apps/{}/",
            crate::core::runtime_paths::default_dashboard_base_url(),
            app_id
        );
        if self.launch_app_with_bundle("QvaPeQ7RDA.tizenclawbridge", &[("url", app_url.as_str())]) {
            return true;
        }
        if self.launch_app_open("QvaPeQ7RDA.tizenclawbridge") {
            return true;
        }
        if self.launch_app_with_bundle(
            "org.tizen.tizenclaw-webview",
            &[("__APP_SVC_URI__", app_url.as_str())],
        ) {
            return true;
        }
        if self.launch_app_open("org.tizen.tizenclaw-webview") {
            return true;
        }
        if self.launch_app_with_request(
            "QvaPeQ7RDA.tizenclawbridge",
            &app_url,
            &[("url", app_url.as_str())],
        ) {
            return true;
        }
        if self.launch_app_with_request(
            "org.tizen.tizenclaw-webview",
            &app_url,
            &[("__APP_SVC_URI__", app_url.as_str())],
        ) {
            return true;
        }
        self.platform
            .app_control
            .launch_app("org.tizen.tizenclaw-webview")
            .is_ok()
    }

    fn launch_app_with_bundle(&self, app_id: &str, extras: &[(&str, &str)]) -> bool {
        unsafe {
            use libtizenclaw_core::tizen_sys::{aul::*, bundle::*};

            let Ok(app_id) = std::ffi::CString::new(app_id) else {
                return false;
            };
            let bundle = bundle_create();
            if bundle.is_null() {
                return false;
            }

            for (key, value) in extras {
                let Ok(key) = std::ffi::CString::new(*key) else {
                    continue;
                };
                let Ok(value) = std::ffi::CString::new(*value) else {
                    continue;
                };
                let _ = bundle_add_str(bundle, key.as_ptr(), value.as_ptr());
            }

            let result = aul_launch_app(app_id.as_ptr(), bundle.cast());
            let _ = bundle_free(bundle);
            result >= 0
        }
    }

    fn launch_app_open(&self, app_id: &str) -> bool {
        unsafe {
            use libtizenclaw_core::tizen_sys::aul::aul_open_app;

            let Ok(app_id) = std::ffi::CString::new(app_id) else {
                return false;
            };
            aul_open_app(app_id.as_ptr()) >= 0
        }
    }

    fn launch_app_with_request(&self, app_id: &str, uri: &str, extras: &[(&str, &str)]) -> bool {
        unsafe {
            use libtizenclaw_core::tizen_sys::app_control::*;

            let mut handle: app_control_h = std::ptr::null_mut();
            if app_control_create(&mut handle) != APP_CONTROL_ERROR_NONE {
                return false;
            }

            let operation =
                match std::ffi::CString::new("http://tizen.org/appcontrol/operation/default") {
                    Ok(value) => value,
                    Err(_) => {
                        let _ = app_control_destroy(handle);
                        return false;
                    }
                };
            let app_id = match std::ffi::CString::new(app_id) {
                Ok(value) => value,
                Err(_) => {
                    let _ = app_control_destroy(handle);
                    return false;
                }
            };
            let uri = match std::ffi::CString::new(uri) {
                Ok(value) => value,
                Err(_) => {
                    let _ = app_control_destroy(handle);
                    return false;
                }
            };

            let _ = app_control_set_operation(handle, operation.as_ptr());
            let _ = app_control_set_app_id(handle, app_id.as_ptr());
            let _ = app_control_set_uri(handle, uri.as_ptr());

            for (key, value) in extras {
                let Ok(key) = std::ffi::CString::new(*key) else {
                    continue;
                };
                let Ok(value) = std::ffi::CString::new(*value) else {
                    continue;
                };
                let _ = app_control_add_extra_data(handle, key.as_ptr(), value.as_ptr());
            }

            let result = app_control_send_launch_request(handle, None, std::ptr::null_mut());
            let _ = app_control_destroy(handle);
            result == APP_CONTROL_ERROR_NONE
        }
    }

    pub async fn execute_bridge_tool(
        &self,
        tool_name: &str,
        args: &Value,
        allowed_tools: &[String],
    ) -> Value {
        if !allowed_tools.is_empty() && !allowed_tools.iter().any(|name| name == tool_name) {
            return json!({"error": format!("Tool not allowed for this app: {}", tool_name)});
        }

        if let Ok(policy) = self.tool_policy.lock() {
            if matches!(
                policy.get_risk_level(tool_name),
                crate::core::tool_policy::RiskLevel::High
            ) {
                return json!({"error": format!("High-risk tool not available via bridge: {}", tool_name)});
            }
        }

        match tool_name {
            "execute_cli" => {
                let requested_tool = args
                    .get("tool_name")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .trim();
                if requested_tool.is_empty() {
                    return json!({"error": "Missing CLI tool name"});
                }

                let cli_args = match args.get("arguments") {
                    Some(Value::String(raw)) => json!({ "args": raw }),
                    Some(Value::Object(map)) => Value::Object(map.clone()),
                    Some(other) if !other.is_null() => json!({ "args": other.to_string() }),
                    _ => json!({}),
                };

                let candidates = [
                    requested_tool.to_string(),
                    requested_tool.replace("tizenclaw-", ""),
                ];

                for candidate in candidates {
                    let result = match self
                        .tool_dispatcher
                        .read()
                        .await
                        .execute(&candidate, &cli_args, None)
                        .await
                    {
                        Ok(value) => value,
                        Err(error) => json!({ "error": error }),
                    };
                    let is_unknown = result
                        .get("error")
                        .and_then(|value| value.as_str())
                        .map(|value| value.contains("Unknown tool:"))
                        .unwrap_or(false);
                    if !is_unknown {
                        return result;
                    }
                }

                json!({"error": format!("Unknown CLI tool: {}", requested_tool)})
            }
            "generate_web_app" => self.generate_web_app(args).await,
            "file_manager" => {
                let requested_session_id = args
                    .get("session_id")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("ipc-bridge");
                let session_workdir = if let Ok(store_guard) = self.session_store.lock() {
                    if let Some(store) = store_guard.as_ref() {
                        store.ensure_session(requested_session_id);
                        store.session_workdir(requested_session_id)
                    } else {
                        let fallback = self
                            .platform
                            .paths
                            .data_dir
                            .join("bridge_tool")
                            .join(requested_session_id);
                        let _ = std::fs::create_dir_all(&fallback);
                        fallback
                    }
                } else {
                    let fallback = self
                        .platform
                        .paths
                        .data_dir
                        .join("bridge_tool")
                        .join(requested_session_id);
                    let _ = std::fs::create_dir_all(&fallback);
                    fallback
                };
                file_manager_tool(args, &session_workdir).await
            }
            "validate_web_search" => {
                let engine = args.get("engine").and_then(|value| value.as_str());
                feature_tools::validate_web_search(&self.platform.paths.config_dir, engine)
            }
            "web_search" => {
                let query = args
                    .get("query")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let engine = args.get("engine").and_then(|value| value.as_str());
                let limit = args
                    .get("limit")
                    .and_then(|value| value.as_u64())
                    .map(|value| value as usize)
                    .unwrap_or(5);
                feature_tools::web_search(
                    query,
                    engine,
                    limit,
                    &self.platform.paths.data_dir,
                    &self.platform.paths.config_dir,
                )
                .await
            }
            "remember" => {
                let key = args
                    .get("key")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let value = args
                    .get("value")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                let category = args
                    .get("category")
                    .and_then(|value| value.as_str())
                    .unwrap_or("general");
                match self.memory_store.lock() {
                    Ok(store_guard) => {
                        if let Some(store) = store_guard.as_ref() {
                            if key.is_empty() || value.is_empty() {
                                json!({"error": "Missing key or value"})
                            } else {
                                store.set(key, value, category);
                                json!({"status": "success", "message": format!("Remembered '{}'", key)})
                            }
                        } else {
                            json!({"error": "MemoryStore not initialized"})
                        }
                    }
                    Err(_) => json!({"error": "MemoryStore lock failed"}),
                }
            }
            "recall" => {
                let key = args
                    .get("key")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                match self.memory_store.lock() {
                    Ok(store_guard) => {
                        if let Some(store) = store_guard.as_ref() {
                            if let Some(value) = store.get(key) {
                                json!({"status": "success", "value": value})
                            } else {
                                json!({"error": "Key not found"})
                            }
                        } else {
                            json!({"error": "MemoryStore not initialized"})
                        }
                    }
                    Err(_) => json!({"error": "MemoryStore lock failed"}),
                }
            }
            "forget" => {
                let key = args
                    .get("key")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                match self.memory_store.lock() {
                    Ok(store_guard) => {
                        if let Some(store) = store_guard.as_ref() {
                            if store.delete(key) {
                                json!({"status": "success", "message": format!("Forgot '{}'", key)})
                            } else {
                                json!({"error": "Key not found"})
                            }
                        } else {
                            json!({"error": "MemoryStore not initialized"})
                        }
                    }
                    Err(_) => json!({"error": "MemoryStore lock failed"}),
                }
            }
            _ if tool_name.starts_with("action_") => match self.action_bridge.lock() {
                Ok(bridge) => {
                    let action_id = tool_name.strip_prefix("action_").unwrap_or(tool_name);
                    bridge.execute_action(action_id, args)
                }
                Err(_) => json!({"error": "Failed to lock action bridge"}),
            },
            _ => {
                match self
                    .tool_dispatcher
                    .read()
                    .await
                    .execute_in_dir(tool_name, args, None, Some(&self.platform.paths.data_dir))
                    .await
                {
                    Ok(value) => value,
                    Err(error) => json!({ "error": error }),
                }
            }
        }
    }

    fn build_session_profile(
        &self,
        role_name: Option<&str>,
        system_prompt: Option<&str>,
        prompt_mode: Option<PromptMode>,
        reasoning_policy: Option<ReasoningPolicy>,
        allowed_tools: Option<Vec<String>>,
        description: Option<&str>,
    ) -> Result<SessionPromptProfile, String> {
        let mut profile = SessionPromptProfile::default();

        if let Some(role_name) = role_name.filter(|name| !name.trim().is_empty()) {
            let role = self
                .agent_roles
                .read()
                .ok()
                .and_then(|registry| registry.get_role(role_name).cloned())
                .ok_or_else(|| format!("Unknown agent role '{}'", role_name))?;
            profile.role_name = Some(role.name.clone());
            profile.role_description = Some(role.description.clone());
            if !role.system_prompt.trim().is_empty() {
                profile.system_prompt = Some(role.system_prompt);
            }
            if !role.allowed_tools.is_empty() {
                profile.allowed_tools = Some(role.allowed_tools);
            }
            profile.max_iterations = Some(role.max_iterations);
            profile.role_type = Some(role.role_type);
            if !role.can_delegate_to.is_empty() {
                profile.can_delegate_to = Some(role.can_delegate_to);
            }
            profile.prompt_mode = role.prompt_mode;
            profile.reasoning_policy = role.reasoning_policy;
        }

        if let Some(system_prompt) = system_prompt.filter(|value| !value.trim().is_empty()) {
            profile.system_prompt = Some(system_prompt.trim().to_string());
        }
        if let Some(prompt_mode) = prompt_mode {
            profile.prompt_mode = Some(prompt_mode);
        }
        if let Some(reasoning_policy) = reasoning_policy {
            profile.reasoning_policy = Some(reasoning_policy);
        }
        if let Some(allowed_tools) = allowed_tools.filter(|items| !items.is_empty()) {
            profile.allowed_tools = Some(allowed_tools);
        }
        if let Some(description) = description.filter(|value| !value.trim().is_empty()) {
            profile.role_description = Some(description.trim().to_string());
        }

        Ok(profile)
    }

    pub async fn initialize(&self) -> bool {
        log::debug!("AgentCore initializing...");
        let paths = &self.platform.paths;
        let _ = self.event_bus.start();

        let policy_path = paths.config_dir.join("tool_policy.json");
        if let Ok(mut tp) = self.tool_policy.lock() {
            tp.load_config(&policy_path.to_string_lossy());
        }
        self.reload_safety_guard();

        // Load system prompt
        let prompt_path = paths.config_dir.join("system_prompt.txt");
        let prompt = std::fs::read_to_string(&prompt_path).unwrap_or_else(|_| {
            "You are TizenClaw, an AI assistant that can execute tools \
             to help users interact with the system."
                .into()
        });
        if let Ok(mut sp) = self.system_prompt.write() {
            *sp = prompt;
        }

        // Load SOUL persona if present
        let soul_path = paths.config_dir.join("SOUL.md");
        if let Ok(soul) = std::fs::read_to_string(&soul_path) {
            log::info!("Loaded persona from SOUL.md");
            if let Ok(mut sc) = self.soul_content.write() {
                *sc = Some(soul);
            }
        }

        if let Ok(mut roles) = self.agent_roles.write() {
            roles.ensure_builtin_roles();
            let role_path = self.role_file_path();
            let _ = roles.load_roles(&role_path.to_string_lossy());
        }

        // Load LLM config (supports multi-backend + fallback)
        let llm_config_path = paths.config_dir.join("llm_config.json");
        let config = LlmConfig::load(&llm_config_path.to_string_lossy());
        let llm_doc = crate::core::llm_config_store::load(&paths.config_dir)
            .unwrap_or_else(|_| crate::core::llm_config_store::default_document());

        // Initialize plugin manager
        let mut plugin_manager = crate::llm::plugin_manager::PluginManager::new();
        // Plugins are exclusively scanned via PackageManager via `scan_plugins`.
        plugin_manager.scan_plugins(Some(self.platform.package_manager.as_ref()));

        // Build provider routing config from llm_config document.
        let routing =
            crate::core::provider_selection::ProviderCompatibilityTranslator::translate(&llm_doc);

        // Unified priority-based selection — candidates are ordered by priority.
        let candidates = self.get_backend_candidates(&config, &plugin_manager);

        // Initialize backends; build the provider registry in configured preference order.
        let mut instances: Vec<crate::core::provider_selection::ProviderInstance> = Vec::new();
        let mut failed_inits_startup: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        // Iterate candidate list in priority order and initialize each backend.
        for cand in &candidates {
            let merged_cfg = {
                let ks_guard = self.key_store.lock().unwrap_or_else(|e| e.into_inner());
                let base = config.backend_config(&cand.name);
                merge_backend_auth(base, &cand.name, &ks_guard)
            };

            if let Some(be) =
                Self::create_and_init_backend_static(&plugin_manager, &cand.name, merged_cfg)
            {
                let slot = if instances.is_empty() { "Primary" } else { "Fallback" };
                log::info!(
                    "{} LLM backend '{}' initialized (priority {})",
                    slot,
                    cand.name,
                    cand.priority
                );
                instances.push(crate::core::provider_selection::ProviderInstance {
                    name: cand.name.clone(),
                    backend: be,
                    last_init_error: None,
                });
            } else {
                log::warn!("Backend '{}' failed to initialize", cand.name);
                failed_inits_startup.insert(
                    cand.name.clone(),
                    "not configured or initialization failed".to_string(),
                );
            }
        }

        // Re-order instances to match the configured routing preference order so
        // ProviderSelector iterates in the operator-specified order, not the
        // initialization candidate order which may differ (e.g. plugin priority).
        let ordered_names = routing.ordered_names();
        if routing.providers_array_present || !ordered_names.is_empty() {
            instances.sort_by_key(|inst| {
                ordered_names
                    .iter()
                    .position(|name| *name == inst.name.as_str())
                    .unwrap_or(usize::MAX)
            });
            // When the `providers` array key is present it is authoritative.
            // Drop only instances that are explicitly listed and disabled.
            // Plugin-discovered backends absent from `providers[]` are kept
            // at the end of the list as last-resort fallbacks.
            if routing.providers_array_present {
                instances.retain(|inst| {
                    routing
                        .providers
                        .iter()
                        .find(|p| p.name == inst.name)
                        .map(|p| p.enabled)
                        .unwrap_or(true)
                });
                log::debug!(
                    "Init: providers[] authoritative — retained {} enabled instance(s)",
                    instances.len()
                );
            }
        }

        if instances.is_empty() {
            log::error!("Failed to initialize ANY backend from candidates list!");
        }

        let registry = crate::core::provider_selection::ProviderRegistry::new(
            routing,
            instances,
            failed_inits_startup,
        );
        *self.provider_registry.write().await = registry;

        // Store config for later use
        if let Ok(mut cfg) = self.llm_config.lock() {
            *cfg = config;
        }

        // Initialize session store
        let db_path = paths.sessions_db_path();
        match SessionStore::new(&paths.app_data_dir(), &db_path.to_string_lossy()) {
            Ok(store) => {
                log::info!("Session store initialized");
                if let Ok(mut ss) = self.session_store.lock() {
                    *ss = Some(store);
                }
            }
            Err(e) => log::error!("Session store failed: {}", e),
        }

        // Initialize memory store
        let mem_dir = paths.data_dir.join("memory");
        let mem_db = mem_dir.join("memories.db");
        let model_dir = paths.data_dir.join("models");
        match crate::storage::memory_store::MemoryStore::new(
            &mem_dir.to_string_lossy(),
            &mem_db.to_string_lossy(),
            &model_dir.to_string_lossy(),
        ) {
            Ok(store) => {
                log::info!("Memory store initialized");
                if let Ok(mut ms) = self.memory_store.lock() {
                    *ms = Some(store);
                }
            }
            Err(e) => log::error!("Memory store failed: {}", e),
        }

        // Load tools from all subdirectories under the tools directory
        {
            let mut td = self.tool_dispatcher.write().await;
            let tool_roots = collect_tool_roots(paths);
            td.load_tools_from_paths(tool_roots.iter().map(|root| root.as_str()));
        }
        log::info!("Tools loaded from {:?}", collect_tool_roots(paths));

        // Load workflows
        {
            let mut we = self.workflow_engine.write().await;
            we.load_workflows_from(&paths.workflows_dir.to_string_lossy());
        }

        {
            let mut bridge = self.action_bridge.lock().unwrap();
            bridge.start();
        }

        self.publish_runtime_event(
            "initialize",
            json!({
                "primary_backend": self.provider_registry.read().await.primary_name(),
                "tool_roots": collect_tool_roots(paths),
            }),
        );

        true
    }

    /// Dynamically handle package manager events for plugins
    pub async fn handle_pkgmgr_event(&self, event_name: &str, pkgid: &str) {
        log::debug!("Handling pkgmgr event: {} for pkgid: {}", event_name, pkgid);

        let mut plugin_manager = crate::llm::plugin_manager::PluginManager::new();
        let loaded = if event_name == "install"
            || event_name == "recoverinstall"
            || event_name == "upgrade"
            || event_name == "recoverupgrade"
        {
            plugin_manager.load_plugin_from_pkg(Some(self.platform.package_manager.as_ref()), pkgid)
        } else {
            false
        };

        let unloaded = if event_name == "uninstall" || event_name == "recoveruninstall" {
            // Note: PluginManager removes from registry, but we do a full reload of backends anyway
            true
        } else {
            false
        };

        if loaded || unloaded {
            log::debug!("Triggering LLM backend reload due to plugin changes...");
            self.reload_backends().await;
        }

        // --- NEW: Handle Tool Extensibility Indexing via PkgMgr ---
        // If a package is installed/uninstalled, we re-evaluate if index.md and tools.md
        // need to be rebuilt. This removes the need for periodic filesystem polling.
        if loaded || unloaded {
            self.reload_tools().await;
            self.run_startup_indexing().await;
        }
    }

    /// Reload LLM backends dynamically.
    ///
    /// Shuts down the current registry, re-reads config, re-initializes all
    /// backends, and publishes a `reload_backends` event.
    pub async fn reload_backends(&self) {
        let paths = &self.platform.paths;
        let llm_config_path = paths.config_dir.join("llm_config.json");
        let config = LlmConfig::load(&llm_config_path.to_string_lossy());
        let llm_doc = crate::core::llm_config_store::load(&paths.config_dir)
            .unwrap_or_else(|_| crate::core::llm_config_store::default_document());
        self.reload_safety_guard();

        // Re-scan plugins
        let mut plugin_manager = crate::llm::plugin_manager::PluginManager::new();
        plugin_manager.scan_plugins(Some(self.platform.package_manager.as_ref()));

        let routing =
            crate::core::provider_selection::ProviderCompatibilityTranslator::translate(&llm_doc);

        let candidates = self.get_backend_candidates(&config, &plugin_manager);
        let mut instances: Vec<crate::core::provider_selection::ProviderInstance> = Vec::new();
        let mut failed_inits: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        for cand in &candidates {
            let merged_cfg = {
                let ks_guard = self.key_store.lock().unwrap_or_else(|e| e.into_inner());
                let base = config.backend_config(&cand.name);
                merge_backend_auth(base, &cand.name, &ks_guard)
            };

            if let Some(be) =
                Self::create_and_init_backend_static(&plugin_manager, &cand.name, merged_cfg)
            {
                let slot = if instances.is_empty() { "Primary" } else { "Fallback" };
                log::debug!(
                    "Reload: {} LLM backend '{}' initialized (priority {})",
                    slot,
                    cand.name,
                    cand.priority
                );
                instances.push(crate::core::provider_selection::ProviderInstance {
                    name: cand.name.clone(),
                    backend: be,
                    last_init_error: None,
                });
            } else {
                // Record the init failure so it is visible in provider status
                // output even though no live instance was created.
                failed_inits.insert(
                    cand.name.clone(),
                    "not configured or initialization failed".to_string(),
                );
            }
        }

        // Re-order to match configured routing preference.
        let ordered_names = routing.ordered_names();
        if routing.providers_array_present || !ordered_names.is_empty() {
            instances.sort_by_key(|inst| {
                ordered_names
                    .iter()
                    .position(|name| *name == inst.name.as_str())
                    .unwrap_or(usize::MAX)
            });
            // When the `providers` array key is present it is authoritative.
            // Drop only instances explicitly listed and disabled; plugin-
            // discovered backends absent from `providers[]` stay as fallbacks.
            if routing.providers_array_present {
                instances.retain(|inst| {
                    routing
                        .providers
                        .iter()
                        .find(|p| p.name == inst.name)
                        .map(|p| p.enabled)
                        .unwrap_or(true)
                });
                log::debug!(
                    "Reload: providers[] authoritative — retained {} enabled instance(s)",
                    instances.len()
                );
            }
        }

        if instances.is_empty() {
            log::warn!("Failed to initialize ANY backend during reload!");
        }

        // Atomically replace the old registry (old backends are dropped here).
        {
            let mut rg = self.provider_registry.write().await;
            rg.shutdown_all();
            *rg = crate::core::provider_selection::ProviderRegistry::new(routing, instances, failed_inits);
        }

        if let Ok(mut stored_config) = self.llm_config.lock() {
            *stored_config = config;
        }

        self.publish_runtime_event("reload_backends", self.get_llm_runtime());
    }

    /// Create and initialize an LLM backend by name using the provided merged config.
    ///
    /// The caller is responsible for merging the api_key from KeyStore into
    /// `merged_cfg` before calling this function.
    fn create_and_init_backend_static(
        plugin_manager: &crate::llm::plugin_manager::PluginManager,
        name: &str,
        merged_cfg: Value,
    ) -> Option<Box<dyn LlmBackend>> {
        let mut be = plugin_manager.create_backend(name)?;
        if be.initialize(&merged_cfg) {
            Some(be)
        } else {
            log::debug!(
                "Backend '{}' skipped: not configured or initialization failed",
                name
            );
            None
        }
    }

    /// Determine LLM backend candidates and their priorities.
    fn get_backend_candidates(
        &self,
        config: &LlmConfig,
        plugin_manager: &crate::llm::plugin_manager::PluginManager,
    ) -> Vec<BackendCandidate> {
        let ks_guard = self.key_store.lock().unwrap_or_else(|e| e.into_inner());
        build_backend_candidates(config, plugin_manager, &ks_guard)
    }

    fn is_backend_available(&self, name: &str) -> bool {
        let cb_guard = self.circuit_breakers.read().unwrap();
        if let Some(state) = cb_guard.get(name) {
            if state.consecutive_failures >= 2 {
                if let Some(last_fail) = state.last_failure_time {
                    if last_fail.elapsed().as_secs() < 60 {
                        return false;
                    }
                }
            }
        }
        true
    }

    fn record_success(&self, name: &str) {
        let mut cb_guard = self.circuit_breakers.write().unwrap();
        let state = cb_guard
            .entry(name.to_string())
            .or_insert(CircuitBreakerState {
                consecutive_failures: 0,
                last_failure_time: None,
            });
        state.consecutive_failures = 0;
        state.last_failure_time = None;
    }

    /// Reset all circuit breakers. Called at the start of each new session
    /// so that failures from a prior session do not block new requests.
    fn reset_circuit_breakers(&self) {
        let mut cb_guard = self.circuit_breakers.write().unwrap();
        cb_guard.clear();
    }

    fn record_failure(&self, name: &str) {
        let mut cb_guard = self.circuit_breakers.write().unwrap();
        let state = cb_guard
            .entry(name.to_string())
            .or_insert(CircuitBreakerState {
                consecutive_failures: 0,
                last_failure_time: None,
            });
        state.consecutive_failures += 1;
        state.last_failure_time = Some(std::time::Instant::now());
    }

    /// Execute a chat request using the provider registry.
    ///
    /// Iterates providers in the configured preference order, skipping any
    /// that are blocked by the circuit breaker.  Holds the registry read lock
    /// for the duration of each `chat()` call.
    async fn chat_with_fallback(
        &self,
        messages: &[LlmMessage],
        tools: &[crate::llm::backend::LlmToolDecl],
        on_chunk: Option<&(dyn Fn(&str) + Send + Sync)>,
        system_prompt: &str,
        max_tokens: Option<u32>,
    ) -> LlmResponse {
        use crate::core::provider_selection::{
            ProviderAttempt, ProviderAttemptResult, ProviderSelectionRecord, ProviderSelector,
        };

        let mut failure_summaries: Vec<String> = Vec::new();
        let mut attempted: Vec<ProviderAttempt> = Vec::new();

        // Snapshot the enabled, preference-ordered provider names through
        // ProviderSelector so selection policy stays centralized there.
        // Disabled providers are excluded here; the circuit-breaker check
        // inside the loop handles temporarily unavailable ones.
        let provider_names: Vec<String> = {
            let rg = self.provider_registry.read().await;
            ProviderSelector::ordered_enabled_names(&rg)
        };

        // Track a successful response to write the selection record after the loop.
        let mut success_result: Option<(LlmResponse, usize)> = None;

        'providers: for (idx, name) in provider_names.iter().enumerate() {
            let slot = if idx == 0 { "Primary" } else { "Fallback" };

            if !self.is_backend_available(name) {
                log::warn!("{} backend '{}' skipped due to Circuit Breaker", slot, name);
                failure_summaries.push(format!("{}: skipped by circuit breaker", name));
                attempted.push(ProviderAttempt {
                    provider: name.clone(),
                    result: ProviderAttemptResult::SkippedOpenCircuit,
                    detail: Some("circuit breaker open".to_string()),
                });
                continue;
            }

            if idx > 0 {
                log::debug!("Trying fallback backend '{}'", name);
            }

            let resp = {
                let rg = self.provider_registry.read().await;
                let Some(inst) = rg.instances().iter().find(|i| i.name == *name) else {
                    continue 'providers;
                };
                inst.backend
                    .chat(messages, tools, on_chunk, system_prompt, max_tokens)
                    .await
            };

            if resp.success {
                self.record_success(name);
                attempted.push(ProviderAttempt {
                    provider: name.clone(),
                    result: ProviderAttemptResult::Selected,
                    detail: None,
                });
                success_result = Some((resp, idx));
                break;
            }

            self.record_failure(name);
            log::warn!(
                "{} backend '{}' failed (HTTP {}): {}",
                slot,
                name,
                resp.http_status,
                resp.error_message
            );
            failure_summaries.push(format!(
                "{} (HTTP {}): {}",
                name, resp.http_status, resp.error_message
            ));
            attempted.push(ProviderAttempt {
                provider: name.clone(),
                result: ProviderAttemptResult::ExecutionFailed,
                detail: Some(resp.error_message.clone()),
            });
        }

        if let Some((resp, idx)) = success_result {
            let selected = &provider_names[idx];
            let record = ProviderSelectionRecord {
                selected_provider: selected.clone(),
                attempted_providers: attempted,
                reason: if idx == 0 {
                    "first ready provider in configured order".to_string()
                } else {
                    format!("fallback to provider at position {}", idx)
                },
                selected_at_unix_secs: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
            };
            if let Ok(mut rg_write) = self.provider_registry.try_write() {
                rg_write.set_active_selection(record);
            }
            return resp;
        }

        // Record the all-failure state so admin/runtime status reflects the
        // most recent attempt rather than keeping the last successful selection.
        let failure_reason = if failure_summaries.is_empty() {
            "all providers skipped (no eligible provider available)".to_string()
        } else {
            format!(
                "all providers failed: {}",
                failure_summaries.join(" | ")
            )
        };
        let failure_record = ProviderSelectionRecord {
            selected_provider: String::new(),
            attempted_providers: attempted,
            reason: failure_reason.clone(),
            selected_at_unix_secs: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        if let Ok(mut rg_write) = self.provider_registry.try_write() {
            rg_write.set_active_selection(failure_record);
        }

        LlmResponse {
            error_message: failure_reason,
            ..Default::default()
        }
    }

    async fn rewrite_humanized_text_with_backend(
        &self,
        skill_content: Option<&str>,
        source_text: &str,
    ) -> Option<String> {
        let mut system_prompt = "You rewrite stiff AI-generated prose into a natural human-written voice. Preserve the original meaning, concrete examples, and overall heading structure. Remove robotic transitions, repetitive sentence openings, filler, and obvious AI stock phrases. Use contractions where natural, vary sentence rhythm, and prefer fresh, specific wording over generic corporate language. Keep roughly the same length, section count, and example coverage as the source instead of compressing it. If a heading sounds robotic or formulaic, keep the same structural role but rename it to something a human writer would naturally use. Output only the rewritten text with no commentary or markdown fences.".to_string();
        if let Some(skill_content) = skill_content {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(skill_content.trim());
        }
        let user_prompt = format!(
            "Rewrite the following source text so it sounds natural and human-written while keeping the same meaning.\n\n{}",
            source_text.trim()
        );
        let response = self
            .chat_with_fallback(
                &sanitize_messages_for_transport(vec![LlmMessage::user(&user_prompt)]),
                &[],
                None,
                &system_prompt,
                Some(2600),
            )
            .await;
        if !response.success {
            return None;
        }
        let rewritten = strip_wrapping_markdown_fence(&response.text);
        let normalized = rewritten
            .trim()
            .replace("## In Conclusion", "## Wrapping Up")
            .replace("## Conclusion", "## Wrapping Up");
        let normalized = normalized.trim();
        if normalized.is_empty() {
            return None;
        }
        Some(normalized.to_string())
    }

    async fn draft_longform_markdown_with_backend(&self, prompt: &str) -> Option<(String, String)> {
        if !prompt_allows_direct_longform_shortcut(prompt) {
            return None;
        }

        let target = longform_markdown_output_target(prompt)?;
        let topic = extract_longform_topic(prompt)?;
        let (min_words, max_words) = requested_word_budget_bounds(prompt).unwrap_or((420, 620));
        let target_words = ((min_words + max_words) / 2).clamp(min_words, max_words);
        let system_prompt = format!(
            "You write polished Markdown blog posts and articles in one pass. Output only the final Markdown document with no commentary or code fences. Use this structure exactly: H1 title, short introduction, 3 to 4 H2 sections with concrete examples or practical implications, then an H2 Conclusion. Keep the voice confident and readable, avoid filler, and keep the final draft between {} and {} words.",
            min_words, max_words
        );
        let user_prompt = format!(
            "Write a complete Markdown article about {}. Aim for about {} words while staying within {} to {} words. Include at least one concrete scenario, workflow, or before/after example, and make the conclusion explicit.",
            topic, target_words, min_words, max_words
        );
        let response = self
            .chat_with_fallback(
                &sanitize_messages_for_transport(vec![LlmMessage::user(&user_prompt)]),
                &[],
                None,
                &system_prompt,
                Some(2600),
            )
            .await;
        if !response.success {
            return None;
        }

        let rendered = strip_wrapping_markdown_fence(&response.text).trim().to_string();
        if rendered.is_empty()
            || output_lacks_longform_markdown_structure(prompt, &rendered)
            || output_violates_requested_word_budget(prompt, &rendered)
        {
            return None;
        }

        Some((target, rendered))
    }

    #[allow(dead_code)]
    async fn try_prediction_market_briefing_shortcut(
        &self,
        session_id: &str,
        prompt: &str,
        session_workdir: &Path,
        snapshot_content: &str,
        _on_chunk: Option<&(dyn Fn(&str) + Send + Sync)>,
    ) -> Option<String> {
        let primary_candidates = basic_polymarket_briefing_candidates(snapshot_content, 3);
        let mut candidate_markets = primary_candidates.clone();
        if candidate_markets.len() < 3 {
            let mut seen_topics = candidate_markets
                .iter()
                .map(polymarket_market_topic_key)
                .collect::<HashSet<_>>();
            for entry in basic_polymarket_briefing_candidates(snapshot_content, 8) {
                if seen_topics.insert(polymarket_market_topic_key(&entry)) {
                    candidate_markets.push(entry);
                }
                if candidate_markets.len() >= 8 {
                    break;
                }
            }
        }
        if candidate_markets.len() < 3 {
            let mut seen_topics = candidate_markets
                .iter()
                .map(polymarket_market_topic_key)
                .collect::<HashSet<_>>();
            for entry in top_polymarket_briefing_entries(snapshot_content, 6) {
                if seen_topics.insert(polymarket_market_topic_key(&entry)) {
                    candidate_markets.push(entry);
                }
                if candidate_markets.len() >= 6 {
                    break;
                }
            }
        }
        if candidate_markets.len() < 3 {
            return None;
        }

        let search_budget = std::time::Duration::from_secs(40);
        let direct_search_timeout = std::time::Duration::from_secs(4);
        let started_at = std::time::Instant::now();
        let mut scored_sections = Vec::new();
        for (candidate_index, entry) in candidate_markets.iter().enumerate() {
            if started_at.elapsed() >= search_budget {
                break;
            }
            let question = entry.get("question").and_then(Value::as_str)?.trim();
            let description = entry
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let (yes_pct, no_pct) = polymarket_yes_no_percentages(entry)?;

            let mut best_news_summary = None;
            for (attempt, query) in prediction_market_direct_news_queries(question, description)
                .into_iter()
                .take(if candidate_index < primary_candidates.len() { 3 } else { 2 })
                .enumerate()
            {
                if started_at.elapsed() >= search_budget {
                    break;
                }
                let search_result = match tokio::time::timeout(
                    direct_search_timeout,
                    crate::core::feature_tools::web_search(
                        &query,
                        Some("duckduckgo_mirror"),
                        5,
                        session_workdir,
                        &self.platform.paths.config_dir,
                    ),
                )
                .await
                {
                    Ok(result) => result,
                    Err(_) => continue,
                };

                if let Ok(ss) = self.session_store.lock() {
                    if let Some(store) = ss.as_ref() {
                        let search_call_id = format!(
                            "auto_search_polymarket_news_direct_{}_{}",
                            candidate_index + 1,
                            attempt + 1
                        );
                        record_synthetic_tool_interaction(
                            store,
                            session_id,
                            &search_call_id,
                            "web_search",
                            "web_search",
                            json!({
                                "query": query,
                                "limit": 5,
                                "engine": "duckduckgo_mirror",
                            }),
                            &search_result,
                        );
                    }
                }

                best_news_summary = search_result
                    .get("result")
                    .and_then(|value| value.get("results"))
                    .and_then(Value::as_array)
                    .and_then(|results| select_best_recent_news_summary(question, description, results));
                if best_news_summary.is_some() {
                    break;
                }
            }

            if best_news_summary.is_none() {
                for (attempt, query) in prediction_market_news_queries(question, description)
                    .into_iter()
                    .take(if candidate_index < primary_candidates.len() { 3 } else { 2 })
                    .enumerate()
                {
                    if started_at.elapsed() >= search_budget {
                        break;
                    }
                    let rss_results = fetch_recent_news_rss_results(&query).await.unwrap_or_default();
                    let search_result = synthetic_web_search_result(&rss_results);

                    if let Ok(ss) = self.session_store.lock() {
                        if let Some(store) = ss.as_ref() {
                            let search_call_id = format!(
                            "auto_search_polymarket_news_{}_{}",
                            candidate_index + 1,
                            attempt + 1
                        );
                            record_synthetic_tool_interaction(
                                store,
                                session_id,
                                &search_call_id,
                            "web_search",
                            "web_search",
                            json!({
                                "query": query,
                                "limit": 8,
                                "source": "google_news_rss",
                            }),
                            &search_result,
                        );
                        }
                    }

                    best_news_summary = search_result
                        .get("result")
                        .and_then(|value| value.get("results"))
                        .and_then(Value::as_array)
                        .and_then(|results| select_best_recent_news_summary(question, description, results));
                    if best_news_summary.is_some() {
                        break;
                    }
                }
            }

            let Some((news_score, news_summary)) = best_news_summary else {
                continue;
            };
            let combined_score =
                polymarket_market_candidate_score(entry) + news_score.saturating_mul(3);
            scored_sections.push((
                combined_score,
                question.to_string(),
                yes_pct,
                no_pct,
                news_summary,
            ));
        }

        if scored_sections.len() < 3 {
            return None;
        }

        scored_sections.sort_by(|left, right| right.0.cmp(&left.0));
        scored_sections.truncate(3);
        let final_sections = scored_sections
            .into_iter()
            .enumerate()
            .map(|(index, (_, question, yes_pct, no_pct, news_summary))| {
                format!(
                    "## {}. {}\n**Current odds:** Yes {}% / No {}%\n**Related news:** {}",
                    index + 1,
                    question,
                    yes_pct,
                    no_pct,
                    format_prediction_market_related_news(yes_pct, no_pct, &news_summary)
                )
            })
            .collect::<Vec<_>>();

        let deterministic_body = format!(
            "# Polymarket Briefing — {}\n\n{}\n",
            format_unix_timestamp_utc(unix_timestamp_secs())
                .get(..10)
                .unwrap_or("today"),
            final_sections.join("\n\n")
        );
        let final_body = deterministic_body;

        let briefing_path = session_workdir.join("polymarket_briefing.md");
        if std::fs::write(&briefing_path, final_body.as_bytes()).is_err() {
            return None;
        }
        if output_lacks_prediction_market_briefing(prompt, &final_body) {
            return None;
        }

        if let Ok(ss) = self.session_store.lock() {
            if let Some(store) = ss.as_ref() {
                record_synthetic_tool_interaction(
                    store,
                    session_id,
                    "auto_write_polymarket_briefing",
                    "file_write",
                    "file_write",
                    json!({
                        "path": "polymarket_briefing.md",
                        "content": final_body,
                    }),
                    &json!({
                        "success": true,
                        "path": briefing_path.to_string_lossy().to_string(),
                        "bytes_written": final_body.len(),
                    }),
                );
                if let Some(preview) = completion_preview_payload_for_file_target(
                    session_workdir,
                    "polymarket_briefing.md",
                ) {
                    record_synthetic_tool_interaction(
                        store,
                        session_id,
                        "auto_preview_polymarket_briefing",
                        "read_file",
                        "read_file",
                        json!({
                            "path": "polymarket_briefing.md",
                            "mode": "completion_preview",
                        }),
                        &preview,
                    );
                }
            }
        }

        Some(completion_message_for_prompt_file_targets(
            prompt,
            session_workdir,
            &["polymarket_briefing.md".to_string()],
        ))
    }

    /// Extract intent keywords for dynamic tool filtering.
    fn extract_intent_keywords(prompt: &str) -> Vec<String> {
        let p = prompt.to_lowercase();
        let mut keywords = Vec::new();

        if p.contains("file") || p.contains("read") || p.contains("cat") {
            keywords.extend(["fs", "file", "read", "write", "content"].map(String::from));
        }
        if p.contains("install") || p.contains("package") || p.contains("app") || p.contains("run")
        {
            keywords.extend(["pkg", "app", "install", "exec", "shell", "run"].map(String::from));
        }
        if p.contains("remember")
            || p.contains("memory")
            || p.contains("search")
            || p.contains("knowledge")
            || p.contains("recall")
        {
            keywords.extend(
                ["mem", "remember", "forget", "recall", "search", "know"].map(String::from),
            );
        }
        if p.contains("task") || p.contains("schedule") || p.contains("alarm") || p.contains("time")
        {
            keywords.extend(["task", "sched", "alarm", "time", "date"].map(String::from));
        }
        if p.contains("system")
            || p.contains("info")
            || p.contains("status")
            || p.contains("battery")
            || p.contains("wifi")
        {
            keywords.extend(
                [
                    "sys", "info", "status", "battery", "wifi", "network", "device",
                ]
                .map(String::from),
            );
        }
        if p.contains("help") || p.contains("list") {
            keywords.extend(["ALL"].map(String::from));
        }

        keywords
    }

    fn is_web_dashboard_app_request(session_id: &str, prompt: &str) -> bool {
        if !session_id.starts_with("web_") {
            return false;
        }

        let p = prompt.to_lowercase();
        let asks_to_create = [
            "create", "make", "build", "generate", "write", "update", "improve", "enhance",
            "modify", "edit",
        ]
        .iter()
        .any(|needle| p.contains(needle));
        let mentions_browser_ui = [
            "web app",
            "browser app",
            "browser",
            "html",
            "css",
            "javascript",
            "js",
            "webview",
            "dashboard",
            "ui",
            "interface",
            "screen",
            "panel",
            "visualization",
            "chart",
            "monitor",
            "tetris",
            "game",
            "page",
        ]
        .iter()
        .any(|needle| p.contains(needle));

        asks_to_create && mentions_browser_ui
    }
}
