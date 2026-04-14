impl Channel for TelegramClient {
    fn name(&self) -> &str {
        &self.name
    }

    fn start(&mut self) -> bool {
        if self.running.load(Ordering::SeqCst) {
            return true;
        }
        if self.bot_token.is_empty() || self.bot_token == "YOUR_TELEGRAM_BOT_TOKEN_HERE" {
            log::warn!("TelegramClient: invalid bot token");
            return false;
        }

        let reset_url = format!(
            "https://api.telegram.org/bot{}/deleteWebhook",
            self.bot_token
        );
        let client = crate::infra::http_client::HttpClient::new();
        let _ = client.get_sync(&reset_url);
        Self::register_bot_commands(&self.bot_token);

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let bot_token = self.bot_token.clone();
        let allowed_ids = self.allowed_chat_ids.clone();
        let active_handlers = self.active_handlers.clone();
        let agent = self.agent.clone();
        let cli_workdir = self.cli_workdir.clone();
        let cli_timeout_secs = self.cli_timeout_secs;
        let cli_backends = self.cli_backends.clone();
        let cli_backend_paths = self.cli_backend_paths.clone();
        let chat_states = self.chat_states.clone();
        let chat_state_path = self.chat_state_path.clone();
        let last_user_input = self.last_user_input.clone();

        Self::broadcast_startup_status(&self.bot_token, &self.allowed_chat_ids, &self.chat_states);

        // Idle-trim background task: when no user input for 3 minutes, release
        // free heap pages back to the OS via malloc_trim(0).
        {
            const IDLE_TRIM_SECS: u64 = 180;
            const CHECK_INTERVAL_SECS: u64 = 30;
            let running_trim = running.clone();
            let last_input_trim = last_user_input.clone();
            tokio::spawn(async move {
                let mut trimmed_at: u64 = 0;
                loop {
                    tokio::time::sleep(Duration::from_secs(CHECK_INTERVAL_SECS)).await;
                    if !running_trim.load(Ordering::SeqCst) {
                        break;
                    }
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let last = last_input_trim.load(Ordering::Relaxed);
                    // Trim once per idle window; don't repeat until next message arrives.
                    if now.saturating_sub(last) >= IDLE_TRIM_SECS && last != trimmed_at {
                        unsafe { libc::malloc_trim(0) };
                        trimmed_at = last;
                        log::info!(
                            "TelegramClient: idle {}s — malloc_trim(0) executed",
                            now.saturating_sub(last)
                        );
                    }
                }
            });
        }

        tokio::spawn(async move {
            log::debug!("TelegramClient async epoll reactor started");
            let mut offset: i64 = 0;
            let mut backoff_secs = 5u64;

            while running.load(Ordering::SeqCst) {
                let url = format!(
                    "https://api.telegram.org/bot{}/getUpdates?offset={}&timeout=50",
                    bot_token, offset
                );

                let client = crate::infra::http_client::HttpClient::new();
                let resp = match client.get(&url).await {
                    Ok(r) => r,
                    Err(e) => {
                        log::error!("Telegram polling error: {}", e);
                        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                        backoff_secs = (backoff_secs * 2).min(60);
                        continue;
                    }
                };

                if !running.load(Ordering::SeqCst) {
                    break;
                }
                backoff_secs = 5;

                let data: Value = match serde_json::from_str(&resp.body) {
                    Ok(v) => v,
                    Err(_) => {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                };

                if !data["ok"].as_bool().unwrap_or(false) {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    continue;
                }

                if let Some(results) = data["result"].as_array() {
                    for item in results {
                        offset = item["update_id"].as_i64().unwrap_or(0) + 1;
                        let msg = match item.get("message") {
                            Some(m) => m,
                            None => continue,
                        };
                        let text = msg["text"].as_str().unwrap_or("");
                        let chat_id = msg["chat"]["id"].as_i64().unwrap_or(0);

                        if text.is_empty() || chat_id == 0 {
                            continue;
                        }
                        if !allowed_ids.is_empty() && !allowed_ids.contains(&chat_id) {
                            log::debug!("Blocked chat_id {} — not in allowlist", chat_id);
                            continue;
                        }

                        let current_handlers = active_handlers.load(Ordering::SeqCst);
                        if current_handlers >= MAX_CONCURRENT_HANDLERS {
                            log::warn!(
                                "Telegram dropping message: max concurrent handlers ({}) reached",
                                current_handlers
                            );
                            continue;
                        }

                        log::debug!("Telegram received from {}: {}", chat_id, text);

                        // Record activity time to reset the idle-trim window.
                        let now_secs = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        last_user_input.store(now_secs, Ordering::Relaxed);

                        active_handlers.fetch_add(1, Ordering::SeqCst);
                        let text_clone = text.to_string();
                        let bot_token_clone = bot_token.clone();
                        let agent_clone = agent.clone();
                        let active_handlers_clone = active_handlers.clone();
                        let cli_workdir_clone = cli_workdir.clone();
                        let cli_backends_clone = cli_backends.clone();
                        let cli_backend_paths_clone = cli_backend_paths.clone();
                        let chat_states_clone = chat_states.clone();
                        let chat_state_path_clone = chat_state_path.clone();

                        tokio::spawn(async move {
                            let results = TelegramClient::route_message(
                                &bot_token_clone,
                                chat_id,
                                &text_clone,
                                agent_clone,
                                cli_workdir_clone,
                                cli_timeout_secs,
                                cli_backends_clone,
                                cli_backend_paths_clone,
                                chat_states_clone,
                                chat_state_path_clone,
                                current_handlers + 1,
                            )
                            .await;
                            for result in results {
                                TelegramClient::send_telegram_message(
                                    &bot_token_clone,
                                    chat_id,
                                    &result,
                                );
                            }
                            active_handlers_clone.fetch_sub(1, Ordering::SeqCst);
                        });
                    }
                }
            }
            log::debug!("TelegramClient async epoll reactor stopped");
        });

        log::info!("TelegramClient started");
        true
    }

    fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }

    fn send_message(&self, msg: &str) -> Result<(), String> {
        for chat_id in self.allowed_chat_ids.iter() {
            Self::send_telegram_chunks(
                &self.bot_token,
                *chat_id,
                msg,
                self.max_message_chars,
                None,
            );
        }
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

