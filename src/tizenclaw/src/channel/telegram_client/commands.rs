impl TelegramClient {
    fn parse_command(text: &str) -> Option<(String, Vec<String>)> {
        let trimmed = text.trim();
        if !trimmed.starts_with('/') {
            return None;
        }

        let mut parts = trimmed.split_whitespace();
        let command_token = parts.next()?;
        let command = command_token
            .trim_start_matches('/')
            .split('@')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if command.is_empty() {
            return None;
        }

        Some((command, parts.map(|part| part.to_string()).collect()))
    }

    fn command_argument_text(text: &str) -> Option<String> {
        let trimmed = text.trim();
        if !trimmed.starts_with('/') {
            return None;
        }

        trimmed
            .split_once(char::is_whitespace)
            .map(|(_, rest)| rest.trim().to_string())
            .filter(|rest| !rest.is_empty())
    }

    fn load_chat_state_snapshot(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        chat_id: i64,
    ) -> TelegramChatState {
        match chat_states.lock() {
            Ok(states) => states.get(&chat_id).cloned().unwrap_or_default(),
            Err(err) => {
                log::warn!("TelegramClient: state lock poisoned: {}", err);
                TelegramChatState::default()
            }
        }
    }

    fn mutate_chat_state<F>(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
        mutator: F,
    ) -> String
    where
        F: FnOnce(&mut TelegramChatState) -> String,
    {
        let (reply, snapshot) = match chat_states.lock() {
            Ok(mut states) => {
                let state = states.entry(chat_id).or_default();
                let reply = mutator(state);
                (reply, states.clone())
            }
            Err(err) => {
                return format!("State update failed: {}", err);
            }
        };

        Self::persist_chat_states(state_path, &snapshot);
        reply
    }

    fn set_pending_menu(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
        pending_menu: Option<TelegramPendingMenu>,
    ) {
        let _ = Self::mutate_chat_state(chat_states, state_path, chat_id, move |state| {
            state.pending_menu = pending_menu;
            String::new()
        });
    }

    fn pending_menu_command(
        state: &TelegramChatState,
        text: &str,
        cli_backends: &TelegramCliBackendRegistry,
    ) -> Option<String> {
        let selection = text.trim().parse::<usize>().ok()?;
        if selection == 0 {
            return None;
        }

        match state.pending_menu.as_ref()? {
            TelegramPendingMenu::SelectMode => match selection {
                1 => Some("/select chat".to_string()),
                2 => Some("/select coding".to_string()),
                _ => None,
            },
            TelegramPendingMenu::CodingAgent => cli_backends
                .backends()
                .nth(selection - 1)
                .map(|backend| format!("/coding_agent {}", backend.as_str())),
            TelegramPendingMenu::Model => {
                let backend = state.effective_cli_backend(cli_backends);
                let (choices, _) = Self::available_model_choices(state, &backend, cli_backends);
                if selection <= choices.len() {
                    Some(format!("/model {}", choices[selection - 1].value.trim()))
                } else if selection == choices.len() + 1 {
                    Some("/model reset".to_string())
                } else {
                    None
                }
            }
            TelegramPendingMenu::ExecutionMode => match selection {
                1 => Some("/mode plan".to_string()),
                2 => Some("/mode fast".to_string()),
                _ => None,
            },
            TelegramPendingMenu::AutoApprove => match selection {
                1 => Some("/auto_approve on".to_string()),
                2 => Some("/auto_approve off".to_string()),
                _ => None,
            },
        }
    }

    fn set_interaction_mode(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
        args: &[String],
    ) -> TelegramOutgoingMessage {
        let Some(mode_raw) = args.first() else {
            Self::set_pending_menu(
                chat_states,
                state_path,
                chat_id,
                Some(TelegramPendingMenu::SelectMode),
            );
            return TelegramOutgoingMessage::with_markup("Select Mode.", Self::select_keyboard());
        };
        let Some(mode) = TelegramInteractionMode::parse(mode_raw) else {
            Self::set_pending_menu(
                chat_states,
                state_path,
                chat_id,
                Some(TelegramPendingMenu::SelectMode),
            );
            return TelegramOutgoingMessage::with_markup(
                "Choose [chat] or [coding].",
                Self::select_keyboard(),
            );
        };

        TelegramOutgoingMessage::with_removed_keyboard(Self::mutate_chat_state(
            chat_states,
            state_path,
            chat_id,
            move |state| {
                state.interaction_mode = mode;
                state.pending_menu = None;
                format!(
                    "Mode: {}\nCodingAgent: {}",
                    Self::value_label(mode.as_str()),
                    Self::backend_label(&state.cli_backend)
                )
            },
        ))
    }

    fn set_cli_backend(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
        args: &[String],
        cli_backends: &TelegramCliBackendRegistry,
        cli_backend_paths: &HashMap<TelegramCliBackend, String>,
    ) -> TelegramOutgoingMessage {
        let Some(backend_raw) = args.first() else {
            Self::set_pending_menu(
                chat_states,
                state_path,
                chat_id,
                Some(TelegramPendingMenu::CodingAgent),
            );
            return TelegramOutgoingMessage::with_markup(
                "Select CodingAgent.",
                Self::cli_backend_keyboard(cli_backends),
            );
        };
        let Some(backend) = cli_backends.parse(backend_raw) else {
            Self::set_pending_menu(
                chat_states,
                state_path,
                chat_id,
                Some(TelegramPendingMenu::CodingAgent),
            );
            return TelegramOutgoingMessage::with_markup(
                format!(
                    "Choose CodingAgent: {}.",
                    Self::backend_choices_labels_text(cli_backends)
                ),
                Self::cli_backend_keyboard(cli_backends),
            );
        };

        let availability = cli_backend_paths
            .get(&backend)
            .map(|path| format!("Binary: {}", Self::value_label(path)))
            .unwrap_or_else(|| format!("Binary: {}", Self::value_label("not found")));

        TelegramOutgoingMessage::with_removed_keyboard(Self::mutate_chat_state(
            chat_states,
            state_path,
            chat_id,
            move |state| {
                state.cli_backend = backend.clone();
                state.pending_menu = None;
                let availability = availability.replace('`', "");
                format!(
                    "CodingAgent: {}\n{}",
                    Self::backend_label(&backend),
                    availability
                )
            },
        ))
    }

    fn set_model(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
        args: &[String],
        cli_backends: &TelegramCliBackendRegistry,
    ) -> TelegramOutgoingMessage {
        if args.is_empty() {
            let state = Self::load_chat_state_snapshot(chat_states, chat_id);
            let backend = state.effective_cli_backend(cli_backends);
            let (choices, _) = Self::available_model_choices(&state, &backend, cli_backends);
            Self::set_pending_menu(
                chat_states,
                state_path,
                chat_id,
                Some(TelegramPendingMenu::Model),
            );
            return TelegramOutgoingMessage::with_markup(
                Self::format_model_menu_text(&state, &backend, cli_backends),
                Self::model_keyboard(&choices),
            );
        }

        let requested = args.join(" ").trim().to_string();
        if requested.is_empty() {
            return TelegramOutgoingMessage::plain("Model name cannot be empty.");
        }

        match requested.to_ascii_lowercase().as_str() {
            "list" | "menu" | "show" => {
                let state = Self::load_chat_state_snapshot(chat_states, chat_id);
                let backend = state.effective_cli_backend(cli_backends);
                let (choices, _) = Self::available_model_choices(&state, &backend, cli_backends);
                Self::set_pending_menu(
                    chat_states,
                    state_path,
                    chat_id,
                    Some(TelegramPendingMenu::Model),
                );
                TelegramOutgoingMessage::with_markup(
                    Self::format_model_menu_text(&state, &backend, cli_backends),
                    Self::model_keyboard(&choices),
                )
            }
            "reset" | "clear" | "default" => TelegramOutgoingMessage::with_removed_keyboard(
                Self::mutate_chat_state(chat_states, state_path, chat_id, move |state| {
                    let backend = state.effective_cli_backend(cli_backends);
                    state.model_overrides.remove(backend.as_str());
                    state.pending_menu = None;
                    let model = state
                        .effective_cli_model(&backend, cli_backends)
                        .unwrap_or_else(|| "auto".to_string());
                    let source = state.effective_cli_model_source(&backend, cli_backends);
                    format!(
                        "CodingAgent: {}\nModel: {}\nSource: {}",
                        Self::backend_label(&backend),
                        Self::value_label(model),
                        Self::value_label(source)
                    )
                }),
            ),
            _ => TelegramOutgoingMessage::with_removed_keyboard(Self::mutate_chat_state(
                chat_states,
                state_path,
                chat_id,
                move |state| {
                    let backend = state.effective_cli_backend(cli_backends);
                    state
                        .model_overrides
                        .insert(backend.as_str().to_string(), requested.clone());
                    state.pending_menu = None;
                    format!(
                        "CodingAgent: {}\nModel: {}\nSource: {}",
                        Self::backend_label(&backend),
                        Self::value_label(requested.clone()),
                        Self::value_label("chat override")
                    )
                },
            )),
        }
    }

    fn resolve_project_directory(
        requested: &str,
        default_cli_workdir: &Path,
        state: &TelegramChatState,
    ) -> Result<PathBuf, String> {
        let trimmed = requested.trim();
        if trimmed.is_empty() {
            return Err("Project path cannot be empty.".to_string());
        }

        let effective_base = state.effective_cli_workdir(default_cli_workdir);
        let candidate = PathBuf::from(trimmed);
        let resolved = if candidate.is_absolute() {
            candidate
        } else {
            effective_base.join(candidate)
        };

        let canonical = std::fs::canonicalize(&resolved).map_err(|err| {
            format!(
                "Project directory '{}' could not be resolved: {}",
                resolved.display(),
                err
            )
        })?;
        if !canonical.is_dir() {
            return Err(format!(
                "Project directory '{}' is not a directory.",
                canonical.display()
            ));
        }

        Ok(canonical)
    }

    fn set_project_directory(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
        args: &[String],
        default_cli_workdir: &Path,
    ) -> TelegramOutgoingMessage {
        if args.is_empty() {
            let state = Self::load_chat_state_snapshot(chat_states, chat_id);
            let effective = state.effective_cli_workdir(default_cli_workdir);
            return TelegramOutgoingMessage::plain(format!(
                "Project: {}\nUse: /project [path] | /project reset",
                Self::value_label(effective.display().to_string())
            ));
        }

        let requested = args.join(" ");
        match requested.trim().to_ascii_lowercase().as_str() {
            "reset" | "clear" | "default" => {
                let default_display = default_cli_workdir.display().to_string();
                return TelegramOutgoingMessage::plain(Self::mutate_chat_state(
                    chat_states,
                    state_path,
                    chat_id,
                    move |state| {
                        state.project_dir = None;
                        format!(
                            "Project: {}\nPath: {}",
                            Self::value_label("default"),
                            Self::value_label(&default_display)
                        )
                    },
                ));
            }
            _ => {}
        }

        let state = Self::load_chat_state_snapshot(chat_states, chat_id);
        let project_dir =
            match Self::resolve_project_directory(&requested, default_cli_workdir, &state) {
                Ok(path) => path,
                Err(err) => return TelegramOutgoingMessage::plain(err),
            };
        let project_dir_text = project_dir.display().to_string();

        TelegramOutgoingMessage::plain(Self::mutate_chat_state(
            chat_states,
            state_path,
            chat_id,
            move |state| {
                state.project_dir = Some(project_dir_text.clone());
                format!("Project: {}", Self::value_label(&project_dir_text))
            },
        ))
    }

    fn set_execution_mode(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
        args: &[String],
    ) -> TelegramOutgoingMessage {
        let Some(mode_raw) = args.first() else {
            Self::set_pending_menu(
                chat_states,
                state_path,
                chat_id,
                Some(TelegramPendingMenu::ExecutionMode),
            );
            return TelegramOutgoingMessage::with_markup(
                "Select CodingMode.",
                Self::mode_keyboard(),
            );
        };
        let Some(mode) = TelegramExecutionMode::parse(mode_raw) else {
            Self::set_pending_menu(
                chat_states,
                state_path,
                chat_id,
                Some(TelegramPendingMenu::ExecutionMode),
            );
            return TelegramOutgoingMessage::with_markup(
                "Choose [plan] or [fast].",
                Self::mode_keyboard(),
            );
        };

        TelegramOutgoingMessage::with_removed_keyboard(Self::mutate_chat_state(
            chat_states,
            state_path,
            chat_id,
            move |state| {
                state.execution_mode = mode;
                state.pending_menu = None;
                format!("CodingMode: {}", Self::value_label(mode.as_str()))
            },
        ))
    }

    fn set_auto_approve(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
        args: &[String],
    ) -> TelegramOutgoingMessage {
        let Some(value_raw) = args.first() else {
            Self::set_pending_menu(
                chat_states,
                state_path,
                chat_id,
                Some(TelegramPendingMenu::AutoApprove),
            );
            return TelegramOutgoingMessage::with_markup(
                "Select AutoApprove.",
                Self::auto_approve_keyboard(),
            );
        };
        let enabled = match value_raw.trim().to_ascii_lowercase().as_str() {
            "on" | "true" | "yes" | "1" => true,
            "off" | "false" | "no" | "0" => false,
            _ => {
                Self::set_pending_menu(
                    chat_states,
                    state_path,
                    chat_id,
                    Some(TelegramPendingMenu::AutoApprove),
                );
                return TelegramOutgoingMessage::with_markup(
                    "Choose [on] or [off].",
                    Self::auto_approve_keyboard(),
                );
            }
        };

        TelegramOutgoingMessage::with_removed_keyboard(Self::mutate_chat_state(
            chat_states,
            state_path,
            chat_id,
            move |state| {
                state.auto_approve = enabled;
                state.pending_menu = None;
                format!(
                    "AutoApprove: {}\nCodingAgent: {}",
                    Self::value_label(if enabled { "on" } else { "off" }),
                    Self::backend_label(&state.cli_backend)
                )
            },
        ))
    }

    fn start_new_session(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
    ) -> TelegramOutgoingMessage {
        let mut prepared_state = None;
        let reply = Self::mutate_chat_state(chat_states, state_path, chat_id, |state| {
            let mode = state.interaction_mode;
            match mode {
                TelegramInteractionMode::Chat => {
                    state.chat_session_index = state.chat_session_index.saturating_add(1);
                }
                TelegramInteractionMode::Coding => {
                    state.coding_session_index = state.coding_session_index.saturating_add(1);
                }
            }

            prepared_state = Some(state.clone());
            format!(
                "Session: {}",
                Self::session_value_label(&state.session_label_for(mode))
            )
        });

        if let Some(state) = prepared_state {
            Self::ensure_session_file(chat_id, state.interaction_mode, &state);
        }

        TelegramOutgoingMessage::plain(reply)
    }

    fn chat_session_id(chat_id: i64, state: &TelegramChatState) -> String {
        format!(
            "tg_{}_{}",
            chat_id,
            state.session_label_for(TelegramInteractionMode::Chat)
        )
    }

    fn format_chat_usage_report(state: &TelegramChatState, usage: &Value) -> String {
        let read = |name: &str| usage.get(name).and_then(Value::as_i64).unwrap_or(0);
        format!(
            "Mode: {}\n\
Session: {}\n\
Prompt: {}\n\
Completion: {}\n\
CacheWrite: {}\n\
CacheRead: {}\n\
Requests: {}\n\
Refresh: {}\n\
Remaining: {}\n\
Reset: {}",
            Self::value_label(TelegramInteractionMode::Chat.as_str()),
            Self::session_value_label_for_mode(state, TelegramInteractionMode::Chat),
            Self::value_label(read("prompt_tokens").to_string()),
            Self::value_label(read("completion_tokens").to_string()),
            Self::value_label(read("cache_creation_input_tokens").to_string()),
            Self::value_label(read("cache_read_input_tokens").to_string()),
            Self::value_label(read("total_requests").to_string()),
            Self::value_label("updates after the next chat response"),
            Self::value_label("not tracked by daemon session store"),
            Self::value_label("not tracked by daemon session store")
        )
    }

    fn format_coding_usage_report(
        state: &TelegramChatState,
        backend: &TelegramCliBackend,
        cli_backends: &TelegramCliBackendRegistry,
    ) -> String {
        let usage = state.usage_for(backend);
        let backend_definition = cli_backends.get(backend);
        let effective_model = state
            .effective_cli_model(backend, cli_backends)
            .unwrap_or_else(|| "auto".to_string());
        let model_source = state.effective_cli_model_source(backend, cli_backends);
        let usage_source = backend_definition
            .map(|definition| definition.usage_source_label.as_str())
            .filter(|label| !label.trim().is_empty())
            .unwrap_or("backend-specific usage payload");
        let refresh_hint = backend_definition
            .and_then(|definition| definition.usage_refresh_hint.as_deref())
            .unwrap_or("updates after the next successful backend run");
        let mut lines = vec![
            format!(
                "Mode: {}",
                Self::value_label(TelegramInteractionMode::Coding.as_str())
            ),
            format!(
                "Session: {}",
                Self::session_value_label_for_mode(state, TelegramInteractionMode::Coding)
            ),
            format!("CodingAgent: {}", Self::backend_label(backend)),
            format!("Model: {}", Self::value_label(effective_model)),
            format!("ModelSource: {}", Self::value_label(model_source)),
            format!("Source: {}", Self::value_label(usage_source)),
            format!(
                "Updated: {}",
                Self::value_label(Self::usage_capture_label(usage.last_actual_usage_at_ms))
            ),
            format!("Refresh: {}", Self::value_label(refresh_hint)),
        ];

        if let Some(actual) = &usage.last_actual_usage {
            lines.push(format!(
                "LatestCLI: {}",
                Self::value_label(actual.session_id.as_deref().unwrap_or("-"))
            ));
            lines.push(format!(
                "ReportedModel: {}",
                Self::value_label(actual.model.as_deref().unwrap_or("-"))
            ));
            lines.push(format!(
                "Latest: {}",
                Self::value_label(format!(
                    "in {} | out {} | total {}",
                    actual.input_tokens, actual.output_tokens, actual.total_tokens
                ))
            ));
            if actual.cached_input_tokens > 0 {
                lines.push(format!(
                    "Cached: {}",
                    Self::value_label(actual.cached_input_tokens.to_string())
                ));
            }
            if actual.cache_creation_input_tokens > 0 {
                lines.push(format!(
                    "CacheWrite: {}",
                    Self::value_label(actual.cache_creation_input_tokens.to_string())
                ));
            }
            if actual.cache_read_input_tokens > 0 {
                lines.push(format!(
                    "CacheRead: {}",
                    Self::value_label(actual.cache_read_input_tokens.to_string())
                ));
            }
            if actual.thought_tokens > 0 {
                lines.push(format!(
                    "Thought: {}",
                    Self::value_label(actual.thought_tokens.to_string())
                ));
            }
            if actual.tool_tokens > 0 {
                lines.push(format!(
                    "Tool: {}",
                    Self::value_label(actual.tool_tokens.to_string())
                ));
            }
            lines.push(format!(
                "Remaining: {}",
                Self::value_label(
                    actual
                        .remaining_text
                        .as_deref()
                        .or_else(|| {
                            backend_definition
                                .and_then(|definition| definition.remaining_usage_hint.as_deref())
                        })
                        .unwrap_or("pending first successful run")
                )
            ));
            lines.push(format!(
                "Reset: {}",
                Self::value_label(
                    actual
                        .reset_at
                        .as_deref()
                        .or_else(|| {
                            backend_definition
                                .and_then(|definition| definition.reset_usage_hint.as_deref())
                        })
                        .unwrap_or("pending first successful run")
                )
            ));
        } else {
            lines.push(format!("Latest: {}", Self::value_label("not reported yet")));
            lines.push(format!(
                "Remaining: {}",
                Self::value_label(
                    backend_definition
                        .and_then(|definition| definition.remaining_usage_hint.as_deref())
                        .unwrap_or("pending first successful run")
                )
            ));
            lines.push(format!(
                "Reset: {}",
                Self::value_label(
                    backend_definition
                        .and_then(|definition| definition.reset_usage_hint.as_deref())
                        .unwrap_or("pending first successful run")
                )
            ));
        }

        lines.push(format!(
            "Total: {}",
            Self::value_label(format!(
                "in {} | out {} | total {}",
                usage.total_cli_input_tokens, usage.total_cli_output_tokens, usage.total_cli_tokens
            ))
        ));
        if usage.total_cli_cached_input_tokens > 0 {
            lines.push(format!(
                "TotalCached: {}",
                Self::value_label(usage.total_cli_cached_input_tokens.to_string())
            ));
        }
        if usage.total_cli_cache_creation_input_tokens > 0 {
            lines.push(format!(
                "TotalCacheWrite: {}",
                Self::value_label(usage.total_cli_cache_creation_input_tokens.to_string())
            ));
        }
        if usage.total_cli_cache_read_input_tokens > 0 {
            lines.push(format!(
                "TotalCacheRead: {}",
                Self::value_label(usage.total_cli_cache_read_input_tokens.to_string())
            ));
        }
        if usage.total_cli_thought_tokens > 0 {
            lines.push(format!(
                "TotalThought: {}",
                Self::value_label(usage.total_cli_thought_tokens.to_string())
            ));
        }
        if usage.total_cli_tool_tokens > 0 {
            lines.push(format!(
                "TotalTool: {}",
                Self::value_label(usage.total_cli_tool_tokens.to_string())
            ));
        }

        lines.push(format!(
            "Runs: {}",
            Self::value_label(format!(
                "req {} | ok {} | fail {}",
                usage.requests, usage.successes, usage.failures
            ))
        ));
        lines.push(format!(
            "Last: {}",
            Self::value_label(format!(
                "avg {}ms | exit {}",
                usage.average_duration_ms(),
                usage
                    .last_exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ))
        ));

        lines.join("\n")
    }

    fn format_usage_text(
        chat_id: i64,
        state: &TelegramChatState,
        cli_backends: &TelegramCliBackendRegistry,
        agent: Option<&crate::core::agent_core::AgentCore>,
    ) -> String {
        match state.interaction_mode {
            TelegramInteractionMode::Chat => {
                let Some(agent) = agent else {
                    return format!(
                        "Mode: {}\nSession: {}\nStatus: {}",
                        Self::value_label(TelegramInteractionMode::Chat.as_str()),
                        Self::session_value_label_for_mode(state, TelegramInteractionMode::Chat),
                        Self::value_label("usage unavailable")
                    );
                };
                let Some(session_store) = agent.get_session_store() else {
                    return format!(
                        "Mode: {}\nSession: {}\nStatus: {}",
                        Self::value_label(TelegramInteractionMode::Chat.as_str()),
                        Self::session_value_label_for_mode(state, TelegramInteractionMode::Chat),
                        Self::value_label("usage unavailable")
                    );
                };
                let session_id = Self::chat_session_id(chat_id, state);
                let usage = session_store
                    .store()
                    .load_token_usage(&session_id)
                    .to_json();
                Self::format_chat_usage_report(state, &usage)
            }
            TelegramInteractionMode::Coding => {
                let backend = state.effective_cli_backend(cli_backends);
                Self::format_coding_usage_report(state, &backend, cli_backends)
            }
        }
    }

    fn format_status_text(
        chat_id: i64,
        state: &TelegramChatState,
        cli_workdir: &Path,
        cli_backends: &TelegramCliBackendRegistry,
        cli_backend_paths: &HashMap<TelegramCliBackend, String>,
        active_handlers: i32,
    ) -> String {
        let effective_workdir = state.effective_cli_workdir(cli_workdir);
        let backend = state.effective_cli_backend(cli_backends);
        let backend_path = cli_backend_paths
            .get(&backend)
            .map(|path| path.as_str())
            .unwrap_or("not found");
        let usage = state.usage_for(&backend);
        let model = state
            .effective_cli_model(&backend, cli_backends)
            .unwrap_or_else(|| "auto".to_string());

        format!(
            "TizenClaw: {}\n\
Mode: {}\n\
Session: {}\n\
CodingAgent: {}\n\
Model: {}\n\
CodingMode: {}\n\
AutoApprove: {}\n\
Project: {}\n\
Binary: {}\n\
Handlers: {}\n\
Runs: {}",
            Self::value_label("online"),
            Self::value_label(state.interaction_mode.as_str()),
            Self::active_session_value_label(state),
            Self::backend_label(&backend),
            Self::value_label(model),
            Self::value_label(state.execution_mode.as_str()),
            Self::value_label(if state.auto_approve { "on" } else { "off" }),
            Self::value_label(effective_workdir.display().to_string()),
            Self::value_label(backend_path),
            Self::value_label(active_handlers.to_string()),
            Self::value_label(format!(
                "req {} | ok {} | fail {}",
                usage.requests, usage.successes, usage.failures
            ))
        )
    }

    fn handle_command(
        chat_id: i64,
        text: &str,
        agent: Option<&crate::core::agent_core::AgentCore>,
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        cli_backends: &TelegramCliBackendRegistry,
        cli_backend_paths: &HashMap<TelegramCliBackend, String>,
        cli_workdir: &Path,
        active_handlers: i32,
    ) -> Option<TelegramOutgoingMessage> {
        let (command, args) = Self::parse_command(text)?;

        let reply = match command.as_str() {
            "start" | "help" => {
                TelegramOutgoingMessage::plain(Self::supported_commands_text(cli_backends))
            }
            "select" => Self::set_interaction_mode(chat_states, state_path, chat_id, &args),
            "coding-agent" | "coding_agent" | "agent-cli" | "agent_cli" | "cli-backend"
            | "cli_backend" => Self::set_cli_backend(
                chat_states,
                state_path,
                chat_id,
                &args,
                cli_backends,
                cli_backend_paths,
            ),
            "devel" => match Self::command_argument_text(text) {
                Some(prompt_text) => match crate::core::devel_mode::create_prompt_file(&prompt_text) {
                    Ok(path) => {
                        let watcher_state = if crate::core::devel_mode::devel_status(Path::new("."), &std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))).result_watcher_active {
                            "active"
                        } else {
                            "inactive"
                        };
                        TelegramOutgoingMessage::plain(format!(
                            "DevelPrompt: {}\nFile: {}\nWatcher: {}",
                            Self::value_label("queued"),
                            Self::value_label(path.display().to_string()),
                            Self::value_label(watcher_state)
                        ))
                    }
                    Err(err) => TelegramOutgoingMessage::plain(format!(
                        "DevelPrompt: {}\nReason: {}",
                        Self::value_label("error"),
                        Self::value_label(err)
                    )),
                },
                None => TelegramOutgoingMessage::plain(
                    "Usage: /devel [prompt]\nExample: /devel Telegram devel flow를 파일 브리지로 바꿔줘."
                        .to_string(),
                ),
            },
            "devel_result" => {
                let repo_root = std::env::current_dir()
                    .ok()
                    .and_then(|cwd| crate::core::devel_mode::detect_repo_root(&cwd))
                    .unwrap_or_else(|| {
                        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                    });
                let result = crate::core::devel_mode::latest_devel_result(&repo_root);

                if result.available {
                    let latest_path = result
                        .latest_result_path
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| result.result_dir.display().to_string());
                    let mut reply = format!(
                        "DevelResult: {}\nFile: {}",
                        Self::value_label("loaded"),
                        Self::value_label(latest_path),
                    );
                    if !result.latest_result_matches_latest_prompt {
                        let prompt_path = result
                            .latest_prompt_path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| result.result_dir.display().to_string());
                        let prompt_result_path = result
                            .latest_prompt_result_path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| result.result_dir.display().to_string());
                        reply.push_str(&format!(
                            "\nPrompt: {}\nExpectedResult: {}\nState: {}",
                            Self::value_label(prompt_path),
                            Self::value_label(prompt_result_path),
                            Self::value_label("latest prompt pending; showing latest completed result"),
                        ));
                    }
                    reply.push_str(&format!("\n\n{}", result.content.trim()));
                    TelegramOutgoingMessage::plain(reply)
                } else {
                    TelegramOutgoingMessage::plain(format!(
                        "DevelResult: {}\nDir: {}",
                        Self::value_label("empty"),
                        Self::value_label(result.result_dir.display().to_string())
                    ))
                }
            }
            "model" => Self::set_model(chat_states, state_path, chat_id, &args, cli_backends),
            "project" => {
                Self::set_project_directory(chat_states, state_path, chat_id, &args, cli_workdir)
            }
            "new_session" => Self::start_new_session(chat_states, state_path, chat_id),
            "mode" => Self::set_execution_mode(chat_states, state_path, chat_id, &args),
            "auto-approve" | "auto_approve" => {
                Self::set_auto_approve(chat_states, state_path, chat_id, &args)
            }
            "usage" => {
                let state = Self::load_chat_state_snapshot(chat_states, chat_id);
                TelegramOutgoingMessage::plain(Self::format_usage_text(
                    chat_id,
                    &state,
                    cli_backends,
                    agent,
                ))
            }
            "status" => {
                let state = Self::load_chat_state_snapshot(chat_states, chat_id);
                TelegramOutgoingMessage::plain(Self::format_status_text(
                    chat_id,
                    &state,
                    cli_workdir,
                    cli_backends,
                    cli_backend_paths,
                    active_handlers,
                ))
            }
            _ => TelegramOutgoingMessage::with_markup(
                format!(
                    "Unknown: {}\nUse: {}",
                    Self::value_label(format!("/{}", command)),
                    Self::value_label("/help")
                ),
                Self::build_reply_keyboard(&[&["/help"]]),
            ),
        };

        Some(reply)
    }

    fn ensure_chat_state(
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: &Path,
        chat_id: i64,
    ) -> (TelegramChatState, bool) {
        let (state, snapshot, is_new) = match chat_states.lock() {
            Ok(mut states) => {
                if let Some(state) = states.get(&chat_id).cloned() {
                    (state, None, false)
                } else {
                    let state = TelegramChatState::default();
                    states.insert(chat_id, state.clone());
                    (state, Some(states.clone()), true)
                }
            }
            Err(err) => {
                log::warn!(
                    "TelegramClient: state lock poisoned while ensuring chat state: {}",
                    err
                );
                (TelegramChatState::default(), None, false)
            }
        };

        if let Some(snapshot) = snapshot {
            Self::persist_chat_states(state_path, &snapshot);
        }

        if is_new {
            Self::ensure_session_file(chat_id, TelegramInteractionMode::Chat, &state);
            Self::ensure_session_file(chat_id, TelegramInteractionMode::Coding, &state);
        }

        (state, is_new)
    }

    fn build_connected_message(state: &TelegramChatState) -> TelegramOutgoingMessage {
        TelegramOutgoingMessage::plain(format!(
            "Telegram: {}\nMode: {}\nSession: {}\nCodingAgent: {}",
            Self::value_label("connected"),
            Self::value_label(state.interaction_mode.as_str()),
            Self::active_session_value_label(state),
            Self::backend_label(&state.cli_backend)
        ))
    }

    fn build_startup_message(state: &TelegramChatState) -> TelegramOutgoingMessage {
        TelegramOutgoingMessage::plain(format!(
            "TizenClaw: {}\nMode: {}\nSession: {}\nCodingAgent: {}",
            Self::value_label("online"),
            Self::value_label(state.interaction_mode.as_str()),
            Self::active_session_value_label(state),
            Self::backend_label(&state.cli_backend)
        ))
    }

    fn startup_notification_targets(
        allowed_chat_ids: &Arc<HashSet<i64>>,
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
    ) -> Vec<(i64, TelegramChatState)> {
        let mut snapshot = match chat_states.lock() {
            Ok(states) => states.clone(),
            Err(err) => {
                log::warn!(
                    "TelegramClient: state lock poisoned while gathering startup targets: {}",
                    err
                );
                HashMap::new()
            }
        };

        for chat_id in allowed_chat_ids.iter() {
            snapshot.entry(*chat_id).or_default();
        }

        let mut targets = snapshot.into_iter().collect::<Vec<_>>();
        targets.sort_by_key(|(chat_id, _)| *chat_id);
        targets
    }

    fn broadcast_startup_status(
        bot_token: &str,
        allowed_chat_ids: &Arc<HashSet<i64>>,
        chat_states: &Arc<Mutex<HashMap<i64, TelegramChatState>>>,
    ) {
        for (chat_id, state) in Self::startup_notification_targets(allowed_chat_ids, chat_states) {
            let message = Self::build_startup_message(&state);
            Self::send_telegram_message(bot_token, chat_id, &message);
        }
    }

    fn build_cli_prompt(
        chat_id: i64,
        state: &TelegramChatState,
        execution_mode: TelegramExecutionMode,
        backend: &TelegramCliBackend,
        cli_workdir: &Path,
        text: &str,
    ) -> String {
        let mode_prefix = match execution_mode {
            TelegramExecutionMode::Plan => {
                "You are operating in TizenClaw Telegram coding mode. Start with a short plan, then perform the work carefully. Keep the final response concise and actionable."
            }
            TelegramExecutionMode::Fast => {
                "You are operating in TizenClaw Telegram coding mode. Optimize for speed, keep the response concise, and take the fastest reasonable path."
            }
        };
        let session_label = state.session_label_for(TelegramInteractionMode::Coding);
        let recent_context = Self::read_recent_session_excerpt(
            chat_id,
            TelegramInteractionMode::Coding,
            state,
            5000,
        );
        let history_block = if recent_context.trim().is_empty() {
            String::new()
        } else {
            format!(
                "\nCurrent Telegram coding session history ({})\n{}\n",
                session_label, recent_context
            )
        };

        format!(
            "{}\n\
\n\
Selected backend: {}\n\
Session: {}\n\
Project directory: {}\n\
\n\
{}\
User request:\n{}",
            mode_prefix,
            backend.as_str(),
            session_label,
            cli_workdir.display(),
            history_block,
            text.trim()
        )
    }

    fn build_tool_cli_prompt(
        state: &TelegramChatState,
        effective_cli_workdir: &Path,
        backend: &TelegramCliBackend,
        prompt: &str,
    ) -> String {
        let mode_prefix = match state.execution_mode {
            TelegramExecutionMode::Plan => {
                "You are operating as a local coding agent invoked by TizenClaw. Start with a short plan, then perform the work carefully. Keep the final response concise and actionable."
            }
            TelegramExecutionMode::Fast => {
                "You are operating as a local coding agent invoked by TizenClaw. Optimize for speed, keep the response concise, and take the fastest reasonable path."
            }
        };

        format!(
            "{}\n\nSelected backend: {}\nProject directory: {}\nAuto approve: {}\n\nUser request:\n{}",
            mode_prefix,
            backend.as_str(),
            effective_cli_workdir.display(),
            if state.auto_approve { "on" } else { "off" },
            prompt.trim()
        )
    }

    fn build_unified_agent_prompt(
        state: &TelegramChatState,
        default_cli_workdir: &Path,
        cli_backends: &TelegramCliBackendRegistry,
        text: &str,
    ) -> String {
        let backend = state.effective_cli_backend(cli_backends);
        let project_dir = state.effective_cli_workdir(default_cli_workdir);
        let model = state
            .effective_cli_model(&backend, cli_backends)
            .unwrap_or_else(|| "backend auto".to_string());

        format!(
            "You are handling a Telegram request through TizenClaw.\n\
\n\
Telegram development preferences:\n\
- Coding backend: {}\n\
- Coding model: {}\n\
- Project directory: {}\n\
- Coding execution mode: {}\n\
- Coding auto approve: {}\n\
\n\
Ordinary Telegram messages must be handled by TizenClaw first. If the user requests repository work, implementation, refactoring, debugging, testing, or other development work, prefer the run_coding_agent tool instead of replying with prose only.\n\
If the user requests periodic follow-up development work, use create_task and preserve the same coding defaults with project_dir, coding_backend, coding_model, execution_mode, and auto_approve.\n\
\n\
Telegram user request:\n{}",
            backend.as_str(),
            model,
            project_dir.display(),
            state.execution_mode.as_str(),
            if state.auto_approve { "on" } else { "off" },
            text.trim()
        )
    }

    fn build_cli_invocation(
        chat_id: i64,
        state: &TelegramChatState,
        effective_cli_workdir: &Path,
        cli_backends: &TelegramCliBackendRegistry,
        cli_backend_paths: &HashMap<TelegramCliBackend, String>,
        text: &str,
    ) -> Result<(String, Vec<String>), String> {
        let backend = state.effective_cli_backend(cli_backends);
        let definition = cli_backends.get(&backend).ok_or_else(|| {
            format!(
                "Selected backend `{}` is not defined in Telegram config.",
                backend.as_str()
            )
        })?;
        let binary = cli_backend_paths.get(&backend).cloned().ok_or_else(|| {
            format!(
                "Selected backend `{}` is not available on PATH.",
                backend.as_str()
            )
        })?;

        let prompt = Self::build_cli_prompt(
            chat_id,
            state,
            state.execution_mode,
            &backend,
            effective_cli_workdir,
            text,
        );
        let effective_model = state.effective_cli_model(&backend, cli_backends);
        let approval_value = if state.auto_approve {
            definition
                .invocation
                .auto_approve_value
                .as_deref()
                .or(definition.invocation.default_approval_value.as_deref())
                .unwrap_or("")
        } else {
            definition
                .invocation
                .default_approval_value
                .as_deref()
                .unwrap_or("")
        };
        let mut args = Vec::new();
        for template in &definition.invocation.args {
            args.extend(Self::render_cli_arg_template(
                template,
                &prompt,
                effective_cli_workdir,
                effective_model.as_deref(),
                definition.invocation.approval_placeholder.as_deref(),
                approval_value,
            ));
        }

        Ok((binary, args))
    }

    fn render_cli_arg_template(
        template: &str,
        prompt: &str,
        project_dir: &Path,
        model: Option<&str>,
        approval_placeholder: Option<&str>,
        approval_value: &str,
    ) -> Vec<String> {
        let trimmed = template.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        if trimmed == "{model_args}" {
            return model
                .map(|model| vec!["--model".to_string(), model.to_string()])
                .unwrap_or_default();
        }

        if trimmed == "{model}" && model.is_none() {
            return Vec::new();
        }

        if let Some(placeholder) = approval_placeholder {
            if trimmed == placeholder {
                return approval_value
                    .split_whitespace()
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string)
                    .collect();
            }
        }

        let mut rendered = trimmed.replace("{prompt}", prompt);
        rendered = rendered.replace("{project_dir}", project_dir.to_string_lossy().as_ref());
        if rendered.contains("{model}") {
            let Some(model) = model else {
                return Vec::new();
            };
            rendered = rendered.replace("{model}", model);
        }
        if let Some(placeholder) = approval_placeholder {
            rendered = rendered.replace(placeholder, approval_value);
        }
        vec![rendered]
    }
}
