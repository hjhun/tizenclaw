#[cfg(test)]
mod tests {
    use super::{
        TelegramChatState, TelegramCliActualUsage, TelegramCliBackend, TelegramCliBackendRegistry,
        TelegramCliUsageStats, TelegramClient, TelegramExecutionMode, TelegramInteractionMode,
    };
    use serde_json::json;
    use std::collections::{HashMap, HashSet};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn backend(value: &str) -> TelegramCliBackend {
        TelegramCliBackend::new(value)
    }

    fn default_registry() -> TelegramCliBackendRegistry {
        TelegramCliBackendRegistry::default()
    }

    #[test]
    fn parse_command_handles_bot_mentions() {
        let parsed = TelegramClient::parse_command("/status@tizenclaw_bot").unwrap();
        assert_eq!(parsed.0, "status");
        assert!(parsed.1.is_empty());
    }

    #[test]
    fn command_argument_text_preserves_prompt_spacing() {
        let parsed =
            TelegramClient::command_argument_text("/devel  build prompt bridge  ").unwrap();
        assert_eq!(parsed, "build prompt bridge");
    }

    #[test]
    fn parse_mode_aliases_work() {
        // "backend" is the user-facing alias for the backend/CLI mode.
        assert_eq!(
            TelegramInteractionMode::parse("backend"),
            Some(TelegramInteractionMode::Coding)
        );
        // Legacy coding aliases are no longer accepted.
        assert_eq!(TelegramInteractionMode::parse("coding"), None);
        assert_eq!(TelegramInteractionMode::parse("coding-agent"), None);
        assert_eq!(TelegramInteractionMode::parse("coding_agent"), None);
        assert_eq!(TelegramInteractionMode::parse("agent"), None);
        assert_eq!(
            TelegramExecutionMode::parse("fast"),
            Some(TelegramExecutionMode::Fast)
        );
        assert_eq!(
            default_registry().parse("claude-code"),
            Some(backend("claude"))
        );
    }

    #[test]
    fn default_chat_state_prefers_codex_plan_chat_mode() {
        let state = TelegramChatState::default();
        assert_eq!(state.interaction_mode, TelegramInteractionMode::Chat);
        assert_eq!(state.cli_backend, backend("codex"));
        assert_eq!(state.execution_mode, TelegramExecutionMode::Plan);
        assert!(!state.auto_approve);
        assert_eq!(
            state.session_label_for(TelegramInteractionMode::Chat),
            "chat-0001"
        );
        assert_eq!(
            state.session_label_for(TelegramInteractionMode::Coding),
            "backend-0001"
        );
    }

    #[test]
    fn send_message_payload_is_plain_text_json() {
        let payload = TelegramClient::build_send_message_payload(123, "value_with`markdown`", None);
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(json["chat_id"], 123);
        assert_eq!(json["text"], "value_with`markdown`");
        assert!(json.get("parse_mode").is_none());
        assert!(json.get("reply_markup").is_none());
    }

    #[test]
    fn supported_commands_text_lists_core_commands() {
        let help = TelegramClient::supported_commands_text(&default_registry());
        assert!(help.contains("/backend [name]"));
        assert!(help.contains("/model [name|list|reset]"));
        assert!(help.contains("/usage"));
        assert!(help.contains("/project"));
        assert!(help.contains("/new_session"));
        assert!(help.contains("/status"));
        assert!(!help.contains("/coding_agent"));
        assert!(!help.contains("/devel"));
        assert!(!help.contains("/auto_approve"));
        assert!(!help.contains("/mode ["));
    }

    #[test]
    fn set_my_commands_payload_contains_expected_commands() {
        let payload = TelegramClient::build_set_my_commands_payload();
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();
        let commands = json["commands"].as_array().unwrap();
        let names: Vec<&str> = commands
            .iter()
            .filter_map(|entry| entry["command"].as_str())
            .collect();

        assert_eq!(
            names,
            vec!["backend", "model", "project", "new_session", "usage", "status"]
        );
        assert!(!names.contains(&"coding_agent"));
        assert!(!names.contains(&"devel"));
        assert!(!names.contains(&"mode"));
        assert!(!names.contains(&"auto_approve"));
    }

    #[test]
    fn build_send_message_payload_can_include_reply_markup() {
        let payload = TelegramClient::build_send_message_payload(
            123,
            "pick one",
            Some(TelegramClient::mode_keyboard()),
        );
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(json["chat_id"], 123);
        assert_eq!(json["text"], "pick one");
        assert_eq!(json["reply_markup"]["one_time_keyboard"], true);
        assert_eq!(json["reply_markup"]["keyboard"][0][0], "/mode plan");
        assert_eq!(json["reply_markup"]["keyboard"][0][1], "/mode fast");
    }

