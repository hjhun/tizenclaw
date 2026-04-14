impl TelegramClient {
    fn build_send_message_payload(chat_id: i64, text: &str, reply_markup: Option<Value>) -> String {
        let mut payload = json!({
            "chat_id": chat_id,
            "text": text
        });

        if let Some(reply_markup) = reply_markup {
            payload["reply_markup"] = reply_markup;
        }

        payload.to_string()
    }

    fn build_edit_message_payload(
        chat_id: i64,
        message_id: i64,
        text: &str,
        reply_markup: Option<Value>,
    ) -> String {
        let mut payload = json!({
            "chat_id": chat_id,
            "message_id": message_id,
            "text": text
        });

        if let Some(reply_markup) = reply_markup {
            payload["reply_markup"] = reply_markup;
        }

        payload.to_string()
    }

    fn build_chat_action_payload(chat_id: i64, action: &str) -> String {
        json!({
            "chat_id": chat_id,
            "action": action
        })
        .to_string()
    }

    fn command_menu_entries() -> Vec<(&'static str, &'static str)> {
        vec![
            ("select", "Switch mode"),
            ("coding_agent", "Choose backend"),
            ("devel", "Queue devel prompt"),
            ("devel_result", "Read latest devel result"),
            ("model", "Choose model"),
            ("project", "Set project path"),
            ("new_session", "Start new session"),
            ("usage", "Show usage"),
            ("mode", "Choose plan or fast"),
            ("status", "Show current state"),
            ("auto_approve", "Toggle auto approve"),
        ]
    }

    fn build_set_my_commands_payload() -> String {
        let commands: Vec<Value> = Self::command_menu_entries()
            .into_iter()
            .map(|(command, description)| {
                json!({
                    "command": command,
                    "description": description
                })
            })
            .collect();

        json!({
            "commands": commands
        })
        .to_string()
    }

    fn build_reply_keyboard(rows: &[&[&str]]) -> Value {
        let keyboard: Vec<Vec<Value>> = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|entry| Value::String((*entry).to_string()))
                    .collect()
            })
            .collect();

        json!({
            "keyboard": keyboard,
            "resize_keyboard": true,
            "one_time_keyboard": true
        })
    }

    fn build_owned_reply_keyboard(rows: &[Vec<String>]) -> Value {
        let keyboard: Vec<Vec<Value>> = rows
            .iter()
            .map(|row| row.iter().cloned().map(Value::String).collect())
            .collect();

        json!({
            "keyboard": keyboard,
            "resize_keyboard": true,
            "one_time_keyboard": true
        })
    }

    fn remove_keyboard_markup() -> Value {
        json!({
            "remove_keyboard": true
        })
    }

    fn select_keyboard() -> Value {
        Self::build_reply_keyboard(&[&["/select chat", "/select coding"]])
    }

    fn cli_backend_keyboard(cli_backends: &TelegramCliBackendRegistry) -> Value {
        let rows = cli_backends
            .backends()
            .map(|backend| vec![format!("/coding_agent {}", backend.as_str())])
            .collect::<Vec<_>>();
        let row_refs = rows
            .iter()
            .map(|row| row.iter().map(String::as_str).collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let borrowed = row_refs.iter().map(Vec::as_slice).collect::<Vec<_>>();
        Self::build_reply_keyboard(&borrowed)
    }

    fn available_model_choices(
        state: &TelegramChatState,
        backend: &TelegramCliBackend,
        cli_backends: &TelegramCliBackendRegistry,
    ) -> (Vec<TelegramCliModelChoice>, String) {
        let definition = cli_backends.get(backend);
        let mut choices = Vec::new();
        let mut seen = HashSet::new();

        if let Some(current) = state.effective_cli_model(backend, cli_backends) {
            Self::push_model_choice(
                &mut choices,
                &mut seen,
                TelegramCliModelChoice::simple(&current),
            );
        }

        if let Some(definition) = definition {
            for choice in definition.model_choices.iter().cloned() {
                Self::push_model_choice(&mut choices, &mut seen, choice);
            }
        }

        if choices.is_empty() {
            Self::push_model_choice(
                &mut choices,
                &mut seen,
                TelegramCliModelChoice::simple("auto"),
            );
        }

        let source = definition
            .map(|definition| definition.model_choices_source_label.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or("configured backend model choices")
            .to_string();

        (choices, source)
    }

    fn push_model_choice(
        choices: &mut Vec<TelegramCliModelChoice>,
        seen: &mut HashSet<String>,
        choice: TelegramCliModelChoice,
    ) {
        let trimmed = choice.value.trim();
        if trimmed.is_empty() {
            return;
        }

        let normalized = choice.normalized_value();
        if seen.insert(normalized) {
            choices.push(TelegramCliModelChoice {
                value: trimmed.to_string(),
                label: choice.label,
                description: choice.description,
            });
        }
    }

    fn model_keyboard(choices: &[TelegramCliModelChoice]) -> Value {
        let mut rows = Vec::new();
        let mut current_row = Vec::new();

        for choice in choices {
            current_row.push(format!("/model {}", choice.value.trim()));
            if current_row.len() == 2 {
                rows.push(std::mem::take(&mut current_row));
            }
        }

        if !current_row.is_empty() {
            rows.push(current_row);
        }

        rows.push(vec!["/model reset".to_string()]);
        Self::build_owned_reply_keyboard(&rows)
    }

    fn format_model_menu_text(
        state: &TelegramChatState,
        backend: &TelegramCliBackend,
        cli_backends: &TelegramCliBackendRegistry,
    ) -> String {
        let model = state
            .effective_cli_model(backend, cli_backends)
            .unwrap_or_else(|| "auto".to_string());
        let source = state.effective_cli_model_source(backend, cli_backends);
        let (choices, catalog_source) = Self::available_model_choices(state, backend, cli_backends);
        let choices_text = choices
            .iter()
            .map(TelegramCliModelChoice::summary_text)
            .collect::<Vec<_>>()
            .join(" | ");

        format!(
            "CodingAgent: {}\nModel: {}\nSource: {}\nCatalog: {}\nChoices: {}\nUse: /model [name] | /model reset",
            Self::backend_label(backend),
            Self::value_label(model),
            Self::value_label(source),
            Self::value_label(catalog_source),
            Self::value_label(choices_text)
        )
    }

    fn mode_keyboard() -> Value {
        Self::build_reply_keyboard(&[&["/mode plan", "/mode fast"]])
    }

    fn auto_approve_keyboard() -> Value {
        Self::build_reply_keyboard(&[&["/auto_approve on", "/auto_approve off"]])
    }

    fn register_bot_commands(bot_token: &str) {
        if bot_token.is_empty() {
            return;
        }

        let url = format!("https://api.telegram.org/bot{}/setMyCommands", bot_token);
        let payload = Self::build_set_my_commands_payload();
        let client = crate::infra::http_client::HttpClient::new();

        match client.post_sync(&url, &payload) {
            Ok(_) => log::info!("Telegram bot commands registered"),
            Err(err) => log::warn!("Telegram setMyCommands failed: {}", err),
        }
    }

    async fn post_telegram_api(
        bot_token: &str,
        method: &str,
        payload: String,
    ) -> Result<Value, String> {
        if bot_token.is_empty() {
            return Err("Telegram bot token is empty.".to_string());
        }

        let url = format!("https://api.telegram.org/bot{}/{}", bot_token, method);
        let client = crate::infra::http_client::HttpClient::new();
        let response = client
            .post(&url, &payload)
            .await
            .map_err(|err| format!("Telegram {} failed: {}", method, err))?;
        let value = serde_json::from_str::<Value>(&response.body)
            .map_err(|err| format!("Telegram {} returned invalid JSON: {}", method, err))?;

        if value.get("ok").and_then(Value::as_bool) == Some(false) {
            let description = value
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("Telegram API request failed.");
            return Err(description.to_string());
        }

        Ok(value)
    }

    fn extract_telegram_message_id(body: &str) -> Option<i64> {
        serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|value| Self::telegram_message_id_from_value(&value))
    }

    fn telegram_message_id_from_value(value: &Value) -> Option<i64> {
        value
            .get("result")
            .and_then(|result| result.get("message_id"))
            .and_then(Value::as_i64)
    }

    async fn send_telegram_message_and_get_id(
        bot_token: &str,
        chat_id: i64,
        message: &TelegramOutgoingMessage,
    ) -> Result<i64, String> {
        let safe_text = Self::truncate_chars(&message.text, 4000);
        let payload =
            Self::build_send_message_payload(chat_id, &safe_text, message.reply_markup.clone());
        let value = Self::post_telegram_api(bot_token, "sendMessage", payload).await?;

        Self::telegram_message_id_from_value(&value)
            .or_else(|| Self::extract_telegram_message_id(&value.to_string()))
            .ok_or_else(|| "Telegram sendMessage response did not include message_id.".to_string())
    }

    async fn edit_telegram_message(
        bot_token: &str,
        chat_id: i64,
        message_id: i64,
        message: &TelegramOutgoingMessage,
    ) -> Result<(), String> {
        let safe_text = Self::truncate_chars(&message.text, 4000);
        let payload = Self::build_edit_message_payload(
            chat_id,
            message_id,
            &safe_text,
            message.reply_markup.clone(),
        );

        match Self::post_telegram_api(bot_token, "editMessageText", payload).await {
            Ok(_) => Ok(()),
            Err(err) if err.contains("message is not modified") => Ok(()),
            Err(err) => Err(err),
        }
    }

    async fn send_telegram_chat_action(
        bot_token: &str,
        chat_id: i64,
        action: &str,
    ) -> Result<(), String> {
        let payload = Self::build_chat_action_payload(chat_id, action);
        Self::post_telegram_api(bot_token, "sendChatAction", payload)
            .await
            .map(|_| ())
    }

    async fn wait_with_typing_indicator<F>(
        bot_token: &str,
        chat_id: i64,
        response_future: F,
    ) -> String
    where
        F: Future<Output = String>,
    {
        let _ = Self::send_telegram_chat_action(bot_token, chat_id, "typing").await;

        tokio::pin!(response_future);
        let mut typing_heartbeat =
            tokio::time::interval(Duration::from_secs(TELEGRAM_CHAT_ACTION_UPDATE_SECS));
        typing_heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        typing_heartbeat.tick().await;

        loop {
            tokio::select! {
                response = &mut response_future => return response,
                _ = typing_heartbeat.tick() => {
                    let _ = Self::send_telegram_chat_action(bot_token, chat_id, "typing").await;
                }
            }
        }
    }

    // Static so it can be called inside spawned async tasks easily
    fn send_telegram_message(bot_token: &str, chat_id: i64, message: &TelegramOutgoingMessage) {
        if bot_token.is_empty() {
            return;
        }

        Self::send_telegram_chunks(
            bot_token,
            chat_id,
            &message.text,
            TELEGRAM_MAX_MESSAGE_CHARS,
            message.reply_markup.clone(),
        );
    }

    fn send_telegram_chunks(
        bot_token: &str,
        chat_id: i64,
        text: &str,
        max_message_chars: usize,
        reply_markup: Option<Value>,
    ) {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", bot_token);
        let chunks = split_message_chunks(text, max_message_chars.max(1));

        std::thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(err) => {
                    log::warn!("Telegram send runtime init failed: {}", err);
                    return;
                }
            };

            for (index, chunk) in chunks.into_iter().enumerate() {
                let payload = TelegramClient::build_send_message_payload(
                    chat_id,
                    &chunk,
                    if index == 0 { reply_markup.clone() } else { None },
                );

                let send_result = runtime.block_on(async {
                    let client = reqwest::Client::builder()
                        .timeout(Duration::from_secs(10))
                        .build()
                        .map_err(|err| err.to_string())?;
                    let response = client
                        .post(&url)
                        .header("Content-Type", "application/json")
                        .body(payload)
                        .send()
                        .await
                        .map_err(|err| err.to_string())?;
                    if response.status().is_success() {
                        Ok(())
                    } else {
                        Err(format!("HTTP {}", response.status().as_u16()))
                    }
                });

                if let Err(err) = send_result {
                    log::warn!("Telegram sendMessage failed: {}", err);
                }
            }
        });
    }

    fn supported_commands_text(cli_backends: &TelegramCliBackendRegistry) -> String {
        let backend_choices = cli_backends.backend_choices_text();
        [
            "Commands",
            "Development requests can be sent directly in normal chat.",
            "/select [chat|coding]",
            &format!("/coding_agent [{}]", backend_choices),
            "/devel [prompt]",
            "/devel_result",
            "/model [name|list|reset]",
            "/project [path]",
            "/project reset",
            "/new_session",
            "/usage",
            "/mode [plan|fast]",
            "/status",
            "/auto_approve [on|off]",
        ]
        .join("\n")
    }

    fn value_label(value: impl AsRef<str>) -> String {
        format!("[{}]", value.as_ref())
    }

    fn backend_label(backend: &TelegramCliBackend) -> String {
        Self::value_label(backend.as_str())
    }

    fn session_number(session_label: &str) -> &str {
        session_label
            .rsplit('-')
            .next()
            .filter(|value| !value.is_empty())
            .unwrap_or(session_label)
    }

    fn session_value_label(session_label: &str) -> String {
        Self::value_label(Self::session_number(session_label))
    }

    fn active_session_value_label(state: &TelegramChatState) -> String {
        Self::session_value_label(&state.active_session_label())
    }

    fn usage_capture_label(captured_at_ms: Option<u64>) -> String {
        captured_at_ms
            .map(|captured_at_ms| {
                let age_secs =
                    Self::current_timestamp_millis().saturating_sub(captured_at_ms) / 1000;
                format!("captured {}s ago", age_secs)
            })
            .unwrap_or_else(|| "not captured yet".to_string())
    }

    fn session_value_label_for_mode(
        state: &TelegramChatState,
        mode: TelegramInteractionMode,
    ) -> String {
        Self::session_value_label(&state.session_label_for(mode))
    }

    fn backend_choices_labels_text(cli_backends: &TelegramCliBackendRegistry) -> String {
        cli_backends
            .backends()
            .map(Self::backend_label)
            .collect::<Vec<_>>()
            .join(", ")
    }
}
