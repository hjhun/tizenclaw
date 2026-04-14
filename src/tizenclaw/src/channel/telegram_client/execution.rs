impl TelegramClient {
    pub(crate) async fn run_coding_agent_tool(
        config_dir: &Path,
        request: &CodingAgentToolRequest,
    ) -> Result<Value, String> {
        let (default_cli_workdir, default_timeout_secs, cli_backends, cli_backend_paths) =
            Self::load_coding_agent_runtime(config_dir);
        let mut state = TelegramChatState::default();
        state.auto_approve = request.auto_approve.unwrap_or(false);
        state.execution_mode = request
            .execution_mode
            .as_deref()
            .and_then(TelegramExecutionMode::parse)
            .unwrap_or(TelegramExecutionMode::Plan);
        state.project_dir = request
            .project_dir
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);

        if let Some(backend) = request
            .backend
            .as_deref()
            .and_then(|value| cli_backends.parse(value))
        {
            state.cli_backend = backend;
        }

        let backend = state.effective_cli_backend(&cli_backends);
        if let Some(model) = request
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            state
                .model_overrides
                .insert(backend.as_str().to_string(), model.to_string());
        }

        let effective_cli_workdir = state.effective_cli_workdir(&default_cli_workdir);
        if !effective_cli_workdir.is_dir() {
            return Err(format!(
                "Coding agent project directory '{}' is not available",
                effective_cli_workdir.display()
            ));
        }

        let definition = cli_backends.get(&backend).ok_or_else(|| {
            format!(
                "Selected backend '{}' is not defined in Telegram config.",
                backend.as_str()
            )
        })?;
        let binary = cli_backend_paths.get(&backend).cloned().ok_or_else(|| {
            format!(
                "Selected backend '{}' is not available on PATH.",
                backend.as_str()
            )
        })?;

        let prompt =
            Self::build_tool_cli_prompt(&state, &effective_cli_workdir, &backend, &request.prompt);
        let effective_model = state.effective_cli_model(&backend, &cli_backends);
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
                &effective_cli_workdir,
                effective_model.as_deref(),
                definition.invocation.approval_placeholder.as_deref(),
                approval_value,
            ));
        }

        let timeout_secs = request.timeout_secs.unwrap_or(default_timeout_secs);
        let mut command = tokio::process::Command::new(&binary);
        command.args(&args);
        command.current_dir(&effective_cli_workdir);
        command.env("NO_COLOR", "1");
        command.env("CLICOLOR", "0");
        command.env("TERM", "dumb");
        command.stdin(std::process::Stdio::null());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        command.kill_on_drop(true);

        let started = Instant::now();
        let output = tokio::time::timeout(Duration::from_secs(timeout_secs), command.output())
            .await
            .map_err(|_| {
                format!(
                    "Coding agent '{}' timed out after {}s",
                    backend.as_str(),
                    timeout_secs
                )
            })?
            .map_err(|err| format!("Failed to start '{}': {}", backend.as_str(), err))?;

        let duration_ms = started.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);
        let response_text = Self::format_cli_result(
            &cli_backends,
            &backend,
            exit_code,
            duration_ms,
            &stdout,
            &stderr,
        );
        let actual_usage =
            Self::extract_cli_actual_usage(&cli_backends, &backend, &stdout, &stderr);

        Ok(json!({
            "status": if output.status.success() { "success" } else { "error" },
            "backend": backend.as_str(),
            "project_dir": effective_cli_workdir.display().to_string(),
            "model": effective_model,
            "execution_mode": state.execution_mode.as_str(),
            "auto_approve": state.auto_approve,
            "duration_ms": duration_ms,
            "exit_code": exit_code,
            "response_text": response_text,
            "stdout_tail": Self::truncate_chars(stdout.trim(), 4000),
            "stderr_tail": Self::truncate_chars(stderr.trim(), 4000),
            "usage": actual_usage,
        }))
    }

    fn build_cli_streaming_message(
        state: &TelegramChatState,
        backend: &TelegramCliBackend,
        effective_cli_workdir: &Path,
        phase: &str,
        elapsed_secs: u64,
        last_output_secs: Option<u64>,
        output_text: Option<&str>,
    ) -> String {
        let phase_text = match phase {
            "running" => "running".to_string(),
            "completed" | "failed" => phase.to_string(),
            other if other.starts_with("timed out") => other.to_string(),
            other => other.to_string(),
        };
        let activity =
            last_output_secs.map_or_else(|| "waiting".to_string(), |secs| format!("{}s ago", secs));
        let latest_output = output_text
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| Self::truncate_chars(text, 2600))
            .unwrap_or_else(|| "waiting...".to_string());

        format!(
            "CodingAgent: {}\nStatus: {}\nSession: {}\nProject: {}\nElapsed: {}\nLastOutput: {}\n\nOutput:\n{}",
            Self::backend_label(backend),
            Self::value_label(phase_text),
            Self::session_value_label_for_mode(state, TelegramInteractionMode::Coding),
            Self::value_label(effective_cli_workdir.display().to_string()),
            Self::value_label(format!("{}s", elapsed_secs)),
            Self::value_label(activity),
            latest_output
        )
    }

    fn extract_json_value(text: &str) -> Option<Value> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            serde_json::from_str::<Value>(trimmed).ok()
        }
    }

    fn extract_plain_text(text: &str, reject_json_input: bool) -> Option<String> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        if reject_json_input && (trimmed.starts_with('{') || trimmed.starts_with('[')) {
            return None;
        }
        Some(trimmed.to_string())
    }

    fn output_text_by_source<'a>(
        source: TelegramCliOutputSource,
        stdout: &'a str,
        stderr: &'a str,
    ) -> std::borrow::Cow<'a, str> {
        match source {
            TelegramCliOutputSource::Stdout => std::borrow::Cow::Borrowed(stdout),
            TelegramCliOutputSource::Stderr => std::borrow::Cow::Borrowed(stderr),
            TelegramCliOutputSource::Combined => {
                std::borrow::Cow::Owned(format!("{}\n{}", stdout, stderr))
            }
        }
    }

    fn json_documents(text: &str, format: TelegramCliOutputFormat) -> Vec<Value> {
        match format {
            TelegramCliOutputFormat::Json => Self::extract_json_value(text).into_iter().collect(),
            TelegramCliOutputFormat::JsonLines => {
                text.lines().filter_map(Self::extract_json_value).collect()
            }
            TelegramCliOutputFormat::PlainText => Vec::new(),
        }
    }

    fn value_at_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
        let mut current = value;
        for part in path
            .split('.')
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            if part == "@first_value" {
                current = current.as_object()?.values().next()?;
                continue;
            }
            current = current.get(part)?;
        }
        Some(current)
    }

    fn string_at_path(value: &Value, path: &str) -> Option<String> {
        let value = Self::value_at_path(value, path)?;
        value
            .as_str()
            .map(ToString::to_string)
            .or_else(|| value.as_i64().map(|value| value.to_string()))
            .or_else(|| value.as_u64().map(|value| value.to_string()))
            .or_else(|| value.as_bool().map(|value| value.to_string()))
    }

    fn i64_at_path(value: &Value, path: Option<&str>) -> Option<i64> {
        let path = path?;
        let value = Self::value_at_path(value, path)?;
        value
            .as_i64()
            .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
            .or_else(|| value.as_str().and_then(|value| value.parse::<i64>().ok()))
    }

    fn document_matches(document: &Value, match_fields: &HashMap<String, String>) -> bool {
        match_fields.iter().all(|(path, expected)| {
            Self::string_at_path(document, path)
                .map(|actual| actual == *expected)
                .unwrap_or(false)
        })
    }

    fn extract_response_from_extractor(
        extractor: &TelegramCliResponseExtractor,
        stdout: &str,
        stderr: &str,
    ) -> Option<String> {
        match extractor.format {
            TelegramCliOutputFormat::PlainText => {
                let text = Self::output_text_by_source(extractor.source, stdout, stderr);
                Self::extract_plain_text(&text, extractor.reject_json_input)
            }
            TelegramCliOutputFormat::Json | TelegramCliOutputFormat::JsonLines => {
                let text = Self::output_text_by_source(extractor.source, stdout, stderr);
                let mut matches = Vec::new();
                for document in Self::json_documents(&text, extractor.format) {
                    if !Self::document_matches(&document, &extractor.match_fields) {
                        continue;
                    }
                    let Some(path) = extractor.text_path.as_deref() else {
                        continue;
                    };
                    let Some(text) = Self::string_at_path(&document, path) else {
                        continue;
                    };
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        matches.push(trimmed.to_string());
                    }
                }

                if matches.is_empty() {
                    None
                } else if extractor.join_matches {
                    Some(matches.join("\n\n"))
                } else {
                    matches.pop()
                }
            }
        }
    }

    fn extract_usage_from_extractor(
        extractor: &TelegramCliUsageExtractor,
        stdout: &str,
        stderr: &str,
    ) -> Option<TelegramCliActualUsage> {
        let text = Self::output_text_by_source(extractor.source, stdout, stderr);
        let document = Self::json_documents(&text, extractor.format)
            .into_iter()
            .filter(|document| Self::document_matches(document, &extractor.match_fields))
            .last()?;
        let input_tokens =
            Self::i64_at_path(&document, extractor.input_tokens_path.as_deref()).unwrap_or(0);
        let output_tokens =
            Self::i64_at_path(&document, extractor.output_tokens_path.as_deref()).unwrap_or(0);

        Some(TelegramCliActualUsage {
            input_tokens,
            output_tokens,
            total_tokens: Self::i64_at_path(&document, extractor.total_tokens_path.as_deref())
                .unwrap_or_else(|| input_tokens.saturating_add(output_tokens)),
            cached_input_tokens: Self::i64_at_path(
                &document,
                extractor.cached_input_tokens_path.as_deref(),
            )
            .unwrap_or(0),
            cache_creation_input_tokens: Self::i64_at_path(
                &document,
                extractor.cache_creation_input_tokens_path.as_deref(),
            )
            .unwrap_or(0),
            cache_read_input_tokens: Self::i64_at_path(
                &document,
                extractor.cache_read_input_tokens_path.as_deref(),
            )
            .unwrap_or(0),
            thought_tokens: Self::i64_at_path(&document, extractor.thought_tokens_path.as_deref())
                .unwrap_or(0),
            tool_tokens: Self::i64_at_path(&document, extractor.tool_tokens_path.as_deref())
                .unwrap_or(0),
            model: extractor
                .model_path
                .as_deref()
                .and_then(|path| Self::string_at_path(&document, path))
                .or_else(|| {
                    extractor.model_key_path.as_deref().and_then(|path| {
                        Self::value_at_path(&document, path)?
                            .as_object()?
                            .keys()
                            .next()
                            .cloned()
                    })
                }),
            session_id: extractor
                .session_id_path
                .as_deref()
                .and_then(|path| Self::string_at_path(&document, path)),
            remaining_text: extractor
                .remaining_text_path
                .as_deref()
                .and_then(|path| Self::string_at_path(&document, path)),
            reset_at: extractor
                .reset_at_path
                .as_deref()
                .and_then(|path| Self::string_at_path(&document, path)),
        })
    }

    fn extract_cli_actual_usage(
        cli_backends: &TelegramCliBackendRegistry,
        backend: &TelegramCliBackend,
        stdout: &str,
        stderr: &str,
    ) -> Option<TelegramCliActualUsage> {
        let definition = cli_backends.get(backend)?;
        for extractor in &definition.usage_extractors {
            if let Some(usage) = Self::extract_usage_from_extractor(extractor, stdout, stderr) {
                return Some(usage);
            }
        }
        None
    }

    fn extract_cli_response_text(
        cli_backends: &TelegramCliBackendRegistry,
        backend: &TelegramCliBackend,
        stdout: &str,
        stderr: &str,
    ) -> Option<String> {
        let definition = cli_backends.get(backend)?;
        for extractor in &definition.response_extractors {
            if let Some(text) = Self::extract_response_from_extractor(extractor, stdout, stderr) {
                return Some(text);
            }
        }
        None
    }

    fn extract_incremental_cli_response(
        cli_backends: &TelegramCliBackendRegistry,
        backend: &TelegramCliBackend,
        stdout: &str,
        stderr: &str,
        last_sent_text: &str,
    ) -> Option<String> {
        let current = Self::extract_cli_response_text(cli_backends, backend, stdout, stderr)?;
        let current = current.trim();
        if current.is_empty() || current == last_sent_text {
            return None;
        }

        let candidate = current
            .strip_prefix(last_sent_text)
            .map(str::trim)
            .filter(|delta| !delta.is_empty())
            .unwrap_or(current);
        let candidate_len = candidate.chars().count();

        let should_send = if last_sent_text.is_empty() {
            candidate_len >= CLI_PROGRESS_MIN_PARTIAL_CHARS
                || (candidate.contains('\n') && candidate_len >= 20)
        } else {
            candidate_len >= 40
                || (candidate.contains('\n') && candidate_len >= 20)
                || candidate.matches('\n').count() >= 2
        };

        should_send.then(|| candidate.to_string())
    }

    async fn read_cli_stream<R>(
        reader: R,
        is_stdout: bool,
        tx: tokio::sync::mpsc::UnboundedSender<TelegramCliStreamEvent>,
    ) where
        R: tokio::io::AsyncRead + Unpin + Send + 'static,
    {
        let mut lines = tokio::io::BufReader::new(reader).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let event = if is_stdout {
                        TelegramCliStreamEvent::StdoutLine(line)
                    } else {
                        TelegramCliStreamEvent::StderrLine(line)
                    };
                    if tx.send(event).is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(err) => {
                    let line = format!("stream read failed: {}", err);
                    let event = if is_stdout {
                        TelegramCliStreamEvent::StderrLine(line)
                    } else {
                        TelegramCliStreamEvent::StderrLine(line)
                    };
                    let _ = tx.send(event);
                    break;
                }
            }
        }
    }

    fn format_cli_result(
        cli_backends: &TelegramCliBackendRegistry,
        backend: &TelegramCliBackend,
        exit_code: i32,
        duration_ms: u64,
        stdout: &str,
        stderr: &str,
    ) -> String {
        if let Some(definition) = cli_backends.get(backend) {
            for hint in &definition.error_hints {
                let haystack = Self::output_text_by_source(hint.source, stdout, stderr);
                if hint
                    .patterns
                    .iter()
                    .any(|pattern| haystack.contains(pattern))
                {
                    return hint.message.clone();
                }
            }
        }

        if exit_code == 0 {
            if let Some(text) =
                Self::extract_cli_response_text(cli_backends, backend, stdout, stderr)
            {
                return Self::truncate_chars(text.trim(), 3400);
            }

            return format!(
                "CodingAgent: {}\nStatus: {}\nElapsed: {}\nOutput: {}",
                Self::backend_label(backend),
                Self::value_label("done"),
                Self::value_label(format!("{}ms", duration_ms)),
                Self::value_label("not captured")
            );
        }

        let body = Self::extract_cli_response_text(cli_backends, backend, stdout, stderr)
            .unwrap_or_else(|| "CLI failed with no output.".to_string());

        format!(
            "CodingAgent: {}\nStatus: {}\nElapsed: {}\nExitCode: {}\n\n{}",
            Self::backend_label(backend),
            Self::value_label("failed"),
            Self::value_label(format!("{}ms", duration_ms)),
            Self::value_label(exit_code.to_string()),
            Self::truncate_chars(body.trim(), 3400)
        )
    }

    async fn execute_cli_request(
        bot_token: &str,
        chat_id: i64,
        text: &str,
        cli_workdir: Arc<PathBuf>,
        cli_timeout_secs: u64,
        cli_backends: Arc<TelegramCliBackendRegistry>,
        cli_backend_paths: Arc<HashMap<TelegramCliBackend, String>>,
        chat_states: Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: Arc<PathBuf>,
    ) -> TelegramCliExecutionResult {
        let state = Self::load_chat_state_snapshot(&chat_states, chat_id);
        let backend = state.effective_cli_backend(&cli_backends);
        let started_at = Self::current_timestamp_millis();
        let effective_cli_workdir = state.effective_cli_workdir(&cli_workdir);

        let invocation = match Self::build_cli_invocation(
            chat_id,
            &state,
            &effective_cli_workdir,
            &cli_backends,
            &cli_backend_paths,
            text,
        ) {
            Ok(invocation) => invocation,
            Err(err) => {
                return TelegramCliExecutionResult {
                    response_text: err,
                    send_followup: true,
                }
            }
        };

        let snapshot = match chat_states.lock() {
            Ok(mut states) => {
                let state = states.entry(chat_id).or_default();
                let usage = state.usage.entry(backend.as_str().to_string()).or_default();
                usage.requests = usage.requests.saturating_add(1);
                usage.last_started_at_ms = Some(started_at);
                states.clone()
            }
            Err(err) => {
                return TelegramCliExecutionResult {
                    response_text: format!("State update failed before CLI execution: {}", err),
                    send_followup: true,
                };
            }
        };
        Self::persist_chat_states(&state_path, &snapshot);

        let (binary, args) = invocation;
        let mut command = tokio::process::Command::new(&binary);
        command.args(&args);
        command.current_dir(&effective_cli_workdir);
        command.env("NO_COLOR", "1");
        command.env("CLICOLOR", "0");
        command.env("TERM", "dumb");
        command.stdin(std::process::Stdio::null());
        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());
        command.kill_on_drop(true);

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                let snapshot = match chat_states.lock() {
                    Ok(mut states) => {
                        let state = states.entry(chat_id).or_default();
                        let usage = state.usage.entry(backend.as_str().to_string()).or_default();
                        usage.failures = usage.failures.saturating_add(1);
                        usage.last_exit_code = Some(-1);
                        usage.last_completed_at_ms = Some(Self::current_timestamp_millis());
                        states.clone()
                    }
                    Err(_) => HashMap::new(),
                };
                if !snapshot.is_empty() {
                    Self::persist_chat_states(&state_path, &snapshot);
                }
                return TelegramCliExecutionResult {
                    response_text: format!("Failed to start `{}`: {}", backend.as_str(), err),
                    send_followup: true,
                };
            }
        };

        let initial_progress = TelegramOutgoingMessage::plain(Self::build_cli_streaming_message(
            &state,
            &backend,
            &effective_cli_workdir,
            "running",
            0,
            None,
            None,
        ));
        let initial_message_id =
            match Self::send_telegram_message_and_get_id(bot_token, chat_id, &initial_progress)
                .await
            {
                Ok(message_id) => Some(message_id),
                Err(err) => {
                    log::warn!(
                        "TelegramClient: failed to create streaming progress message: {}",
                        err
                    );
                    None
                }
            };
        if initial_message_id.is_some() {
            let _ = Self::send_telegram_chat_action(bot_token, chat_id, "typing").await;
        }

        let Some(stdout_reader) = child.stdout.take() else {
            let response_text = format!("Failed to capture `{}` stdout.", backend.as_str());
            if let Some(message_id) = initial_message_id {
                let message = TelegramOutgoingMessage::plain(Self::build_cli_streaming_message(
                    &state,
                    &backend,
                    &effective_cli_workdir,
                    "failed",
                    0,
                    None,
                    Some(&response_text),
                ));
                let _ = Self::edit_telegram_message(bot_token, chat_id, message_id, &message).await;
            }
            return TelegramCliExecutionResult {
                response_text,
                send_followup: initial_message_id.is_none(),
            };
        };
        let Some(stderr_reader) = child.stderr.take() else {
            let response_text = format!("Failed to capture `{}` stderr.", backend.as_str());
            if let Some(message_id) = initial_message_id {
                let message = TelegramOutgoingMessage::plain(Self::build_cli_streaming_message(
                    &state,
                    &backend,
                    &effective_cli_workdir,
                    "failed",
                    0,
                    None,
                    Some(&response_text),
                ));
                let _ = Self::edit_telegram_message(bot_token, chat_id, message_id, &message).await;
            }
            return TelegramCliExecutionResult {
                response_text,
                send_followup: initial_message_id.is_none(),
            };
        };

        let started = Instant::now();
        let progress_state = state.clone();
        let progress_workdir = effective_cli_workdir.clone();
        let execution_backend = backend.clone();
        let execution_cli_backends = cli_backends.clone();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(Self::read_cli_stream(stdout_reader, true, tx.clone()));
        tokio::spawn(Self::read_cli_stream(stderr_reader, false, tx));

        let execution = async move {
            let mut child = child;
            let wait_fut = child.wait();
            tokio::pin!(wait_fut);
            let mut progress_heartbeat =
                tokio::time::interval(Duration::from_secs(CLI_PROGRESS_UPDATE_SECS));
            progress_heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            progress_heartbeat.tick().await;
            let mut typing_heartbeat =
                tokio::time::interval(Duration::from_secs(TELEGRAM_CHAT_ACTION_UPDATE_SECS));
            typing_heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            typing_heartbeat.tick().await;

            let mut stdout = String::new();
            let mut stderr = String::new();
            let mut last_partial_text = String::new();
            let mut latest_output_text = None::<String>;
            let mut last_output_at = None::<Instant>;
            let mut child_status = None;
            let mut streaming_message_id = initial_message_id;
            let mut stream_message_usable = streaming_message_id.is_some();

            loop {
                tokio::select! {
                    status = &mut wait_fut, if child_status.is_none() => {
                        child_status = Some(status);
                    }
                    maybe_event = rx.recv() => {
                        match maybe_event {
                            Some(TelegramCliStreamEvent::StdoutLine(line)) => {
                                if !stdout.is_empty() {
                                    stdout.push('\n');
                                }
                                stdout.push_str(&line);
                                last_output_at = Some(Instant::now());
                            }
                            Some(TelegramCliStreamEvent::StderrLine(line)) => {
                                if !stderr.is_empty() {
                                    stderr.push('\n');
                                }
                                stderr.push_str(&line);
                                last_output_at = Some(Instant::now());
                            }
                            None => {
                                if child_status.is_some() {
                                    break;
                                }
                            }
                        }

                        if let Some(current_text) = Self::extract_cli_response_text(
                            &execution_cli_backends,
                            &execution_backend,
                            &stdout,
                            &stderr,
                        )
                        {
                            let current_text = current_text.trim().to_string();
                            if !current_text.is_empty() {
                                latest_output_text = Some(current_text);
                            }
                        }

                        if stream_message_usable
                            && Self::extract_incremental_cli_response(
                                &execution_cli_backends,
                                &execution_backend,
                                &stdout,
                                &stderr,
                                &last_partial_text,
                            )
                            .is_some()
                        {
                            if let (Some(message_id), Some(current_text)) =
                                (streaming_message_id, latest_output_text.as_deref())
                            {
                                let elapsed_secs = started.elapsed().as_secs();
                                let last_output_secs =
                                    last_output_at.map(|instant| instant.elapsed().as_secs());
                                let message = TelegramOutgoingMessage::plain(
                                    Self::build_cli_streaming_message(
                                        &progress_state,
                                        &execution_backend,
                                        &progress_workdir,
                                        "running",
                                        elapsed_secs,
                                        last_output_secs,
                                        Some(current_text),
                                    ),
                                );
                                match Self::edit_telegram_message(
                                    bot_token,
                                    chat_id,
                                    message_id,
                                    &message,
                                )
                                .await
                                {
                                    Ok(_) => {
                                        last_partial_text = current_text.to_string();
                                    }
                                    Err(err) => {
                                        log::warn!(
                                            "TelegramClient: failed to edit streaming message: {}",
                                            err
                                        );
                                        streaming_message_id = None;
                                        stream_message_usable = false;
                                    }
                                }
                            }
                        }
                    }
                    _ = progress_heartbeat.tick() => {
                        if child_status.is_none() && stream_message_usable {
                            if let Some(message_id) = streaming_message_id {
                                let elapsed_secs = started.elapsed().as_secs();
                                let last_output_secs =
                                    last_output_at.map(|instant| instant.elapsed().as_secs());
                                let message = TelegramOutgoingMessage::plain(
                                    Self::build_cli_streaming_message(
                                        &progress_state,
                                        &execution_backend,
                                        &progress_workdir,
                                        "running",
                                        elapsed_secs,
                                        last_output_secs,
                                        latest_output_text.as_deref(),
                                    ),
                                );
                                if let Err(err) = Self::edit_telegram_message(
                                    bot_token,
                                    chat_id,
                                    message_id,
                                    &message,
                                )
                                .await
                                {
                                    log::warn!(
                                        "TelegramClient: failed to refresh streaming message: {}",
                                        err
                                    );
                                    streaming_message_id = None;
                                    stream_message_usable = false;
                                }
                            }
                        }
                    }
                    _ = typing_heartbeat.tick() => {
                        if child_status.is_none() {
                            let _ = Self::send_telegram_chat_action(
                                bot_token,
                                chat_id,
                                "typing",
                            )
                            .await;
                        }
                    }
                }
            }

            let output = match child_status {
                Some(status) => status?,
                None => wait_fut.await?,
            };
            Ok::<_, std::io::Error>((
                output,
                stdout,
                stderr,
                last_output_at,
                latest_output_text,
                streaming_message_id,
                stream_message_usable,
            ))
        };

        let timed_output =
            tokio::time::timeout(Duration::from_secs(cli_timeout_secs), execution).await;

        match timed_output {
            Ok(Ok((
                status,
                stdout,
                stderr,
                last_output_at,
                latest_output_text,
                streaming_message_id,
                stream_message_usable,
            ))) => {
                let duration_ms = started.elapsed().as_millis() as u64;
                let exit_code = status.code().unwrap_or(-1);
                let success = status.success();
                let completed_at_ms = Self::current_timestamp_millis();
                let actual_cli_usage =
                    Self::extract_cli_actual_usage(&cli_backends, &backend, &stdout, &stderr);

                let snapshot = match chat_states.lock() {
                    Ok(mut states) => {
                        let state = states.entry(chat_id).or_default();
                        let usage = state.usage.entry(backend.as_str().to_string()).or_default();
                        if success {
                            usage.successes = usage.successes.saturating_add(1);
                        } else {
                            usage.failures = usage.failures.saturating_add(1);
                        }
                        usage.total_duration_ms =
                            usage.total_duration_ms.saturating_add(duration_ms);
                        usage.last_exit_code = Some(exit_code);
                        usage.last_completed_at_ms = Some(completed_at_ms);
                        if let Some(actual_cli_usage) = actual_cli_usage {
                            usage.record_actual_usage(actual_cli_usage, completed_at_ms);
                        }
                        states.clone()
                    }
                    Err(_) => HashMap::new(),
                };
                if !snapshot.is_empty() {
                    Self::persist_chat_states(&state_path, &snapshot);
                }

                let response_text = Self::format_cli_result(
                    &cli_backends,
                    &backend,
                    exit_code,
                    duration_ms,
                    &stdout,
                    &stderr,
                );
                let mut send_followup = streaming_message_id.is_none() || !stream_message_usable;

                if let Some(message_id) = streaming_message_id {
                    let phase = if success { "completed" } else { "failed" };
                    let last_output_secs =
                        last_output_at.map(|instant| instant.elapsed().as_secs());
                    let final_output = latest_output_text
                        .as_deref()
                        .filter(|text| !text.trim().is_empty())
                        .unwrap_or(response_text.as_str());
                    let message =
                        TelegramOutgoingMessage::plain(Self::build_cli_streaming_message(
                            &state,
                            &backend,
                            &effective_cli_workdir,
                            phase,
                            started.elapsed().as_secs(),
                            last_output_secs,
                            Some(final_output),
                        ));
                    if let Err(err) =
                        Self::edit_telegram_message(bot_token, chat_id, message_id, &message).await
                    {
                        log::warn!(
                            "TelegramClient: failed to finalize streaming message: {}",
                            err
                        );
                        send_followup = true;
                    }
                }

                TelegramCliExecutionResult {
                    response_text,
                    send_followup,
                }
            }
            Ok(Err(err)) => {
                let snapshot = match chat_states.lock() {
                    Ok(mut states) => {
                        let state = states.entry(chat_id).or_default();
                        let usage = state.usage.entry(backend.as_str().to_string()).or_default();
                        usage.failures = usage.failures.saturating_add(1);
                        usage.last_exit_code = Some(-1);
                        usage.last_completed_at_ms = Some(Self::current_timestamp_millis());
                        states.clone()
                    }
                    Err(_) => HashMap::new(),
                };
                if !snapshot.is_empty() {
                    Self::persist_chat_states(&state_path, &snapshot);
                }

                let response_text = format!(
                    "`{}` failed while waiting for output: {}",
                    backend.as_str(),
                    err
                );
                if let Some(message_id) = initial_message_id {
                    let message =
                        TelegramOutgoingMessage::plain(Self::build_cli_streaming_message(
                            &state,
                            &backend,
                            &effective_cli_workdir,
                            "failed",
                            started.elapsed().as_secs(),
                            None,
                            Some(&response_text),
                        ));
                    if let Err(edit_err) =
                        Self::edit_telegram_message(bot_token, chat_id, message_id, &message).await
                    {
                        log::warn!(
                            "TelegramClient: failed to report execution error in streaming message: {}",
                            edit_err
                        );
                        return TelegramCliExecutionResult {
                            response_text,
                            send_followup: true,
                        };
                    }
                    return TelegramCliExecutionResult {
                        response_text,
                        send_followup: false,
                    };
                }

                TelegramCliExecutionResult {
                    response_text,
                    send_followup: true,
                }
            }
            Err(_) => {
                let snapshot = match chat_states.lock() {
                    Ok(mut states) => {
                        let state = states.entry(chat_id).or_default();
                        let usage = state.usage.entry(backend.as_str().to_string()).or_default();
                        usage.failures = usage.failures.saturating_add(1);
                        usage.last_exit_code = Some(-2);
                        usage.last_completed_at_ms = Some(Self::current_timestamp_millis());
                        states.clone()
                    }
                    Err(_) => HashMap::new(),
                };
                if !snapshot.is_empty() {
                    Self::persist_chat_states(&state_path, &snapshot);
                }

                let response_text = format!(
                    "`{}` timed out after `{}` seconds.",
                    backend.as_str(),
                    cli_timeout_secs
                );
                if let Some(message_id) = initial_message_id {
                    let message =
                        TelegramOutgoingMessage::plain(Self::build_cli_streaming_message(
                            &state,
                            &backend,
                            &effective_cli_workdir,
                            &format!("timed out after `{}` second(s)", cli_timeout_secs),
                            cli_timeout_secs,
                            None,
                            Some(&response_text),
                        ));
                    if let Err(err) =
                        Self::edit_telegram_message(bot_token, chat_id, message_id, &message).await
                    {
                        log::warn!(
                            "TelegramClient: failed to report timeout in streaming message: {}",
                            err
                        );
                        return TelegramCliExecutionResult {
                            response_text,
                            send_followup: true,
                        };
                    }
                    return TelegramCliExecutionResult {
                        response_text,
                        send_followup: false,
                    };
                }

                TelegramCliExecutionResult {
                    response_text,
                    send_followup: true,
                }
            }
        }
    }

    async fn route_message(
        bot_token: &str,
        chat_id: i64,
        text: &str,
        agent: Option<Arc<crate::core::agent_core::AgentCore>>,
        cli_workdir: Arc<PathBuf>,
        cli_timeout_secs: u64,
        cli_backends: Arc<TelegramCliBackendRegistry>,
        cli_backend_paths: Arc<HashMap<TelegramCliBackend, String>>,
        chat_states: Arc<Mutex<HashMap<i64, TelegramChatState>>>,
        state_path: Arc<PathBuf>,
        active_handlers: i32,
    ) -> Vec<TelegramOutgoingMessage> {
        let (state, is_new_chat) = Self::ensure_chat_state(&chat_states, &state_path, chat_id);
        let mut replies = Vec::new();
        let routed_text = Self::pending_menu_command(&state, text, &cli_backends)
            .unwrap_or_else(|| text.to_string());

        if is_new_chat {
            replies.push(Self::build_connected_message(&state));
        }

        if let Some(reply) = Self::handle_command(
            chat_id,
            &routed_text,
            agent.as_deref(),
            &chat_states,
            &state_path,
            &cli_backends,
            &cli_backend_paths,
            &cli_workdir,
            active_handlers,
        ) {
            replies.push(reply);
            return replies;
        }

        let state = Self::load_chat_state_snapshot(&chat_states, chat_id);
        match state.interaction_mode {
            TelegramInteractionMode::Chat => {
                let Some(agent_core) = agent else {
                    replies.push(TelegramOutgoingMessage::plain(
                        "AgentCore is not available for chat mode.",
                    ));
                    return replies;
                };
                let session_id = format!(
                    "tg_{}_{}",
                    chat_id,
                    state.session_label_for(TelegramInteractionMode::Chat)
                );
                let prompt =
                    Self::build_unified_agent_prompt(&state, &cli_workdir, &cli_backends, text);
                let response = Self::wait_with_typing_indicator(
                    bot_token,
                    chat_id,
                    agent_core.process_prompt(&session_id, &prompt, None),
                )
                .await;
                Self::append_session_transcript(
                    chat_id,
                    TelegramInteractionMode::Chat,
                    &state,
                    text,
                    &response,
                );
                replies.push(TelegramOutgoingMessage::plain(response));
                replies
            }
            TelegramInteractionMode::Coding => {
                let Some(agent_core) = agent else {
                    replies.push(TelegramOutgoingMessage::plain(
                        "AgentCore is not available for coding mode.",
                    ));
                    return replies;
                };
                let session_id = format!(
                    "tg_{}_{}",
                    chat_id,
                    state.session_label_for(TelegramInteractionMode::Coding)
                );
                let prompt =
                    Self::build_unified_agent_prompt(&state, &cli_workdir, &cli_backends, text);
                let response = Self::wait_with_typing_indicator(
                    bot_token,
                    chat_id,
                    agent_core.process_prompt(&session_id, &prompt, None),
                )
                .await;
                Self::append_session_transcript(
                    chat_id,
                    TelegramInteractionMode::Coding,
                    &state,
                    text,
                    &response,
                );
                replies.push(TelegramOutgoingMessage::plain(response));
                replies
            }
        }
    }
}