    #[test]
    fn removed_keyboard_markup_is_serialized() {
        let payload = TelegramClient::build_send_message_payload(
            7,
            "done",
            Some(TelegramClient::remove_keyboard_markup()),
        );
        let json: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(json["reply_markup"]["remove_keyboard"], true);
    }

    #[test]
    fn cli_backend_keyboard_uses_backend_command() {
        let keyboard = TelegramClient::cli_backend_keyboard(&default_registry());
        assert_eq!(keyboard["keyboard"][0][0], "/backend codex");
        assert_eq!(keyboard["keyboard"][1][0], "/backend gemini");
        assert_eq!(keyboard["keyboard"][2][0], "/backend claude");
    }

    #[test]
    fn model_keyboard_exposes_operator_choices_and_reset() {
        // model_choices are no longer baked into the Rust defaults — they must
        // be supplied via telegram_config.json. This test verifies that choices
        // loaded through merge_config_value appear correctly in the keyboard.
        let mut registry = default_registry();
        registry.merge_config_value(Some(&serde_json::json!({
            "backends": {
                "gemini": {
                    "model_choices_source_label": "operator-configured gemini choices",
                    "model_choices": [
                        {"value": "gemini-2.5-flash", "label": "Flash", "description": "Fast balanced tier"},
                        {"value": "auto", "label": "Auto", "description": "Gemini CLI default routing"},
                        {"value": "gemini-2.5-pro", "label": "Pro", "description": "Stronger reasoning tier"},
                        {"value": "gemini-2.5-flash-lite", "label": "Flash Lite", "description": "Lightest tier"}
                    ]
                }
            }
        })));

        let state = TelegramChatState::default();
        let backend = backend("gemini");
        let (choices, source) =
            TelegramClient::available_model_choices(&state, &backend, &registry);
        let keyboard = TelegramClient::model_keyboard(&choices);

        assert_eq!(source, "operator-configured gemini choices");
        assert_eq!(keyboard["keyboard"][0][0], "/model gemini-2.5-flash");
        assert_eq!(keyboard["keyboard"][0][1], "/model auto");
        // 4 choices → 2 rows of 2, then reset on row 2
        assert_eq!(keyboard["keyboard"][2][0], "/model reset");
    }

    #[test]
    fn custom_backend_from_config_is_exposed_in_help_and_keyboard() {
        let mut registry = default_registry();
        registry.merge_config_value(Some(&serde_json::json!({
            "default_backend": "custom_agent",
            "backends": {
                "custom_agent": {
                    "aliases": ["custom"],
                    "binary_path": "/usr/bin/custom-agent",
                    "usage_hint": "`custom-agent run --json <prompt>`",
                    "invocation": {
                        "args": ["run", "--json", "{prompt}"]
                    },
                    "response_extractors": [
                        { "source": "stdout", "format": "json", "text_path": "result" }
                    ],
                    "usage_extractors": [
                        {
                            "source": "stdout",
                            "format": "json",
                            "input_tokens_path": "usage.input_tokens",
                            "output_tokens_path": "usage.output_tokens"
                        }
                    ]
                }
            }
        })));

        let keyboard = TelegramClient::cli_backend_keyboard(&registry);

        assert_eq!(keyboard["keyboard"][3][0], "/backend custom_agent");
        assert_eq!(registry.parse("custom"), Some(backend("custom_agent")));
        assert_eq!(registry.default_backend(), backend("custom_agent"));
    }

    #[test]
    fn connected_message_mentions_session() {
        let message = TelegramClient::build_connected_message(&TelegramChatState::default());
        assert!(message.text.contains("Telegram: [connected]"));
        assert!(message.text.contains("Session: [0001]"));
        assert!(!message.text.contains("CodingAgent:"));
        assert!(!message.text.contains("Mode:"));
        assert!(message.reply_markup.is_none());
    }

    #[test]
    fn startup_message_mentions_session() {
        let message = TelegramClient::build_startup_message(&TelegramChatState::default());
        assert!(message.text.contains("TizenClaw: [online]"));
        assert!(message.text.contains("Session: [0001]"));
        assert!(!message.text.contains("CodingAgent:"));
        assert!(!message.text.contains("Mode:"));
        assert!(message.reply_markup.is_none());
    }

