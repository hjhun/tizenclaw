impl AgentCore {
    pub async fn shutdown(&self) {
        log::info!("AgentCore shutting down");
        self.event_bus.stop();
        self.provider_registry.write().await.shutdown_all();
    }

    pub fn get_session_store(&self) -> Option<SessionStoreRef<'_>> {
        let guard = self.session_store.lock().ok()?;
        if guard.is_some() {
            Some(SessionStoreRef { guard })
        } else {
            None
        }
    }

    fn llm_config_path_affects_backends(path: &str) -> bool {
        matches!(
            path.split('.').next(),
            Some("active_backend")
                | Some("fallback_backends")
                | Some("backends")
                | Some("providers")
        )
    }

    pub fn get_llm_config(&self, path: Option<&str>) -> Result<Value, String> {
        let doc = llm_config_store::load(&self.platform.paths.config_dir)?;
        llm_config_store::get_value(&doc, path)
    }

    pub fn resolve_backend_api_key(&self, backend_name: &str) -> Option<String> {
        let guard = self.key_store.lock().unwrap_or_else(|err| err.into_inner());
        guard.get(backend_name)
    }

    pub fn list_keys(&self) -> Result<Value, String> {
        let guard = self.key_store.lock().unwrap_or_else(|err| err.into_inner());
        Ok(json!({
            "stored": guard.list_stored(),
            "from_env": guard.list_from_env(),
        }))
    }

    pub async fn set_key(&self, key: &str, value: &str) -> Result<Value, String> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err("Key value must not be empty".into());
        }

        {
            let guard = self.key_store.lock().unwrap_or_else(|err| err.into_inner());
            guard.set(key, trimmed)?;
        }

        self.reload_backends().await;
        Ok(json!({
            "key": key,
            "stored": true,
        }))
    }

    pub async fn delete_key(&self, key: &str) -> Result<Value, String> {
        {
            let guard = self.key_store.lock().unwrap_or_else(|err| err.into_inner());
            guard.delete(key)?;
        }

        self.reload_backends().await;
        Ok(json!({
            "key": key,
            "deleted": true,
        }))
    }

    pub async fn test_key(&self, key: &str) -> Result<Value, String> {
        let api_key = self
            .resolve_backend_api_key(key)
            .ok_or_else(|| format!("No API key configured for '{}'", key))?;

        self.ping_backend_key(key, &api_key).await
    }

    pub async fn set_llm_config(&self, path: &str, value: Value) -> Result<Value, String> {
        let mut doc = llm_config_store::load(&self.platform.paths.config_dir)?;
        if let Ok(existing) = llm_config_store::get_value(&doc, Some(path)) {
            if existing == value {
                return Ok(existing);
            }
        }
        llm_config_store::set_value(&mut doc, path, value)?;
        llm_config_store::save(&self.platform.paths.config_dir, &doc)?;

        if Self::llm_config_path_affects_backends(path) {
            self.reload_backends().await;
        }

        llm_config_store::get_value(&doc, Some(path))
    }

    pub async fn unset_llm_config(&self, path: &str) -> Result<Value, String> {
        let mut doc = llm_config_store::load(&self.platform.paths.config_dir)?;
        if llm_config_store::get_value(&doc, Some(path)).is_err() {
            return Ok(Value::Null);
        }
        let removed = llm_config_store::unset_value(&mut doc, path)?;
        llm_config_store::save(&self.platform.paths.config_dir, &doc)?;

        if Self::llm_config_path_affects_backends(path) {
            self.reload_backends().await;
        }

        Ok(removed)
    }

    pub async fn reload_llm_backends(&self) -> Result<Value, String> {
        self.reload_backends().await;
        self.get_llm_config(None)
    }

    async fn ping_backend_key(&self, backend_name: &str, api_key: &str) -> Result<Value, String> {
        let doc = llm_config_store::load(&self.platform.paths.config_dir)
            .unwrap_or_else(|_| llm_config_store::default_document());
        let null_value = Value::Null;
        let backend_cfg = doc
            .get("backends")
            .and_then(|value| value.get(backend_name))
            .unwrap_or(&null_value);

        let (url, headers) = match backend_name {
            "anthropic" => {
                let endpoint = config_string(backend_cfg, &["endpoint"])
                    .unwrap_or("https://api.anthropic.com/v1")
                    .trim_end_matches('/')
                    .to_string();
                (
                    format!("{}/models", endpoint),
                    vec![
                        ("x-api-key".to_string(), api_key.to_string()),
                        (
                            "anthropic-version".to_string(),
                            "2023-06-01".to_string(),
                        ),
                    ],
                )
            }
            "openai" => {
                let endpoint = config_string(backend_cfg, &["endpoint"])
                    .unwrap_or("https://api.openai.com/v1")
                    .trim_end_matches('/')
                    .to_string();
                (
                    format!("{}/models", endpoint),
                    vec![(
                        "Authorization".to_string(),
                        format!("Bearer {}", api_key),
                    )],
                )
            }
            "gemini" => {
                let endpoint = config_string(backend_cfg, &["endpoint"])
                    .unwrap_or("https://generativelanguage.googleapis.com/v1beta")
                    .trim_end_matches('/')
                    .to_string();
                (format!("{}/models?key={}", endpoint, api_key), Vec::new())
            }
            "groq" => (
                "https://api.groq.com/openai/v1/models".to_string(),
                vec![(
                    "Authorization".to_string(),
                    format!("Bearer {}", api_key),
                )],
            ),
            _ => return Err(format!("Backend '{}' does not support key.test", backend_name)),
        };

        let owned_headers = headers;
        let borrowed_headers = owned_headers
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect::<Vec<_>>();
        let response = crate::infra::http_client::http_get(&url, &borrowed_headers, 0, 20).await;
        if response.success && (200..300).contains(&response.status_code) {
            return Ok(json!({
                "key": backend_name,
                "reachable": true,
                "status_code": response.status_code,
            }));
        }

        let summary = Self::summarize_backend_test_error(&response);
        Err(format!(
            "Backend '{}' test failed with status {}: {}",
            backend_name, response.status_code, summary
        ))
    }

    fn summarize_backend_test_error(response: &crate::infra::http_client::HttpResponse) -> String {
        if !response.error.trim().is_empty() {
            return response.error.trim().to_string();
        }

        if let Ok(value) = serde_json::from_str::<Value>(&response.body) {
            if let Some(message) = value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .or_else(|| value.get("message").and_then(Value::as_str))
            {
                let trimmed = message.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }

        let trimmed = response.body.trim();
        if trimmed.is_empty() {
            "request failed".to_string()
        } else {
            utf8_safe_preview(trimmed, 160).to_string()
        }
    }

    pub fn get_llm_runtime(&self) -> Value {
        // Use a blocking try to avoid deadlock when called from a sync context
        // that may already hold a registry read lock.  Falls back to reading
        // the stored `llm_config` for basic compatibility fields.
        if let Ok(rg) = self.provider_registry.try_read() {
            return rg.status_json(|name| self.is_backend_available(name));
        }
        // Fallback path: registry is write-locked (reload in progress).
        // Derive the routing config from the stored raw document so that the
        // authoritative `providers[]` array (when present) is reflected in the
        // status output, not just the legacy `active_backend`/`fallback_backends`
        // fields that `LlmConfig` also exposes.
        let raw_doc = match self.llm_config.lock() {
            Ok(config) => config.raw_doc.clone(),
            Err(err) => err.into_inner().raw_doc.clone(),
        };
        let routing =
            crate::core::provider_selection::ProviderCompatibilityTranslator::translate(&raw_doc);
        let configured_active_backend = routing.raw_active_backend.clone();
        let configured_fallback_backends = routing.raw_fallback_backends.clone();
        let configured_provider_order: Vec<String> =
            routing.ordered_names().into_iter().map(String::from).collect();
        // Build the providers array from routing config so per-provider
        // visibility is preserved during reload.  Availability and init-error
        // are unknown because instances are not accessible while the registry
        // is write-locked.
        let providers: Vec<Value> = routing
            .providers
            .iter()
            .map(|pref| {
                json!({
                    "name": pref.name,
                    "priority": pref.priority,
                    "enabled": pref.enabled,
                    "availability": "unknown",
                    "last_init_error": Value::Null,
                    "source": pref.source.as_str(),
                })
            })
            .collect();
        json!({
            "configured_active_backend": configured_active_backend,
            "configured_fallback_backends": configured_fallback_backends,
            "configured_provider_order": configured_provider_order,
            "providers": providers,
            "current_selection": Value::Null,
        })
    }

    pub fn safety_guard_status(&self) -> Value {
        self.safety_guard
            .lock()
            .map(|guard| guard.status_json())
            .unwrap_or_else(|_| {
                json!({
                    "blocked_tools": [],
                    "allow_irreversible": false,
                    "max_tool_calls_per_session": 0,
                    "status": "unavailable",
                })
            })
    }

    pub fn tool_policy_status(&self) -> Value {
        self.tool_policy
            .lock()
            .map(|policy| policy.status_json())
            .unwrap_or_else(|_| {
                json!({
                    "max_repeat_count": 0,
                    "max_iterations": 0,
                    "current_iteration_count": 0,
                    "status": "unavailable",
                })
            })
    }

    pub fn clear_agent_data(
        &self,
        include_memory: bool,
        include_sessions: bool,
    ) -> Result<Value, String> {
        if !include_memory && !include_sessions {
            return Err("Nothing selected to clear".to_string());
        }

        let mut result = json!({
            "status": "success",
            "cleared": {
                "memory": false,
                "sessions": false
            }
        });

        if include_memory {
            let deleted_rows = self
                .memory_store
                .lock()
                .map_err(|_| "MemoryStore lock failed".to_string())?
                .as_ref()
                .cloned()
                .ok_or_else(|| "MemoryStore not initialized".to_string())?
                .clear_all()?;
            result["cleared"]["memory"] = Value::Bool(true);
            result["memory"] = json!({
                "records_deleted": deleted_rows
            });
        }

        if include_sessions {
            let cleanup = self
                .session_store
                .lock()
                .map_err(|_| "SessionStore lock failed".to_string())?
                .as_ref()
                .cloned()
                .ok_or_else(|| "SessionStore not initialized".to_string())?
                .clear_all()?;
            result["cleared"]["sessions"] = Value::Bool(true);
            result["sessions"] = cleanup;
        }

        if let Ok(policy) = self.tool_policy.lock() {
            policy.reset_session("default");
        }
        self.reset_circuit_breakers();

        Ok(result)
    }

    pub fn list_registered_paths(&self) -> RegisteredPaths {
        RegisteredPaths::load(&self.platform.paths.config_dir)
    }

    fn runtime_topology(&self) -> crate::core::runtime_paths::RuntimeTopology {
        crate::core::runtime_paths::RuntimeTopology::from_data_dir(
            self.platform.paths.data_dir.clone(),
        )
    }

    pub fn runtime_topology_summary(&self) -> Value {
        self.runtime_topology().summary_json()
    }

    fn persist_loop_snapshot(&self, state: &AgentLoopState) {
        let topology = self.runtime_topology();
        if let Err(err) = std::fs::create_dir_all(&topology.loop_state_dir) {
            log::warn!(
                "[AgentLoop] Failed to create loop state dir '{}': {}",
                topology.loop_state_dir.display(),
                err
            );
            return;
        }

        let snapshot = state.snapshot();
        let payload = json!({
            "session_id": snapshot.session_id,
            "phase": snapshot.phase,
            "original_goal": snapshot.original_goal,
            "plan_step_count": snapshot.plan_step_count,
            "current_step": snapshot.current_step,
            "round": snapshot.round,
            "error_count": snapshot.error_count,
            "tool_retry_count": snapshot.tool_retry_count,
            "max_tool_rounds": snapshot.max_tool_rounds,
            "last_eval_verdict": snapshot.last_eval_verdict,
            "needs_follow_up": snapshot.needs_follow_up,
            "last_transition_reason": snapshot.last_transition_reason,
            "last_transition_detail": snapshot.last_transition_detail,
            "last_error": snapshot.last_error,
            "total_tool_calls": snapshot.total_tool_calls,
            "stuck_retry_count": snapshot.stuck_retry_count,
            "tool_budget_events": snapshot.tool_budget_events,
            "active_workflow_id": snapshot.active_workflow_id,
            "current_workflow_step": snapshot.current_workflow_step,
            "updated_at_unix_secs": unix_timestamp_secs(),
            "resumable": state.phase != AgentPhase::Complete,
        });

        let path = topology.loop_state_path(&state.session_id);
        let tmp_path = path.with_extension("json.tmp");
        let serialized = match serde_json::to_vec_pretty(&payload) {
            Ok(serialized) => serialized,
            Err(err) => {
                log::warn!(
                    "[AgentLoop] Failed to serialize loop snapshot for session '{}': {}",
                    state.session_id,
                    err
                );
                return;
            }
        };

        if let Err(err) =
            std::fs::write(&tmp_path, serialized).and_then(|_| std::fs::rename(&tmp_path, &path))
        {
            log::warn!(
                "[AgentLoop] Failed to persist loop snapshot '{}': {}",
                path.display(),
                err
            );
            let _ = std::fs::remove_file(&tmp_path);
            return;
        }

        log::debug!(
            "[AgentLoop] Snapshot saved: session='{}' phase={} path='{}'",
            state.session_id,
            state.phase.as_str(),
            path.display()
        );
    }

    fn load_loop_snapshot(&self, session_id: &str) -> Value {
        let path = self.runtime_topology().loop_state_path(session_id);
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str::<Value>(&content).ok())
            .unwrap_or_else(|| {
                json!({
                    "session_id": session_id,
                    "phase": AgentPhase::GoalParsing.as_str(),
                    "original_goal": "",
                    "plan_step_count": 0,
                    "current_step": 0,
                    "round": 0,
                    "error_count": 0,
                    "tool_retry_count": 0,
                    "max_tool_rounds": AgentLoopState::DEFAULT_MAX_TOOL_ROUNDS,
                    "last_eval_verdict": EvalVerdict::NotStarted.as_str(),
                    "needs_follow_up": false,
                    "last_transition_reason": LoopTransitionReason::LoopInitialized.as_str(),
                    "last_transition_detail": "",
                    "last_error": Value::Null,
                    "total_tool_calls": 0,
                    "stuck_retry_count": 0,
                    "tool_budget_events": 0,
                    "active_workflow_id": Value::Null,
                    "current_workflow_step": 0,
                    "updated_at_unix_secs": unix_timestamp_secs(),
                    "resumable": false,
                })
            })
    }

    pub fn session_runtime_status(&self, session_id: &str) -> Value {
        let registrations = self.list_registered_paths();
        let skill_capabilities =
            skill_capability_manager::load_snapshot(&self.platform.paths, &registrations);
        let tool_audit = self
            .tool_dispatcher
            .try_read()
            .map(|dispatcher| dispatcher.audit_summary())
            .unwrap_or_else(|_| {
                json!({
                    "status": "busy",
                    "reason": "tool dispatcher is currently being updated",
                })
            });
        let session = self
            .session_store
            .lock()
            .ok()
            .and_then(|guard| {
                guard
                    .as_ref()
                    .map(|store| store.session_runtime_summary(session_id))
            })
            .unwrap_or_else(|| {
                let topology = self.runtime_topology();
                let session_dir = topology.sessions_dir.join(session_id);
                json!({
                    "session_dir": session_dir,
                    "today_path": session_dir.join("today.md"),
                    "compacted_path": session_dir.join("compacted.md"),
                    "compacted_structured_path": session_dir.join("compacted.jsonl"),
                    "transcript_path": session_dir.join("transcript.jsonl"),
                    "workdir_path": topology.data_dir.join("workdirs").join(session_id),
                    "session_exists": false,
                    "message_file_count": 0,
                    "compacted_snapshot_exists": false,
                    "structured_compaction_exists": false,
                    "transcript_exists": false,
                    "transcript_message_count": 0,
                    "assistant_message_count": 0,
                    "tool_result_count": 0,
                    "resume_ready": false,
                })
            });
        let memory = self
            .memory_store
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|store| store.runtime_summary()))
            .unwrap_or_else(|| {
                let topology = self.runtime_topology();
                let summary_path = topology.memory_dir.join("memory.md");
                json!({
                    "base_dir": topology.memory_dir,
                    "summary_path": summary_path,
                    "short_term_dir": topology.memory_dir.join("short-term"),
                    "long_term_dir": topology.memory_dir.join("long-term"),
                    "episodic_dir": topology.memory_dir.join("episodic"),
                    "summary_exists": false,
                    "prompt_ready": false,
                    "embedding_available": false,
                    "total_entries": 0,
                    "categories": {
                        "general": 0,
                        "facts": 0,
                        "preferences": 0,
                        "episodic": 0,
                    }
                })
            });
        let message_file_count = session
            .get("message_file_count")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        let resume_ready = session
            .get("resume_ready")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let memory_prompt_ready = memory
            .get("prompt_ready")
            .and_then(|value| value.as_bool())
            .unwrap_or(false);
        let memory_total_entries = memory
            .get("total_entries")
            .and_then(|value| value.as_i64())
            .unwrap_or(0);

        json!({
            "status": "ok",
            "session_id": session_id,
            "control_plane": {
                "idle_window": AgentLoopState::IDLE_WINDOW,
                "default_max_tool_rounds": AgentLoopState::DEFAULT_MAX_TOOL_ROUNDS,
                "default_token_budget": AgentLoopState::DEFAULT_TOKEN_BUDGET,
                "default_compact_threshold": AgentLoopState::DEFAULT_COMPACT_THRESHOLD,
            },
            "runtime_topology": self.runtime_topology_summary(),
            "execution": runtime_capabilities::summarize(&self.platform.paths, &registrations),
            "tool_audit": tool_audit.clone(),
            "skills": skill_capabilities.summary_json(),
            "session": session,
            "memory": memory,
            "context_flow": {
                "session_resume_ready": resume_ready,
                "history_message_files": message_file_count,
                "memory_prompt_ready": memory_prompt_ready,
                "memory_total_entries": memory_total_entries,
            },
            "loop_snapshot": self.load_loop_snapshot(session_id),
        })
    }

    pub fn tool_audit_status(&self) -> Value {
        let tool_audit = self
            .tool_dispatcher
            .try_read()
            .map(|dispatcher| dispatcher.audit_summary())
            .unwrap_or_else(|_| {
                json!({
                    "status": "busy",
                    "reason": "tool dispatcher is currently being updated",
                })
            });
        json!({
            "status": "ok",
            "tools": tool_audit,
        })
    }

    pub fn skill_capability_status(&self) -> Value {
        let registrations = self.list_registered_paths();
        let snapshot =
            skill_capability_manager::load_snapshot(&self.platform.paths, &registrations);
        json!({
            "status": "ok",
            "skills": snapshot.summary_json(),
        })
    }

    pub async fn clawhub_search(&self, query: &str) -> Value {
        match crate::core::clawhub_client::clawhub_search(query).await {
            Ok(result) => json!({ "status": "ok", "result": result }),
            Err(err) => json!({ "status": "error", "error": err }),
        }
    }

    pub async fn clawhub_install(&self, source: &str) -> Value {
        let skill_hubs_dir =
            crate::core::clawhub_client::skill_hubs_dir_from_paths(&self.platform.paths);
        match crate::core::clawhub_client::clawhub_install(&skill_hubs_dir, source).await {
            Ok(result) => {
                // A new skill directory was written; drop the cached snapshot so the
                // next consumer sees the updated skill roots immediately rather than
                // waiting for the second-resolution mtime to roll over.
                skill_capability_manager::invalidate_snapshot_cache(
                    skill_capability_manager::SkillSnapshotInvalidationReason::ClawHubInstall,
                );
                json!({ "status": "ok", "result": result })
            }
            Err(err) => json!({ "status": "error", "error": err }),
        }
    }

    pub fn clawhub_list(&self) -> Value {
        let skill_hubs_dir =
            crate::core::clawhub_client::skill_hubs_dir_from_paths(&self.platform.paths);
        match crate::core::clawhub_client::clawhub_list(&skill_hubs_dir) {
            Ok(result) => json!({ "status": "ok", "result": result }),
            Err(err) => json!({ "status": "error", "error": err }),
        }
    }

    pub async fn clawhub_update(&self) -> Value {
        let skill_hubs_dir =
            crate::core::clawhub_client::skill_hubs_dir_from_paths(&self.platform.paths);
        match crate::core::clawhub_client::clawhub_update(&skill_hubs_dir).await {
            Ok(result) => {
                // Invalidate unconditionally: even a partial success writes skill
                // directories, and the mtime-based fingerprint cannot distinguish
                // a same-second write from a no-change read.
                skill_capability_manager::invalidate_snapshot_cache(
                    skill_capability_manager::SkillSnapshotInvalidationReason::ClawHubUpdate,
                );
                json!({ "status": "ok", "result": result })
            }
            Err(err) => json!({ "status": "error", "error": err }),
        }
    }

    pub async fn register_external_path(
        &self,
        kind: RegistrationKind,
        raw_path: &str,
    ) -> Result<RegisteredPaths, String> {
        let (registrations, _) =
            registration_store::register_path(&self.platform.paths.config_dir, kind, raw_path)?;
        self.reload_tools().await;
        self.run_startup_indexing().await;
        self.publish_runtime_event(
            "register_external_path",
            json!({"kind": kind.as_str(), "path": raw_path}),
        );
        Ok(registrations)
    }

    pub async fn unregister_external_path(
        &self,
        kind: RegistrationKind,
        raw_path: &str,
    ) -> Result<(RegisteredPaths, bool), String> {
        let (registrations, removed, _) =
            registration_store::unregister_path(&self.platform.paths.config_dir, kind, raw_path)?;
        self.reload_tools().await;
        self.run_startup_indexing().await;
        self.publish_runtime_event(
            "unregister_external_path",
            json!({"kind": kind.as_str(), "path": raw_path, "removed": removed}),
        );
        Ok((registrations, removed))
    }

    pub async fn reload_tools(&self) {
        {
            let mut td = self.tool_dispatcher.write().await;
            *td = ToolDispatcher::new();
            let tool_roots = collect_tool_roots(&self.platform.paths);
            td.load_tools_from_paths(tool_roots.iter().map(|root| root.as_str()));
            log::info!(
                "Tools reloaded: {} declarations from {:?}",
                td.get_tool_declarations().len(),
                tool_roots
            );
        }
        self.publish_runtime_event(
            "reload_tools",
            json!({"tool_roots": collect_tool_roots(&self.platform.paths)}),
        );
    }

    pub async fn run_startup_indexing(&self) {
        use crate::core::tool_indexer;

        self.reload_tools().await;
        let root_dir = self.platform.paths.tools_dir.to_string_lossy().to_string();
        // Embedded descriptors are documentation/indexing metadata for
        // code-defined built-in tools. They are not the execution source.
        let embedded_dir = self
            .platform
            .paths
            .embedded_tools_dir
            .to_string_lossy()
            .to_string();
        let scan_roots = [root_dir.as_str(), embedded_dir.as_str()];
        let skill_roots = collect_skill_roots(&self.platform.paths);
        let skill_count = crate::core::textual_skill_scanner::scan_textual_skills_from_roots(
            &skill_roots.iter().map(|root| root.as_str()).collect::<Vec<_>>(),
        )
        .len();
        let tool_count = self
            .tool_dispatcher
            .read()
            .await
            .get_tool_declarations()
            .len();
        log::info!(
            "[Startup Indexing] Registered {} tools and {} skills from runtime paths.",
            tool_count,
            skill_count
        );

        // Phase 1: Hash-based change detection (fast, no I/O beyond stat)
        if !tool_indexer::needs_reindex_for_roots(&root_dir, &scan_roots) {
            log::info!("[Startup Indexing] No changes detected (hash match). Skipping.");
            return;
        }

        // Phase 2: Local filesystem scan — collect all tool metadata
        log::info!(
            "[Startup Indexing] Scanning tool metadata from {} and {}...",
            root_dir,
            embedded_dir
        );
        let metadata =
            tool_indexer::scan_tools_metadata_with_embedded(&root_dir, Some(&embedded_dir));

        if metadata.total_tools() == 0 {
            log::info!("[Startup Indexing] No tools found. Skipping index generation.");
            return;
        }

        log::info!(
            "[Startup Indexing] Found {} tools across {} categories.",
            metadata.total_tools(),
            metadata.categories.len(),
        );

        // Phase 3: LLM-assisted markdown generation (single call)
        let has_any_backend = self.provider_registry.read().await.has_any();

        if has_any_backend {
            log::info!("[Startup Indexing] Generating documentation via LLM...");
            let prompt = tool_indexer::build_indexing_prompt(&metadata);
            let system_prompt = "You are a precise documentation generator. \
                Output ONLY the requested JSON. No extra commentary.";

            let msgs = vec![LlmMessage::user(&prompt)];
            let response = self
                .chat_with_fallback(&msgs, &[], None, system_prompt, Some(8192))
                .await;

            if response.success {
                let written =
                    tool_indexer::apply_llm_index_result(&response.text, &root_dir, &metadata);
                if written > 0 {
                    tool_indexer::save_index_hash_for_roots(&root_dir, &scan_roots);
                    log::info!("[Startup Indexing] LLM generated {} index files.", written,);
                } else {
                    log::warn!(
                        "[Startup Indexing] LLM response parsed but 0 files \
                         written. Falling back to template."
                    );
                    tool_indexer::generate_fallback_index(&metadata, &root_dir);
                    tool_indexer::save_index_hash_for_roots(&root_dir, &scan_roots);
                }
            } else {
                log::warn!("[Startup Indexing] LLM call failed. Using fallback template.");
                tool_indexer::generate_fallback_index(&metadata, &root_dir);
                tool_indexer::save_index_hash_for_roots(&root_dir, &scan_roots);
            }
        } else {
            // No LLM available — generate a basic template
            log::info!("[Startup Indexing] No LLM available. Generating fallback index.");
            tool_indexer::generate_fallback_index(&metadata, &root_dir);
            tool_indexer::save_index_hash_for_roots(&root_dir, &scan_roots);
        }

        log::info!("[Startup Indexing] Completed.");
        self.publish_runtime_event(
            "startup_indexing",
            json!({
                "tool_count": tool_count,
                "skill_count": skill_count,
                "tool_roots": scan_roots,
                "skill_roots": skill_roots,
            }),
        );
    }

    /// Extractor sub-task logic. Invokes the LLM to glean long-term knowledge.
    async fn extract_and_save_memory(&self, history: &[LlmMessage], final_response: &str) {
        let ms_clone = self
            .memory_store
            .lock()
            .ok()
            .and_then(|ms| ms.as_ref().cloned());
        if ms_clone.is_none() {
            return;
        }
        let store = ms_clone.unwrap();

        // only extract if we have some messages
        if history.is_empty() {
            return;
        }

        let system_prompt = "You are an automated daemon component for TizenClaw responsible for extracting \
useful Long-Term Memories. Analyze the recent conversation snippet and the assistant's final response. \
Identify permanent facts, user preferences, names, device states, or specific instructions the user wants kept. \
Output ONLY a raw JSON array. DO NOT append Markdown code blocks. \
Output format: [{\"category\": \"preferences\", \"key\": \"pref::timezone\", \"value\": \"KST\"}] \
If there is nothing new to remember, output exactly: []";

        let mut msgs = vec![];
        let mut convo_text = String::new();
        // Give the last few messages for context
        for m in history.iter().rev().take(3).rev() {
            convo_text.push_str(&format!("{}: {}\n", m.role, m.text));
        }
        convo_text.push_str(&format!("assistant: {}\n", final_response));
        msgs.push(LlmMessage::user(&convo_text));

        log::debug!("[MemoryExtractor] Triggering LLM extraction sub-task...");
        let response = self
            .chat_with_fallback(&msgs, &[], None, system_prompt, Some(8192))
            .await;

        if response.success {
            let text = response.text.trim();
            // clean potential markdown code block
            let clean_json = if text.starts_with("```json") {
                text.trim_start_matches("```json")
                    .trim_end_matches("```")
                    .trim()
            } else if text.starts_with("```") {
                text.trim_start_matches("```")
                    .trim_end_matches("```")
                    .trim()
            } else {
                text
            };

            if clean_json == "[]" || clean_json.is_empty() {
                log::debug!("[MemoryExtractor] No new memories extracted.");
                return;
            }

            if let Ok(parsed) = serde_json::from_str::<Vec<serde_json::Value>>(clean_json) {
                let mut count = 0;
                for item in parsed {
                    if let (Some(cat), Some(k), Some(v)) = (
                        item.get("category").and_then(|v| v.as_str()),
                        item.get("key").and_then(|v| v.as_str()),
                        item.get("value").and_then(|v| v.as_str()),
                    ) {
                        store.set(k, v, cat);
                        count += 1;
                        log::debug!("[MemoryExtractor] Saved memory -> {}: {}", k, v);
                    }
                }
                if count > 0 {
                    log::debug!(
                        "[MemoryExtractor] Successfully saved {} extracted memories.",
                        count
                    );
                }
            } else {
                log::warn!(
                    "[MemoryExtractor] Failed to parse JSON response: {}",
                    clean_json
                );
            }
        } else {
            log::warn!("[MemoryExtractor] Extractor LLM call failed.");
        }
    }
}