    #[test]
    fn select_without_args_shows_only_select_submenu() {
        let chat_states = Arc::new(Mutex::new(HashMap::new()));
        let state_path = std::env::temp_dir().join(format!(
            "telegram_select_state_{}_{}.json",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));
        let reply = TelegramClient::handle_command(
            77,
            "/select",
            None,
            &chat_states,
            &state_path,
            &default_registry(),
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();

        assert!(reply.text.contains("Select Mode."));
        assert_eq!(
            reply.reply_markup.as_ref().unwrap()["keyboard"][0][0],
            "/select chat"
        );
        assert_eq!(
            reply.reply_markup.as_ref().unwrap()["keyboard"][0][1],
            "/select backend"
        );
        assert!(
            reply.reply_markup.as_ref().unwrap()["keyboard"][0]
                .as_array()
                .map(|row| row.len())
                == Some(2)
        );
    }

    #[test]
    fn select_with_valid_arg_removes_reply_keyboard() {
        let chat_states = Arc::new(Mutex::new(HashMap::new()));
        let state_path = std::env::temp_dir().join(format!(
            "telegram_select_success_{}_{}.json",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));
        let reply = TelegramClient::handle_command(
            77,
            "/select backend",
            None,
            &chat_states,
            &state_path,
            &default_registry(),
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();

        assert!(reply.text.contains("Mode: [backend]"));
        assert!(!reply.text.contains("CodingAgent:"));
        assert_eq!(
            reply.reply_markup.as_ref().unwrap()["remove_keyboard"],
            true
        );
    }

    #[test]
    fn select_menu_persists_pending_numeric_context() {
        let chat_states = Arc::new(Mutex::new(HashMap::new()));
        let state_path = std::env::temp_dir().join(format!(
            "telegram_select_numeric_{}_{}.json",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));

        let _ = TelegramClient::handle_command(
            77,
            "/select",
            None,
            &chat_states,
            &state_path,
            &default_registry(),
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();

        let state = TelegramClient::load_chat_state_snapshot(&chat_states, 77);
        assert_eq!(
            TelegramClient::pending_menu_command(&state, "1", &default_registry()).as_deref(),
            Some("/select chat")
        );
        assert_eq!(
            TelegramClient::pending_menu_command(&state, "2", &default_registry()).as_deref(),
            Some("/select backend")
        );
        assert_eq!(
            TelegramClient::pending_menu_command(&state, "3", &default_registry()),
            None
        );
    }

    #[test]
    fn project_without_args_reports_current_directory() {
        let chat_states = Arc::new(Mutex::new(HashMap::new()));
        let state_path = std::env::temp_dir().join(format!(
            "telegram_project_status_{}_{}.json",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));
        let reply = TelegramClient::handle_command(
            77,
            "/project",
            None,
            &chat_states,
            &state_path,
            &default_registry(),
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();

        assert!(reply.text.contains("Project: [/tmp]"));
        assert!(reply.text.contains("Use: /project [path] | /project reset"));
    }

    #[test]
    fn project_command_updates_chat_state() {
        let project_dir = std::env::temp_dir();
        let project_text = project_dir.display().to_string();
        let chat_states = Arc::new(Mutex::new(HashMap::new()));
        let state_path = std::env::temp_dir().join(format!(
            "telegram_project_set_{}_{}.json",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));
        let command = format!("/project {}", project_text);

        let reply = TelegramClient::handle_command(
            77,
            &command,
            None,
            &chat_states,
            &state_path,
            &default_registry(),
            &HashMap::new(),
            std::path::Path::new("/work"),
            0,
        )
        .unwrap();

        assert!(reply.text.contains("Project: ["));
        let state = TelegramClient::load_chat_state_snapshot(&chat_states, 77);
        let expected = project_dir
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(state.project_dir.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn removed_commands_return_unknown_response() {
        let chat_states = Arc::new(Mutex::new(HashMap::new()));
        let state_path = std::env::temp_dir().join(format!(
            "telegram_unknown_cmd_{}_{}.json",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));
        for cmd in &["/coding_agent", "/devel test", "/devel_result", "/mode plan", "/auto_approve on"] {
            let reply = TelegramClient::handle_command(
                77, cmd, None, &chat_states, &state_path,
                &default_registry(), &HashMap::new(),
                std::path::Path::new("/tmp"), 0,
            ).unwrap();
            assert!(reply.text.contains("Unknown:"), "Expected Unknown: for {}", cmd);
        }
    }

    #[test]
    fn model_command_sets_shows_and_resets_backend_override() {
        let chat_states = Arc::new(Mutex::new(HashMap::new()));
        let state_path = std::env::temp_dir().join(format!(
            "telegram_model_state_{}_{}.json",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));

        let set_reply = TelegramClient::handle_command(
            77,
            "/model claude-sonnet-4-6",
            None,
            &chat_states,
            &state_path,
            &default_registry(),
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();
        assert!(!set_reply.text.contains("CodingAgent:"));
        assert!(set_reply.text.contains("Model: [claude-sonnet-4-6]"));
        assert!(set_reply.text.contains("Source: [chat override]"));

        let show_reply = TelegramClient::handle_command(
            77,
            "/model",
            None,
            &chat_states,
            &state_path,
            &default_registry(),
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();
        assert!(show_reply.text.contains("Model: [claude-sonnet-4-6]"));
        assert!(show_reply
            .text
            .contains("Catalog: [configure via telegram_config.json"));
        assert!(show_reply.text.contains("Choices: [claude-sonnet-4-6"));
        assert_eq!(
            show_reply.reply_markup.as_ref().unwrap()["keyboard"][0][0],
            "/model claude-sonnet-4-6"
        );

        let reset_reply = TelegramClient::handle_command(
            77,
            "/model reset",
            None,
            &chat_states,
            &state_path,
            &default_registry(),
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();
        assert!(reset_reply.text.contains("Model: [auto]"));
        assert!(reset_reply.text.contains("Source: [backend auto]"));
        assert_eq!(
            reset_reply.reply_markup.as_ref().unwrap()["remove_keyboard"],
            true
        );
    }

    #[test]
    fn model_menu_resolves_numeric_selection_including_reset() {
        // model_choices are operator-configured. Build a codex registry that
        // mirrors the sample telegram_config.json entries for this test.
        let mut registry = default_registry();
        registry.merge_config_value(Some(&serde_json::json!({
            "backends": {
                "codex": {
                    "model_choices": [
                        {"value": "gpt-5.4"},
                        {"value": "gpt-5.3-codex"},
                        {"value": "gpt-5-codex"},
                        {"value": "codex-mini-latest"}
                    ]
                }
            }
        })));

        let chat_states = Arc::new(Mutex::new(HashMap::new()));
        let state_path = std::env::temp_dir().join(format!(
            "telegram_model_numeric_{}_{}.json",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));

        let _ = TelegramClient::handle_command(
            77,
            "/model",
            None,
            &chat_states,
            &state_path,
            &registry,
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();

        let state = TelegramClient::load_chat_state_snapshot(&chat_states, 77);
        assert_eq!(
            TelegramClient::pending_menu_command(&state, "1", &registry).as_deref(),
            Some("/model gpt-5.4")
        );
        assert_eq!(
            TelegramClient::pending_menu_command(&state, "2", &registry).as_deref(),
            Some("/model gpt-5.3-codex")
        );
        assert_eq!(
            TelegramClient::pending_menu_command(&state, "5", &registry).as_deref(),
            Some("/model reset")
        );
    }

    #[test]
    fn custom_backend_model_choices_are_shown_in_model_menu() {
        let mut registry = default_registry();
        registry.merge_config_value(Some(&serde_json::json!({
            "backends": {
                "custom_agent": {
                    "model_choices_source_label": "custom backend menu",
                    "model_choices": [
                        "alpha",
                        { "value": "beta-fast", "label": "beta", "description": "fast tier" }
                    ]
                }
            }
        })));

        let chat_states: Arc<Mutex<HashMap<i64, TelegramChatState>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let state_path = std::env::temp_dir().join(format!(
            "telegram_model_custom_state_{}_{}.json",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));

        // Set backend directly since /coding_agent is no longer a user-facing command.
        {
            let mut states = chat_states.lock().unwrap();
            states.entry(77).or_default().cli_backend = backend("custom_agent");
        }

        let reply = TelegramClient::handle_command(
            77,
            "/model",
            None,
            &chat_states,
            &state_path,
            &registry,
            &HashMap::new(),
            std::path::Path::new("/tmp"),
            0,
        )
        .unwrap();

        assert!(reply.text.contains("Catalog: [custom backend menu]"));
        assert!(reply.text.contains("Choices: [alpha | beta -> beta-fast]"));
        assert_eq!(
            reply.reply_markup.as_ref().unwrap()["keyboard"][0][0],
            "/model alpha"
        );
        assert_eq!(
            reply.reply_markup.as_ref().unwrap()["keyboard"][0][1],
            "/model beta-fast"
        );
    }

    #[test]
    fn config_driven_codex_response_and_usage_are_parsed() {
        let output = concat!(
            "{\"type\":\"thread.started\",\"thread_id\":\"abc\"}\n",
            "{\"type\":\"item.completed\",\"item\":{\"id\":\"item_0\",\"type\":\"agent_message\",\"text\":\"HELLO\"}}\n",
            "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":1}}\n"
        );
        let registry = default_registry();
        let codex = backend("codex");

        assert_eq!(
            TelegramClient::extract_cli_response_text(&registry, &codex, output, "").as_deref(),
            Some("HELLO")
        );
        let output = "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":12,\"cached_input_tokens\":3,\"output_tokens\":4}}\n";
        let usage =
            TelegramClient::extract_cli_actual_usage(&registry, &codex, output, "").unwrap();
        assert_eq!(usage.input_tokens, 12);
        assert_eq!(usage.cached_input_tokens, 3);
        assert_eq!(usage.output_tokens, 4);
        assert_eq!(usage.total_tokens, 16);
    }

    #[test]
    fn telegram_message_id_is_extracted_from_send_message_response() {
        let body = r#"{"ok":true,"result":{"message_id":77,"text":"hello"}}"#;

        assert_eq!(TelegramClient::extract_telegram_message_id(body), Some(77));
    }

    #[test]
    fn chat_action_payload_contains_chat_id_and_action() {
        let payload = TelegramClient::build_chat_action_payload(77, "typing");
        let value: serde_json::Value = serde_json::from_str(&payload).unwrap();

        assert_eq!(value["chat_id"].as_i64(), Some(77));
        assert_eq!(value["action"].as_str(), Some("typing"));
    }

    #[tokio::test]
    async fn typing_indicator_helper_returns_response_even_without_token() {
        let response = TelegramClient::wait_with_typing_indicator("", 77, async {
            tokio::time::sleep(Duration::from_millis(5)).await;
            "done".to_string()
        })
        .await;

        assert_eq!(response, "done");
    }

    #[test]
    fn cli_streaming_message_mentions_progress_and_project() {
        let state = TelegramChatState::default();
        let message = TelegramClient::build_cli_streaming_message(
            &state,
            &backend("codex"),
            std::path::Path::new("/tmp/project"),
            "running",
            15,
            None,
            None,
        );

        assert!(message.contains("Backend: [codex]"));
        assert!(message.contains("Status: [running]"));
        assert!(message.contains("Session: [0001]"));
        assert!(message.contains("Project: [/tmp/project]"));
        assert!(message.contains("Elapsed: [15s]"));
        assert!(message.contains("LastOutput: [waiting]"));
        assert!(message.contains("waiting..."));
    }

    #[test]
    fn cli_streaming_message_includes_latest_output_summary() {
        let state = TelegramChatState {
            interaction_mode: TelegramInteractionMode::Coding,
            ..TelegramChatState::default()
        };
        let message = TelegramClient::build_cli_streaming_message(
            &state,
            &backend("claude"),
            std::path::Path::new("/tmp/project"),
            "completed",
            22,
            Some(3),
            Some("Third line extends the response"),
        );

        assert!(message.contains("Backend: [claude]"));
        assert!(message.contains("Status: [completed]"));
        assert!(message.contains("LastOutput: [3s ago]"));
        assert!(message.contains("Output:"));
        assert!(message.contains("Third line extends the response"));
    }

    #[test]
    fn incremental_cli_response_uses_new_text_delta() {
        let registry = default_registry();
        let stdout = "First line of output\nSecond line of output with enough detail";
        let partial = TelegramClient::extract_incremental_cli_response(
            &registry,
            &backend("claude"),
            stdout,
            "",
            "",
        )
        .unwrap();
        assert!(partial.contains("First line of output"));

        let next_stdout = format!(
            "{}\nThird line extends the response with more useful detail",
            stdout
        );
        let partial = TelegramClient::extract_incremental_cli_response(
            &registry,
            &backend("claude"),
            &next_stdout,
            "",
            stdout,
        )
        .unwrap();
        assert!(partial.contains("Third line extends the response"));
    }

    #[test]
    fn codex_invocation_uses_json_mode_and_project_directory() {
        let state = TelegramChatState {
            interaction_mode: TelegramInteractionMode::Coding,
            cli_backend: backend("codex"),
            execution_mode: TelegramExecutionMode::Plan,
            auto_approve: false,
            project_dir: None,
            model_overrides: HashMap::new(),
            chat_session_index: 1,
            coding_session_index: 1,
            usage: HashMap::new(),
            pending_menu: None,
        };
        let backend_paths = HashMap::from([(backend("codex"), "/usr/bin/codex".to_string())]);
        let (binary, args) = TelegramClient::build_cli_invocation(
            77,
            &state,
            std::path::Path::new("/tmp/project"),
            &default_registry(),
            &backend_paths,
            "hello",
        )
        .unwrap();

        assert_eq!(binary, "/usr/bin/codex");
        assert!(args.iter().any(|arg| arg == "--json"));
        assert!(args.iter().any(|arg| arg == "--full-auto"));
        assert!(args.iter().any(|arg| arg == "--skip-git-repo-check"));
        let cd_index = args.iter().position(|arg| arg == "-C").unwrap();
        assert_eq!(args[cd_index + 1], "/tmp/project");
    }

    #[test]
    fn gemini_invocation_uses_explicit_model() {
        let state = TelegramChatState {
            interaction_mode: TelegramInteractionMode::Coding,
            cli_backend: backend("gemini"),
            execution_mode: TelegramExecutionMode::Plan,
            auto_approve: false,
            project_dir: None,
            model_overrides: HashMap::new(),
            chat_session_index: 1,
            coding_session_index: 1,
            usage: HashMap::new(),
            pending_menu: None,
        };
        let backend_paths = HashMap::from([(backend("gemini"), "/snap/bin/gemini".to_string())]);

        let (binary, args) = TelegramClient::build_cli_invocation(
            77,
            &state,
            std::path::Path::new("/tmp/project"),
            &default_registry(),
            &backend_paths,
            "hello",
        )
        .unwrap();

        assert_eq!(binary, "/snap/bin/gemini");
        let model_index = args.iter().position(|arg| arg == "--model").unwrap();
        assert_eq!(args[model_index + 1], "gemini-2.5-flash");
        assert!(args.iter().any(|arg| arg == "--prompt"));
        assert!(args.iter().any(|arg| arg == "--output-format"));
        let output_index = args
            .iter()
            .position(|arg| arg == "--output-format")
            .unwrap();
        assert_eq!(args[output_index + 1], "json");
    }

    #[test]
    fn codex_and_claude_invocations_include_model_override_when_set() {
        let codex_state = TelegramChatState {
            interaction_mode: TelegramInteractionMode::Coding,
            cli_backend: backend("codex"),
            execution_mode: TelegramExecutionMode::Plan,
            auto_approve: false,
            project_dir: None,
            model_overrides: HashMap::from([("codex".to_string(), "gpt-5-codex".to_string())]),
            chat_session_index: 1,
            coding_session_index: 1,
            usage: HashMap::new(),
            pending_menu: None,
        };
        let codex_paths = HashMap::from([(backend("codex"), "/usr/bin/codex".to_string())]);
        let (_, codex_args) = TelegramClient::build_cli_invocation(
            77,
            &codex_state,
            std::path::Path::new("/tmp/project"),
            &default_registry(),
            &codex_paths,
            "hello",
        )
        .unwrap();
        let codex_model_index = codex_args.iter().position(|arg| arg == "--model").unwrap();
        assert_eq!(codex_args[codex_model_index + 1], "gpt-5-codex");

        let claude_state = TelegramChatState {
            interaction_mode: TelegramInteractionMode::Coding,
            cli_backend: backend("claude"),
            execution_mode: TelegramExecutionMode::Plan,
            auto_approve: false,
            project_dir: None,
            model_overrides: HashMap::from([(
                "claude".to_string(),
                "claude-sonnet-4-6".to_string(),
            )]),
            chat_session_index: 1,
            coding_session_index: 1,
            usage: HashMap::new(),
            pending_menu: None,
        };
        let claude_paths = HashMap::from([(backend("claude"), "/usr/bin/claude".to_string())]);
        let (_, claude_args) = TelegramClient::build_cli_invocation(
            77,
            &claude_state,
            std::path::Path::new("/tmp/project"),
            &default_registry(),
            &claude_paths,
            "hello",
        )
        .unwrap();
        let claude_model_index = claude_args.iter().position(|arg| arg == "--model").unwrap();
        assert_eq!(claude_args[claude_model_index + 1], "claude-sonnet-4-6");
    }

    #[test]
    fn gemini_json_response_and_usage_are_parsed() {
        let output = r#"{
  "session_id": "gemini-session",
  "response": "OK",
  "stats": {
    "models": {
      "gemini-2.5-flash": {
        "tokens": {
          "input": 10,
          "prompt": 10,
          "candidates": 2,
          "total": 15,
          "cached": 1,
          "thoughts": 3,
          "tool": 4
        }
      }
    }
  }
}"#;

        let registry = default_registry();
        let gemini = backend("gemini");
        assert_eq!(
            TelegramClient::extract_cli_response_text(&registry, &gemini, output, "").as_deref(),
            Some("OK")
        );
        let usage =
            TelegramClient::extract_cli_actual_usage(&registry, &gemini, output, "").unwrap();
        assert_eq!(usage.session_id.as_deref(), Some("gemini-session"));
        assert_eq!(usage.model.as_deref(), Some("gemini-2.5-flash"));
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 2);
        assert_eq!(usage.total_tokens, 15);
        assert_eq!(usage.cached_input_tokens, 1);
        assert_eq!(usage.thought_tokens, 3);
        assert_eq!(usage.tool_tokens, 4);
    }

    #[test]
    fn claude_json_response_and_usage_are_parsed() {
        let output = r#"{
  "result": "DONE",
  "session_id": "claude-session",
  "usage": {
    "input_tokens": 5,
    "output_tokens": 7,
    "cache_creation_input_tokens": 11,
    "cache_read_input_tokens": 13
  },
  "modelUsage": {
    "claude-sonnet-4-6": {
      "inputTokens": 5
    }
  }
}"#;

        let registry = default_registry();
        let claude = backend("claude");
        assert_eq!(
            TelegramClient::extract_cli_response_text(&registry, &claude, output, "").as_deref(),
            Some("DONE")
        );
        let usage =
            TelegramClient::extract_cli_actual_usage(&registry, &claude, output, "").unwrap();
        assert_eq!(usage.session_id.as_deref(), Some("claude-session"));
        assert_eq!(usage.model.as_deref(), Some("claude-sonnet-4-6"));
        assert_eq!(usage.input_tokens, 5);
        assert_eq!(usage.output_tokens, 7);
        assert_eq!(usage.total_tokens, 12);
        assert_eq!(usage.cache_creation_input_tokens, 11);
        assert_eq!(usage.cache_read_input_tokens, 13);
    }

    #[test]
    fn coding_usage_report_includes_actual_cli_tokens() {
        let mut state = TelegramChatState {
            interaction_mode: TelegramInteractionMode::Coding,
            cli_backend: backend("gemini"),
            execution_mode: TelegramExecutionMode::Plan,
            auto_approve: false,
            project_dir: None,
            model_overrides: HashMap::new(),
            chat_session_index: 1,
            coding_session_index: 2,
            usage: HashMap::new(),
            pending_menu: None,
        };
        let mut usage = TelegramCliUsageStats::default();
        usage.requests = 2;
        usage.successes = 2;
        usage.total_duration_ms = 120;
        usage.record_actual_usage(
            TelegramCliActualUsage {
                input_tokens: 10,
                output_tokens: 2,
                total_tokens: 15,
                cached_input_tokens: 1,
                thought_tokens: 3,
                model: Some("gemini-2.5-flash".to_string()),
                session_id: Some("gemini-session".to_string()),
                ..TelegramCliActualUsage::default()
            },
            123456,
        );
        state
            .usage
            .insert(backend("gemini").as_str().to_string(), usage);

        let report = TelegramClient::format_coding_usage_report(
            &state,
            &backend("gemini"),
            &default_registry(),
        );
        assert!(report.contains("Mode: [backend]"));
        assert!(report.contains("Session: [0002]"));
        assert!(report.contains("Backend: [gemini]"));
        assert!(report.contains("ModelSource: [backend default]"));
        assert!(report.contains("Source: [stats.models.<model>.tokens]"));
        assert!(report.contains("Refresh: [updates after the next successful Gemini run]"));
        assert!(report.contains("LatestCLI: [gemini-session]"));
        assert!(report.contains("Model: [gemini-2.5-flash]"));
        assert!(report.contains("Latest: [in 10 | out 2 | total 15]"));
        assert!(report.contains("Cached: [1]"));
        assert!(report.contains("Thought: [3]"));
        assert!(report.contains("Remaining: [not reported by Gemini CLI]"));
        assert!(report.contains("Reset: [not reported by Gemini CLI]"));
        assert!(report.contains("TotalThought: [3]"));
    }

    #[test]
    fn gemini_capacity_errors_are_summarized() {
        let registry = default_registry();
        let message = TelegramClient::format_cli_result(
            &registry,
            &backend("gemini"),
            1,
            100,
            "",
            "No capacity available for model gemini-3-flash-preview",
        );

        assert!(message.contains("[gemini] Model capacity reached."));
        assert!(message.contains("gemini-2.5-flash"));
    }

    #[test]
    fn custom_backend_invocation_and_usage_can_be_loaded_from_config() {
        let mut registry = default_registry();
        registry.merge_config_value(Some(&serde_json::json!({
            "backends": {
                "custom_agent": {
                    "binary_path": "/usr/bin/custom-agent",
                    "usage_hint": "`custom-agent run --cwd <project> --prompt <prompt>`",
                    "invocation": {
                        "args": ["run", "--cwd", "{project_dir}", "--prompt", "{prompt}"]
                    },
                    "response_extractors": [
                        { "source": "stdout", "format": "json", "text_path": "reply" }
                    ],
                    "usage_extractors": [
                        {
                            "source": "stdout",
                            "format": "json",
                            "input_tokens_path": "usage.prompt",
                            "output_tokens_path": "usage.completion",
                            "total_tokens_path": "usage.total",
                            "session_id_path": "session"
                        }
                    ]
                }
            }
        })));

        let state = TelegramChatState {
            interaction_mode: TelegramInteractionMode::Coding,
            cli_backend: backend("custom_agent"),
            execution_mode: TelegramExecutionMode::Plan,
            auto_approve: false,
            project_dir: None,
            model_overrides: HashMap::new(),
            chat_session_index: 1,
            coding_session_index: 1,
            usage: HashMap::new(),
            pending_menu: None,
        };
        let backend_paths =
            HashMap::from([(backend("custom_agent"), "/usr/bin/custom-agent".to_string())]);

        let (binary, args) = TelegramClient::build_cli_invocation(
            77,
            &state,
            std::path::Path::new("/tmp/project"),
            &registry,
            &backend_paths,
            "hello",
        )
        .unwrap();
        assert_eq!(binary, "/usr/bin/custom-agent");
        assert_eq!(args[0], "run");
        assert!(args.iter().any(|arg| arg == "/tmp/project"));

        let stdout =
            r#"{"reply":"DONE","session":"sess-1","usage":{"prompt":4,"completion":6,"total":10}}"#;
        assert_eq!(
            TelegramClient::extract_cli_response_text(
                &registry,
                &backend("custom_agent"),
                stdout,
                ""
            )
            .as_deref(),
            Some("DONE")
        );
        let usage = TelegramClient::extract_cli_actual_usage(
            &registry,
            &backend("custom_agent"),
            stdout,
            "",
        )
        .unwrap();
        assert_eq!(usage.input_tokens, 4);
        assert_eq!(usage.output_tokens, 6);
        assert_eq!(usage.total_tokens, 10);
        assert_eq!(usage.session_id.as_deref(), Some("sess-1"));
    }

    #[test]
    fn llm_config_gemini_model_is_used_as_fallback() {
        let temp_root = std::env::temp_dir().join(format!(
            "telegram_gemini_model_{}_{}",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));
        std::fs::create_dir_all(&temp_root).unwrap();
        std::fs::write(
            temp_root.join("llm_config.json"),
            r#"{"backends":{"gemini":{"model":"gemini-2.5-pro"}}}"#,
        )
        .unwrap();

        let mut cli_backends = default_registry();
        if let Some(definition) = cli_backends.definitions.get_mut(&backend("gemini")) {
            definition.model = None;
        }
        TelegramClient::read_backend_models_from_llm_config(&temp_root, &mut cli_backends);

        assert_eq!(
            cli_backends
                .get(&backend("gemini"))
                .and_then(|definition| definition.model.as_deref()),
            Some("gemini-2.5-pro")
        );

        let _ = std::fs::remove_file(temp_root.join("llm_config.json"));
        let _ = std::fs::remove_dir(&temp_root);
    }

    #[test]
    fn llm_config_telegram_section_overrides_model_choices() {
        let temp_root = std::env::temp_dir().join(format!(
            "telegram_model_choices_{}_{}",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));
        std::fs::create_dir_all(&temp_root).unwrap();
        std::fs::write(
            temp_root.join("llm_config.json"),
            r#"{
                "telegram": {
                    "cli_backends": {
                        "backends": {
                            "gemini": {
                                "model_choices": [
                                    {"value": "gemini-2.5-pro", "label": "Pro", "description": "Best quality"},
                                    {"value": "gemini-2.5-flash", "label": "Flash", "description": "Faster"}
                                ]
                            }
                        }
                    }
                }
            }"#,
        )
        .unwrap();

        let mut cli_backends = default_registry();
        TelegramClient::read_backend_models_from_llm_config(&temp_root, &mut cli_backends);

        let choices = &cli_backends
            .get(&backend("gemini"))
            .unwrap()
            .model_choices;
        assert_eq!(choices.len(), 2);
        assert_eq!(choices[0].value, "gemini-2.5-pro");
        assert_eq!(choices[1].value, "gemini-2.5-flash");

        let _ = std::fs::remove_file(temp_root.join("llm_config.json"));
        let _ = std::fs::remove_dir(&temp_root);
    }

    #[test]
    fn startup_targets_include_allowed_chat_ids_without_saved_state() {
        let chat_states = Arc::new(Mutex::new(HashMap::new()));
        let allowed = Arc::new(HashSet::from([12345_i64]));
        let targets = TelegramClient::startup_notification_targets(&allowed, &chat_states);

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].0, 12345);
        assert_eq!(
            targets[0]
                .1
                .session_label_for(TelegramInteractionMode::Chat),
            "chat-0001"
        );
    }

    #[test]
    fn new_session_increments_current_mode_counter() {
        let state_path = std::env::temp_dir().join(format!(
            "telegram_state_{}_{}.json",
            std::process::id(),
            TelegramClient::current_timestamp_millis()
        ));
        let chat_states = Arc::new(Mutex::new(HashMap::new()));

        let first = TelegramClient::start_new_session(&chat_states, &state_path, 77);
        assert!(first.text.contains("Session: [0002]"));

        {
            let mut states = chat_states.lock().unwrap();
            let state = states.entry(77).or_default();
            state.interaction_mode = TelegramInteractionMode::Coding;
        }

        let second = TelegramClient::start_new_session(&chat_states, &state_path, 77);
        assert!(second.text.contains("Session: [0002]"));
    }

    #[test]
    fn from_config_accepts_single_chat_id() {
        let client = TelegramClient::from_config(&json!({
            "bot_token": "token",
            "chat_id": "12345",
            "max_message_chars": 128
        }))
        .expect("telegram config");

        assert!(client.allowed_chat_ids.contains(&12345));
        assert_eq!(client.max_message_chars, 128);
    }
}
