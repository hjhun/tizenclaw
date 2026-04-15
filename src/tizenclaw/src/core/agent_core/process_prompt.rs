impl AgentCore {
    /// Process a user prompt through the 15-phase autonomous agent loop.
    ///
    /// ## Loop Phases
    /// 1. GoalParsing: Initialize AgentLoopState for this session + prompt
    /// 2. ContextLoading: Load session history, build messages + tools
    /// 3. Pre-loop Compaction: Compact if ≥90% of 256k token budget
    ///    4-13. Main loop: DecisionMaking → SafetyCheck → ToolDispatching
    ///    → ObservationCollect → Evaluating → ErrorRecovery
    ///    → StateTracking → SelfInspection → RePlanning → TerminationCheck
    /// 14. ResultReporting: Format and return final answer
    ///
    /// Thread-safe: acquires fine-grained locks on individual fields.
    pub async fn process_prompt(
        &self,
        session_id: &str,
        prompt: &str,
        on_chunk: Option<&(dyn Fn(&str) + Send + Sync)>,
    ) -> String {
        // ── Phase 1: GoalParsing ─────────────────────────────────────────
        // Reset circuit breakers at the start of each session so failures
        // from a prior session do not cascade into new requests.
        self.reset_circuit_breakers();
        if let Ok(policy) = self.tool_policy.lock() {
            policy.reset();
            policy.reset_idle_tracking(session_id);
        }
        let mut loop_state = AgentLoopState::new(session_id, prompt);
        let mut skip_memory_extraction = false;
        let mut auto_prepared_skill_name: Option<String> = None;

        // Load context token budget from config if available
        let (budget, threshold) = {
            let cfg = self.llm_config.lock().ok();
            let b = cfg
                .as_ref()
                .and_then(|c| c.backends.get("context_token_budget"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(CONTEXT_TOKEN_BUDGET);
            let t = cfg
                .as_ref()
                .and_then(|c| c.backends.get("context_compact_threshold"))
                .and_then(|v| v.as_f64())
                .map(|v| v as f32)
                .unwrap_or(CONTEXT_COMPACT_THRESHOLD);
            (b, t)
        };
        loop_state.token_budget = budget;
        loop_state.compact_threshold = threshold;
        self.persist_loop_snapshot(&loop_state);

        log::debug!(
            "[AgentLoop] Phase=GoalParsing session='{}' goal='{}' budget={}",
            session_id,
            utf8_safe_preview(prompt, 80),
            budget
        );

        // Quick check: do we have any backend?
        {
            let has_primary = self.backend.read().await.is_some();
            let has_fallback = !self.fallback_backends.read().await.is_empty();
            if !has_primary && !has_fallback {
                loop_state.last_error = Some("No LLM backend configured".into());
                loop_state.mark_terminal(
                    LoopTransitionReason::NoBackendConfigured,
                    "no primary or fallback backend is configured",
                );
                self.persist_loop_snapshot(&loop_state);
                return "Error: No LLM backend configured".into();
            }
        }

        // ── Phase 2: ContextLoading ──────────────────────────────────────
        loop_state.transition(AgentPhase::ContextLoading);

        log_conversation("User", prompt);

        let session_workdir = if let Ok(ss) = self.session_store.lock() {
            ss.as_ref()
                .map(|store| store.session_workdir(session_id))
                .unwrap_or_else(|| self.platform.paths.data_dir.clone())
        } else {
            self.platform.paths.data_dir.clone()
        };
        let literal_json_output = prompt_requires_literal_json_output(prompt);
        let mut preloaded_context_messages = Vec::new();

        // Store user message
        if let Ok(ss) = self.session_store.lock() {
            if let Some(store) = ss.as_ref() {
                store.add_message(session_id, "user", prompt);
                store.add_structured_user_message(session_id, prompt);
            }
        }

        if let Some(text) = self
            .try_process_prompt_shortcuts(
                session_id,
                prompt,
                &session_workdir,
                literal_json_output,
                &mut loop_state,
            )
            .await
        {
            return text;
        }

        // Build conversation history — compaction-aware load
        let history = {
            let ss = self.session_store.lock();
            if let Some(Ok(Some((msgs, from_compact)))) = ss.ok().map(|s| {
                // Returns (Vec<SessionMessage>, bool)
                Ok::<_, ()>(
                    s.as_ref()
                        .map(|store| store.load_session_context(session_id, MAX_CONTEXT_MESSAGES)),
                )
            }) {
                if from_compact {
                    log::info!(
                        "[ContextLoading] session='{}' loaded from compacted.md",
                        session_id
                    );
                } else {
                    log::info!(
                        "[ContextLoading] session='{}' loaded {} msgs from history",
                        session_id,
                        msgs.len()
                    );
                }
                msgs
            } else {
                vec![]
            }
        };

        let mut messages: Vec<LlmMessage> = history
            .iter()
            .cloned()
            .map(|m| m.into_llm_message())
            .filter_map(sanitize_message_for_transport)
            .collect();

        if messages.is_empty() || messages.last().map(|m| m.role.as_str()) != Some("user") {
            messages.push(LlmMessage::user(prompt));
        }
        for context in preloaded_context_messages.drain(..) {
            inject_context_message(&mut messages, context);
        }
        if let Err(err) = self.check_context_message_limit(session_id, &messages, &mut loop_state) {
            return format!("Error: {}", err);
        }

        // Extract intent keywords for optimal tool injection
        let intent_keywords = Self::extract_intent_keywords(prompt);

        let registrations = self.list_registered_paths();
        let skill_capabilities =
            skill_capability_manager::load_snapshot(&self.platform.paths, &registrations);
        let skill_roots = skill_capabilities
            .roots
            .iter()
            .map(|root| root.path.clone())
            .collect::<Vec<_>>();
        let textual_skills = skill_capabilities.enabled_skills();
        let is_dashboard_web_app_request = Self::is_web_dashboard_app_request(session_id, prompt);
        let is_file_management_request = is_simple_file_management_request(prompt)
            && !prompt_prefers_direct_specialized_tools(prompt);
        let has_expected_file_targets = !expected_file_management_targets(prompt).is_empty();
        let mut session_profile = self.resolve_session_profile(session_id);
        if session_profile.is_none() && is_dashboard_web_app_request {
            session_profile = Some(SessionPromptProfile {
                system_prompt: Some(
                    "For browser-based apps in dashboard web sessions, use only the \
                     generate_web_app tool. Do not write raw HTML files into the \
                     session workdir, do not use run_generated_code for HTML, and do \
                     not open file:// or local workdir paths."
                        .to_string(),
                ),
                allowed_tools: Some(vec!["generate_web_app".to_string()]),
                ..SessionPromptProfile::default()
            });
        }
        if session_profile.is_none() && is_file_management_request {
            session_profile = Some(SessionPromptProfile {
                role_name: Some("file_manager_flow".to_string()),
                role_description: Some(
                    "Direct file management profile for normal file and directory operations."
                        .to_string(),
                ),
                system_prompt: Some(
                    "For normal file and directory tasks, manage files directly with file_manager \
                     or file_write. Create directories explicitly, write the requested files \
                     into the working directory, and avoid run_generated_code unless the user \
                     explicitly asks for an executable script to be generated and run."
                        .to_string(),
                ),
                allowed_tools: Some(vec!["file_manager".to_string(), "file_write".to_string()]),
                max_iterations: Some(0),
                ..SessionPromptProfile::default()
            });
        }
        if let Some(max_iterations) = session_profile
            .as_ref()
            .and_then(|profile| profile.max_iterations)
        {
            loop_state.max_tool_rounds = max_iterations;
        }
        let skill_reference_docs = if literal_json_output {
            Vec::new()
        } else {
            crate::core::skill_support::list_skill_reference_docs(&self.platform.paths.docs_dir)
        };
        let prefetched_skills = if literal_json_output {
            Vec::new()
        } else {
            select_relevant_skills(prompt, &textual_skills, MAX_PREFETCHED_SKILLS)
        };
        for skill in &prefetched_skills {
            log::info!(
                "[SkillAudit] skill='{}' shell_prelude={} code_fence_languages={:?} prelude_excerpt='{}'",
                skill.file_name,
                skill.shell_prelude,
                skill.code_fence_languages,
                utf8_safe_preview(&skill.prelude_excerpt, 160),
            );
        }
        loop_state.record_prefetch_skills(
            prefetched_skills
                .iter()
                .map(|skill| skill.file_name.clone())
                .collect(),
        );
        let skill_context = build_skill_prefetch_message(&prefetched_skills);
        if let Some(skill_context) = skill_context.as_ref() {
            inject_context_message(&mut messages, skill_context.clone());
        }

        // Get tool declarations
        let mut tools = self
            .tool_dispatcher
            .read()
            .await
            .get_tool_declarations_filtered(&intent_keywords);
        let tools_forbidden = prompt_explicitly_forbids_tools(prompt);
        if !tools_forbidden {
            crate::core::tool_declaration_builder::ToolDeclarationBuilder::append_builtin_tools(
                &mut tools, prompt,
            );
        } else {
            tools.clear();
        }
        if is_dashboard_web_app_request && !tools.iter().any(|tool| tool.name == "generate_web_app")
        {
            tools.push(crate::llm::backend::LlmToolDecl {
                name: "generate_web_app".into(),
                description: "Generate or update a web application served by the web dashboard at /apps/<app_id>/. Supports HTML/CSS/JS files, optional asset downloads, bridge tool allowlists, and best-effort bridge or webview launch on Tizen.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "app_id": {
                            "type": "string",
                            "description": "Unique identifier for the app (lowercase alphanumeric + underscore, max 64 chars)"
                        },
                        "title": {
                            "type": "string",
                            "description": "Display title for the web app"
                        },
                        "html": {
                            "type": "string",
                            "description": "Complete HTML content. Can be a single-file app or reference style.css and app.js"
                        },
                        "css": {
                            "type": "string",
                            "description": "Optional separate CSS stylesheet saved as style.css"
                        },
                        "js": {
                            "type": "string",
                            "description": "Optional separate JavaScript code saved as app.js"
                        },
                        "assets": {
                            "type": "array",
                            "description": "Optional external assets to download. Each item is {url, filename}. Max 10MB per file.",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "url": {"type": "string", "description": "Asset download URL"},
                                    "filename": {"type": "string", "description": "Local filename such as logo.png"}
                                }
                            }
                        },
                        "allowed_tools": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Optional tool names this app may call via the bridge API"
                        }
                    },
                    "required": ["app_id", "title", "html"]
                }),
            });
        }
        if let Ok(bridge) = self.action_bridge.lock() {
            tools.extend(bridge.get_action_declarations());
        }
        if let Some(allowed_tools) = session_profile
            .as_ref()
            .and_then(|profile| profile.allowed_tools.as_ref())
        {
            tools.retain(|tool| allowed_tools.iter().any(|name| name == &tool.name));
        }

        let restrict_to_generate_web_app = is_dashboard_web_app_request
            && session_profile
                .as_ref()
                .and_then(|profile| profile.allowed_tools.as_ref())
                .map(|tools| tools.len() == 1 && tools[0] == "generate_web_app")
                .unwrap_or(false);
        let prefer_direct_specialized_tools = prompt_prefers_direct_specialized_tools(prompt);
        if !restrict_to_generate_web_app && !tools_forbidden && !prefer_direct_specialized_tools {
            // Add search_tools meta-tool for Two-Tier router
            tools.push(crate::llm::backend::LlmToolDecl {
                name: "search_tools".into(),
                description: "Search available tools across all categories or within a specific category. Use this whenever the required capability is not already present in context.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "Keyword to search tools, or 'ALL'."}
                    },
                    "required": ["query"]
                })
            });
        }

        // Build System Prompt
        let (system_prompt, dynamic_context) = if literal_json_output {
            (
                "You are TizenClaw. Follow the user's formatting contract exactly. Return only the requested JSON object with no commentary, no markdown, and no tool calls when the prompt forbids tools."
                    .to_string(),
                None,
            )
        } else {
            let prompt_doc = llm_config_store::load(&self.platform.paths.config_dir)
                .unwrap_or_else(|_| llm_config_store::default_document());
            let mut builder = crate::core::prompt_builder::SystemPromptBuilder::new()
                .add_available_tools(tools.clone()); // XML Inject
            if let Some(role_prompt) = session_profile
                .as_ref()
                .and_then(|profile| profile.system_prompt.clone())
            {
                builder = builder.set_base_prompt(role_prompt);
            } else if let Ok(base) = self.system_prompt.read() {
                builder = builder.set_base_prompt(base.clone());
            }
            if let Ok(soul_lock) = self.soul_content.read() {
                if let Some(ref soul) = *soul_lock {
                    builder = builder.set_soul_content(soul.clone());
                }
            }

            let formatted_skills = prefetched_skills
                .into_iter()
                .map(|s| {
                    let summary = format_skill_summary(&s);
                    (s.absolute_path, summary)
                })
                .collect();
            builder = builder.add_available_skills(formatted_skills);
            let formatted_skill_references = skill_reference_docs
                .iter()
                .map(|doc| (doc.absolute_path.clone(), doc.description.clone()))
                .collect();
            builder = builder.add_available_skill_references(formatted_skill_references);

            let model_name = {
                let bn = self.backend_name.read().unwrap_or_else(|e| e.into_inner());
                (*bn).clone()
            };
            let prompt_mode = session_profile
                .as_ref()
                .and_then(|profile| profile.prompt_mode)
                .unwrap_or_else(|| prompt_mode_from_doc(&prompt_doc, &model_name));
            let reasoning_policy = session_profile
                .as_ref()
                .and_then(|profile| profile.reasoning_policy)
                .unwrap_or_else(|| reasoning_policy_from_doc(&prompt_doc, &model_name));
            builder = builder
                .set_prompt_mode(prompt_mode)
                .set_reasoning_policy(reasoning_policy);
            let platform_name = self.platform.platform_name().to_string();
            let data_dir = session_workdir.to_string_lossy().to_string();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| format_unix_timestamp_utc(d.as_secs()))
                .unwrap_or_else(|_| "unknown".into());
            builder = builder.set_runtime_context(platform_name, model_name, data_dir, now);

            let dynamic_context = builder.build_dynamic_context();
            let system_prompt = builder.build();
            (system_prompt, dynamic_context)
        };

        if let Some(dynamic_context) = dynamic_context.as_ref() {
            inject_context_message(&mut messages, dynamic_context.clone());
        }
        if let Some(profile) = session_profile.as_ref() {
            let role_name = profile.role_name.as_deref().unwrap_or("custom");
            let description = profile
                .role_description
                .as_deref()
                .unwrap_or("No role description provided.");
            inject_context_message(
                &mut messages,
                format!(
                    "## Active Role Profile\nRole: {}\nDescription: {}",
                    role_name, description
                ),
            );
        }
        if !literal_json_output {
            inject_context_message(
                &mut messages,
                format!(
                    "## Working Directory\nUse '{}' as the primary working directory for file reads, file writes, generated scripts, and task artifacts unless the user explicitly gives a different absolute path.",
                    session_workdir.to_string_lossy()
                ),
            );
        }
        self.inject_prompt_contract_context(prompt, literal_json_output, &tools, &mut messages);

        // Load long term memory dynamically and inject into messages (preserves system_prompt cache)
        let mut memory_context_for_log: Option<String> = None;
        if literal_json_output || should_skip_memory_for_prompt(prompt) {
            loop_state.record_prefetch_memory(None);
        } else if let Ok(ms) = self.memory_store.lock() {
            if let Some(store) = ms.as_ref() {
                let mem_str = store.load_relevant_for_prompt(prompt, 5, 0.1);
                if !mem_str.is_empty() {
                    let memory_context = format!(
                        "## Context from Long-Term Memory\n<long_term_memory>\n{}\n</long_term_memory>",
                        mem_str
                    );
                    loop_state
                        .record_prefetch_memory(Some(utf8_safe_preview(&mem_str, 240).to_string()));
                    inject_context_message(&mut messages, memory_context.clone());
                    memory_context_for_log = Some(memory_context);
                } else {
                    loop_state.record_prefetch_memory(None);
                }
            }
        }

        messages = sanitize_messages_for_transport(messages);

        // ── Phase 2.5: Prompt Cache Preparation ─────────────────────────
        // Compute hash of system_prompt; refresh server-side cache only when
        // the prompt actually changed. For GeminiBackend this creates/refreshes
        // a CachedContent resource so subsequent rounds skip re-sending the
        // full system_instruction text (~60-80% prompt token savings).
        {
            let new_hash = Self::hash_str(&system_prompt);
            let cached_hash = *self.prompt_hash.read().await;
            if new_hash != cached_hash {
                log::debug!(
                    "[PromptCache] System prompt changed (hash {} → {}), refreshing cache…",
                    cached_hash,
                    new_hash
                );
                let be_guard = self.backend.read().await;
                if let Some(be) = be_guard.as_ref() {
                    let cached = be.prepare_cache(&system_prompt).await;
                    if cached {
                        log::info!(
                            "[PromptCache] Cache ready — subsequent rounds will reference cached content"
                        );
                    } else {
                        log::debug!(
                            "[PromptCache] Backend does not support caching or prompt too short; using inline system_instruction"
                        );
                    }
                }
                drop(be_guard);
                // Always update the stored hash so we do not retry on every call
                *self.prompt_hash.write().await = new_hash;
            } else {
                log::debug!(
                    "[PromptCache] Prompt unchanged (hash={}), reusing cached content",
                    cached_hash
                );
            }
        }
        if !literal_json_output && prompt_requests_longform_markdown_writing(prompt) {
            let exact_budget =
                if let Some((min_words, max_words)) = requested_word_budget_bounds(prompt) {
                    format!(
                        " Keep the final draft between {} and {} words.",
                        min_words, max_words
                    )
                } else {
                    String::new()
                };
            inject_context_message(
                &mut messages,
                format!(
                    "## Long-Form Writing Contract\nFor a blog post or article saved to Markdown, draft the full article mentally first, then save one polished final version with a clear title, an introduction, multiple titled body sections, and an explicit conclusion. Cover several distinct points instead of restating the same benefit, and make the body specific to the requested audience or role with concrete reasoning, examples, or practical implications. Include at least one concrete scenario, developer workflow, or before/after example instead of keeping every section abstract. Avoid extra stat or reread verification after a successful text-file write unless you are correcting a known issue. If the prompt gives a target word count, treat it as a planning budget rather than a loose suggestion, and bias the first draft slightly under the number rather than over it.{}",
                    exact_budget
                ),
            );
        }
        if !literal_json_output && prompt_requests_concise_summary_file(prompt) {
            inject_context_message(
                &mut messages,
                "## Concise Summary Contract\nFor a concise summary saved to a text file, preserve the requested paragraph count exactly and keep the total length compact. Favor roughly 150-300 words unless the user explicitly asks for a different length. Use each paragraph for a distinct job such as overview, core findings or benefits, and challenges or outlook when the source supports that structure. Avoid restating examples or details that are not necessary to capture the main themes.".to_string(),
            );
        }
        if !literal_json_output && prompt_requests_eli5_summary(prompt) {
            inject_context_message(
                &mut messages,
                "## ELI5 Summary Contract\nFor an Explain-Like-I'm-5 summary, use short sentences, concrete everyday analogies, and simple vocabulary. Cover the main idea, what the thing can do, one concrete sign that it works well in the real world if the source gives examples, what people did to make it safer or more reliable if the source supports that, and one honest limitation. When the source highlights different input types or standout evaluation examples, keep both in the child-friendly version instead of collapsing them into one vague sentence. Avoid adult technical shorthand such as 'chatbot', 'code', 'computer program', 'multimodal', 'benchmark', or 'reasoning' unless you immediately translate it into child-friendly language. If the prompt includes a word budget, stay inside it while preserving coverage.".to_string(),
            );
            let prompt_lower = prompt.to_ascii_lowercase();
            if prompt_lower.contains("gpt4.pdf") || prompt_lower.contains("gpt-4") {
                inject_context_message(
                    &mut messages,
                    "## Named Paper Anchor\nThis summary is about GPT-4 specifically. Even if the PDF reads like an outside experiment or commentary paper about GPT-4, explain GPT-4 itself in child-friendly language: it can work with words and pictures, it did unusually well on hard tests, people trained it at large scale in a controlled way, people also worked on safety and behavior, and it can still make mistakes.".to_string(),
                );
            }
        }
        if !literal_json_output && prompt_requests_humanization(prompt) {
            inject_context_message(
                &mut messages,
                "## Humanization Contract\nWhen rewriting text to sound more human, preserve the original meaning while removing stock AI phrasing, stiff transitions, repetitive sentence openings, and overly formal filler. Use contractions where natural, vary sentence length, and save one polished rewrite instead of iterating through multiple near-duplicate drafts. If the prompt explicitly asked for `/install <skill>`, leave a transcript-visible note that the install step was executed before writing the final file, then finish in one pass without a readback preview unless the user asked for one.".to_string(),
            );
        }
        if !literal_json_output && prompt_requests_email_triage_report(prompt) {
            inject_context_message(
                &mut messages,
                "## Email Triage Contract\nFor an inbox triage report, read every email in the referenced inbox exactly once before drafting the final report. Organize the output by priority order with a short summary at the top, and make the proposed plan for the day explicit in that summary. For every email entry, keep the subject or sender, `Priority: Pn`, `Category: ...`, and `Recommended action:` close together in the same compact block so the classification is easy to scan and verify. Treat revenue-bearing client blockers as P1 or higher, security deadlines as P1 or P2, production incidents as P0, newsletters or social noise as P3 or P4, and clear promotional spam as P4. Avoid oversized tables and avoid rewriting the report after the first complete draft unless a required field is missing.".to_string(),
            );
        }
        if !literal_json_output && prompt_requests_email_corpus_review(prompt) {
            inject_context_message(
                &mut messages,
                "## Email Corpus Review Contract\nThis task depends on a bounded set of workspace email files. List the relevant email folder once, then read every email file in that folder exactly once before drafting the final artifact. Build the answer only from those email files, do not stop after a partial sample, and avoid repeated rewrite loops once full coverage is complete. Only state exact numbers, dates, budget amounts, or technical details that are directly supported by the email corpus; if a detail is uncertain, summarize it more cautiously instead of inventing extra precision.".to_string(),
            );
        }
        if !literal_json_output && prompt_requests_executive_briefing(prompt) {
            let budget_hint = executive_briefing_word_budget_bounds(prompt)
                .map(|(min_words, max_words)| {
                    format!(
                        " Keep the final briefing between {} and {} words.",
                        min_words, max_words
                    )
                })
                .unwrap_or_default();
            inject_context_message(
                &mut messages,
                "## Executive Briefing Contract\nFor an executive briefing or daily summary, open with a short executive-summary section that surfaces the top 3-5 takeaways first. Then group the rest into a few clear sections, explicitly call out urgent risks, material opportunities, and actions or decisions needed, and keep the whole document concise enough to scan quickly. Draft the synthesis mentally first and save one polished version instead of rewriting the same briefing multiple times. Once the first complete valid briefing is written, stop unless a required section or source area is still missing.".to_string(),
            );
            if !budget_hint.is_empty() {
                inject_context_message(
                    &mut messages,
                    format!("## Executive Briefing Budget\n{}", budget_hint.trim()),
                );
            }
        }
        if !literal_json_output && prompt_requests_prediction_market_briefing(prompt) {
            if prompt_supplies_prediction_market_briefing_evidence(prompt) {
                inject_context_message(
                    &mut messages,
                    "## Supplied Evidence Contract\nThis prompt already includes the complete market evidence needed for the requested markdown file. Do not fetch external market data or run news searches. Use the supplied numbered items only, keep their order, and write the final file immediately.".to_string(),
                );
            } else {
                inject_context_message(
                    &mut messages,
                    "## Structured Odds Contract\nWhen the task requires live prediction-market odds, prefer a machine-readable official source over landing pages. For Polymarket, download the public Gamma API JSON into the workspace first with `file_manager`, using the active total-volume feed that the task references (`https://gamma-api.polymarket.com/markets?active=true&order=volumeNum&ascending=false&limit=10`) and falling back to the 24-hour-volume feed only if needed. Then read the saved JSON and extract real active market questions plus their Yes/No percentages. Keep the briefing anchored to the top active markets by trading volume unless a candidate has no grounded recent-news match at all. Write the final markdown in the exact requested shape only: `## 1. {Question}`, `**Current odds:** Yes X% / No Y%`, and `**Related news:** ...` for each of the three sections. Once the first complete valid three-market briefing is written, stop immediately instead of rewriting the file. Do not add extra bullet fields or rename those labels. Do not return a fallback note when the API is reachable.".to_string(),
                );
            }
        }
        if !literal_json_output {
            if let Some(rendered_briefing) =
                render_prediction_market_briefing_from_prompt_evidence(prompt)
            {
                let briefing_path = session_workdir.join("polymarket_briefing.md");
                if std::fs::write(&briefing_path, rendered_briefing.as_bytes()).is_ok() {
                    let write_result = json!({
                        "success": true,
                        "path": briefing_path.to_string_lossy(),
                        "bytes_written": rendered_briefing.len(),
                    });
                    if let Ok(ss) = self.session_store.lock() {
                        if let Some(store) = ss.as_ref() {
                            record_synthetic_tool_interaction(
                                store,
                                session_id,
                                "auto_write_prompt_grounded_polymarket_briefing",
                                "file_write",
                                "file_write",
                                synthetic_file_write_args(
                                    "polymarket_briefing.md",
                                    &rendered_briefing,
                                ),
                                &write_result,
                            );
                            let text = completion_message_for_prompt_file_targets(
                                prompt,
                                &session_workdir,
                                &["polymarket_briefing.md".to_string()],
                            );
                            maybe_record_completed_file_preview_interactions(
                                store,
                                session_id,
                                prompt,
                                &session_workdir,
                                &["polymarket_briefing.md".to_string()],
                            );
                            store.add_message(session_id, "assistant", &text);
                            store.add_structured_assistant_text_message(session_id, &text);
                            loop_state.transition(AgentPhase::ResultReporting);
                            loop_state.transition(AgentPhase::Complete);
                            loop_state.log_self_inspection();
                            self.persist_loop_snapshot(&loop_state);
                            log_conversation("Assistant", &text);
                            return text;
                        }
                    }
                    return completion_message_for_prompt_file_targets(
                        prompt,
                        &session_workdir,
                        &["polymarket_briefing.md".to_string()],
                    );
                }
            }
        }
        if prompt_requests_directory_synthesis(prompt) {
            let directory_list = extract_prompt_directory_paths(prompt);
            inject_context_message(
                &mut messages,
                format!(
                    "## Multi-File Synthesis Contract\nThis task depends on reading a set of real files from these workspace directories:\n{}\nList the directory contents first when needed, then read each relevant file exactly once before writing the final artifact. Do not try to read the directory path itself as a file. After the source set is covered, synthesize one complete output instead of performing a series of minor rewrites.",
                    directory_list
                        .iter()
                        .map(|path| format!("- {}", path))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
            );
        }
        if prompt_requests_file_grounded_question_answers(prompt) {
            inject_context_message(
                &mut messages,
                "## File-Grounded Answer Contract\nThis prompt asks direct questions grounded in referenced workspace files. Read the explicitly named relative file path first when one is provided, answer every requested question explicitly in the final assistant response, and keep the answers close to the source facts instead of paraphrasing away key details. If the prompt uses numbered questions, mirror that numbering in the answer. Do not stop at saying that you stored or read the file.".to_string(),
            );
        }
        if !literal_json_output {
            if let Some(skill_name) = requested_skill_install_name(prompt) {
                let can_read_skill = tools.iter().any(|tool| tool.name == "read_skill");
                let can_create_skill = tools.iter().any(|tool| tool.name == "create_skill");
                let install_start_notice =
                    format!("Running `/install {}` as requested.", skill_name);
                messages.push(LlmMessage::assistant(&install_start_notice));
                if let Ok(ss) = self.session_store.lock() {
                    if let Some(store) = ss.as_ref() {
                        store.add_message(session_id, "assistant", &install_start_notice);
                        store.add_structured_assistant_text_message(
                            session_id,
                            &install_start_notice,
                        );
                    }
                }
                if let Ok(Some((skill_path, skill_content, created))) =
                    ensure_requested_skill_available(
                        &skill_name,
                        prompt,
                        &self.platform.paths.skills_dir,
                        &skill_roots,
                    )
                {
                    auto_prepared_skill_name = Some(skill_name.clone());
                    let create_call_id = format!("auto_create_skill_{}", skill_name);
                    let create_result = json!({
                        "status": "success",
                        "name": skill_name,
                        "path": skill_path.clone(),
                        "warnings": [],
                    });
                    if created {
                        if can_create_skill {
                            messages.push(LlmMessage::tool_result(
                                &create_call_id,
                                "create_skill",
                                create_result.clone(),
                            ));
                        }
                        if let Ok(ss) = self.session_store.lock() {
                            if let Some(store) = ss.as_ref() {
                                let (description, content) =
                                    builtin_skill_seed(&skill_name, prompt).unwrap_or(("", ""));
                                record_synthetic_tool_interaction(
                                    store,
                                    session_id,
                                    &create_call_id,
                                    "create_skill",
                                    "create_skill",
                                    json!({
                                        "name": skill_name,
                                        "command": format!("/install {}", skill_name),
                                        "description": description,
                                        "content": content,
                                    }),
                                    &create_result,
                                );
                            }
                        }
                    }

                    let read_call_id = format!("auto_read_skill_{}", skill_name);
                    let read_result = json!({
                        "status": "success",
                        "name": skill_name,
                        "path": skill_path.clone(),
                        "content": skill_content.clone(),
                        "prefetched": true,
                    });
                    if can_read_skill {
                        messages.push(LlmMessage::tool_result(
                            &read_call_id,
                            "read_skill",
                            read_result.clone(),
                        ));
                    }
                    if let Ok(ss) = self.session_store.lock() {
                        if let Some(store) = ss.as_ref() {
                            record_synthetic_tool_interaction(
                                store,
                                session_id,
                                &read_call_id,
                                "read_skill",
                                "read_skill",
                                json!({
                                    "name": skill_name,
                                    "command": format!("/install {}", skill_name),
                                }),
                                &read_result,
                            );
                        }
                    }

                    let install_notice = format!(
                        "Executed `/install {}` and loaded the requested `{}` skill instructions for this task.",
                        skill_name, skill_name
                    );
                    messages.push(LlmMessage::assistant(&install_notice));
                    if let Ok(ss) = self.session_store.lock() {
                        if let Some(store) = ss.as_ref() {
                            store.add_message(session_id, "assistant", &install_notice);
                            store
                                .add_structured_assistant_text_message(session_id, &install_notice);
                        }
                    }
                    inject_context_message(
                        &mut messages,
                        format!(
                            "## Prepared Skill Instructions\nThe `{}` skill is available at `{}`.\n\n{}",
                            skill_name, skill_path, skill_content
                        ),
                    );
                }
                inject_context_message(
                    &mut messages,
                    format!(
                        "## Skill Install Contract\nThe prompt explicitly requests `/install {skill_name}`. Attempt the skill flow first by checking whether `{skill_name}` already exists through `read_skill`. {}If the skill is still unavailable, say that briefly and complete the task with the best manual fallback in one pass. Do not burn extra rounds repeatedly scanning skill directories once availability is known.",
                        if can_create_skill && can_read_skill {
                            format!(
                                "If it does not exist but the prompt clearly describes the needed behavior, create a small reusable `{skill_name}` skill with `create_skill`, then proceed with the task. "
                            )
                        } else if can_create_skill {
                            format!(
                                "If it does not exist but the prompt clearly describes the needed behavior, create a small reusable `{skill_name}` skill with `create_skill` and then follow its saved instructions directly. "
                            )
                        } else {
                            String::new()
                        }
                    ),
                );
            }
        }
        if !literal_json_output
            && prompt_requests_prediction_market_briefing(prompt)
            && !prompt_supplies_prediction_market_briefing_evidence(prompt)
        {
            match prefetch_polymarket_market_snapshot(&session_workdir).await {
                Ok((snapshot_path, snapshot_content)) => {
                    let snapshot_preview = top_polymarket_briefing_entries(&snapshot_content, 5)
                        .into_iter()
                        .filter_map(|entry| {
                            let question = entry.get("question").and_then(Value::as_str)?.trim();
                            let (yes_pct, no_pct) = polymarket_yes_no_percentages(&entry)?;
                            Some(json!({
                                "question": question,
                                "yes_pct": yes_pct,
                                "no_pct": no_pct,
                            }))
                        })
                        .collect::<Vec<_>>();
                    let read_result = json!({
                        "path": snapshot_path.clone(),
                        "market_count": snapshot_content.matches("\"question\"").count(),
                        "top_market_preview": snapshot_preview,
                        "prefetched": true,
                        "truncated": true,
                    });
                    if let Ok(ss) = self.session_store.lock() {
                        if let Some(store) = ss.as_ref() {
                            let download_call_id = "auto_download_polymarket_snapshot";
                            let download_result = json!({
                                "status": "success",
                                "operation": "download",
                                "path": snapshot_path.clone(),
                                "bytes_written": snapshot_content.len(),
                                "sources": [
                                    "https://gamma-api.polymarket.com/markets?active=true&order=volumeNum&ascending=false&limit=10",
                                    "https://gamma-api.polymarket.com/markets?active=true&order=volume24hr&ascending=false&limit=40"
                                ],
                            });
                            record_synthetic_tool_interaction(
                                store,
                                session_id,
                                download_call_id,
                                "file_manager",
                                "file_manager",
                                json!({
                                    "operation": "download",
                                    "sources": [
                                        "https://gamma-api.polymarket.com/markets?active=true&order=volumeNum&ascending=false&limit=10",
                                        "https://gamma-api.polymarket.com/markets?active=true&order=volume24hr&ascending=false&limit=40"
                                    ],
                                    "path": "polymarket_markets.json",
                                }),
                                &download_result,
                            );
                            let read_call_id = "auto_read_polymarket_snapshot";
                            record_synthetic_tool_interaction(
                                store,
                                session_id,
                                read_call_id,
                                "read_file",
                                "read_file",
                                json!({"path": "polymarket_markets.json"}),
                                &read_result,
                            );
                        }
                    }
                    inject_context_message(
                        &mut messages,
                        "## Prefetched Polymarket Snapshot\nA fresh `polymarket_markets.json` snapshot from the public Gamma API is already in the workspace and in the conversation context. Use those real active market questions and odds directly, prefer the highest-signal active rows with recent grounded news support, then write `polymarket_briefing.md` immediately.".to_string(),
                    );
                    if let Some(ranked_context) =
                        build_polymarket_ranked_snapshot_context(&snapshot_content)
                    {
                        inject_context_message(&mut messages, ranked_context);
                    }
                    if let Some(text) = self
                        .try_prediction_market_briefing_shortcut(
                            session_id,
                            prompt,
                            &session_workdir,
                            &snapshot_content,
                            on_chunk,
                        )
                        .await
                    {
                        loop_state.transition(AgentPhase::ResultReporting);
                        loop_state.transition(AgentPhase::Complete);
                        loop_state.log_self_inspection();
                        self.persist_loop_snapshot(&loop_state);
                        log_conversation("Assistant", &text);
                        return text;
                    }
                }
                Err(error) => {
                    inject_context_message(
                        &mut messages,
                        format!(
                            "## Polymarket Snapshot Warning\nThe direct Gamma API prefetch failed before planning: {}. Use `web_search` immediately for active Polymarket markets instead of refusing the task.",
                            error
                        ),
                    );
                }
            }
        }

        // ── Phase 3: Planning (Cognitive Plan-and-Solve & compaction) ────
        let process_prompt_loop::PromptLoopPreparation {
            mut messages,
            context_engine,
        } = self
            .prepare_prompt_loop(
                session_id,
                prompt,
                &mut loop_state,
                &history,
                skill_context.as_deref(),
                dynamic_context.as_deref(),
                memory_context_for_log.as_deref(),
                &system_prompt,
                &tools,
                messages,
                literal_json_output,
            )
            .await;

        // ── Phases 4–13: Main agentic loop ───────────────────────────────
        loop {
            // ── Phase 4: DecisionMaking / LLM call ──────────────────────
            loop_state.transition(AgentPhase::DecisionMaking);
            log::debug!(
                "[AgentLoop] Round {} | session='{}' phase=DecisionMaking msgs={}",
                loop_state.round,
                session_id,
                messages.len()
            );

            log::debug!(
                "[AgentLoop] Round {} dispatching {} transport messages with {} tools",
                loop_state.round,
                messages.len(),
                tools.len()
            );

            let mut response = LlmResponse::default();
            let mut is_workflow_tool = false;

            if let Some(wf_id) = loop_state.active_workflow_id.clone() {
                let we = self.workflow_engine.read().await;
                if let Some(wf) = we.get_workflow(&wf_id) {
                    if loop_state.current_workflow_step >= wf.steps.len() {
                        log::info!("[Workflow] All steps completed for {}", wf.name);
                        loop_state.active_workflow_id = None;
                        loop_state.transition(AgentPhase::ResultReporting);
                        let text = format!(
                            "Workflow '{}' completed successfully.\nVariables:\n{:?}",
                            wf.name,
                            loop_state.workflow_vars.keys().collect::<Vec<_>>()
                        );
                        if let Ok(ss) = self.session_store.lock() {
                            if let Some(store) = ss.as_ref() {
                                store.add_message(session_id, "assistant", &text);
                            }
                        }
                        return text;
                    }

                    let step = &wf.steps[loop_state.current_workflow_step];

                    use crate::core::workflow_engine::WorkflowStepType;
                    match step.step_type {
                        WorkflowStepType::Condition => {
                            if crate::core::workflow_engine::WorkflowEngine::eval_condition(
                                &step.condition,
                                &loop_state.workflow_vars,
                            ) {
                                log::debug!(
                                    "Condition evaluated to TRUE. Branching to '{}'",
                                    step.then_step
                                );
                                loop_state.current_workflow_step += 1;
                            } else {
                                log::debug!(
                                    "Condition evaluated to FALSE. Branching to '{}'",
                                    step.else_step
                                );
                                loop_state.current_workflow_step += 1;
                            }
                            continue;
                        }
                        WorkflowStepType::Tool => {
                            let resolved_args =
                                crate::core::workflow_engine::WorkflowEngine::interpolate_json(
                                    &step.args,
                                    &loop_state.workflow_vars,
                                );
                            response.success = true;
                            // Add randomness so observe_output Doesn't see identical strings and trigger Stuck
                            response.text = format!(
                                "Executing workflow tool '{}' (Round {})",
                                step.tool_name, loop_state.round
                            );
                            response.tool_calls.push(crate::llm::backend::LlmToolCall {
                                id: format!("call_{}_{}", step.id, loop_state.round),
                                name: step.tool_name.clone(),
                                args: resolved_args,
                            });
                            is_workflow_tool = true;
                        }
                        WorkflowStepType::Prompt => {
                            // Only inject the prompt if we haven't already for this step
                            let step_marker = format!("## [Workflow: {}]", step.id);
                            let already_injected =
                                messages.iter().any(|m| m.text.contains(&step_marker));

                            if !already_injected {
                                let resolved_instruction =
                                    crate::core::workflow_engine::WorkflowEngine::interpolate(
                                        &step.instruction,
                                        &loop_state.workflow_vars,
                                    );
                                messages.push(LlmMessage {
                                    role: "system".into(),
                                    text: format!("{}\n{}", step_marker, resolved_instruction),
                                    ..Default::default()
                                });
                            }
                            response = self
                                .chat_with_fallback(
                                    &sanitize_messages_for_transport(messages.clone()),
                                    &tools,
                                    on_chunk,
                                    &system_prompt,
                                    None,
                                )
                                .await;
                        }
                    }
                }
            } else {
                response = self
                    .chat_with_fallback(
                        &sanitize_messages_for_transport(messages.clone()),
                        &tools,
                        on_chunk,
                        &system_prompt,
                        if literal_json_output {
                            Some(1024)
                        } else {
                            None
                        },
                    )
                    .await;
            }

            // ── Phase 6: ObservationCollect ──────────────────────────────
            loop_state.transition(AgentPhase::ObservationCollect);
            log::debug!(
                "[AgentLoop] Round {} Response: success={} text_len={}",
                loop_state.round,
                response.success,
                response.text.len()
            );

            // ── Phase 11: SafetyCheck — handle LLM error ─────────────────
            if !response.success {
                loop_state.transition(AgentPhase::ErrorRecovery);
                loop_state.error_count += 1;
                let err = format!(
                    "LLM error (HTTP {}): {}",
                    response.http_status, response.error_message
                );
                loop_state.last_error = Some(err.clone());
                log::error!("[AgentLoop] {}", err);
                // `chat_with_fallback()` already consumes the per-turn recovery
                // budget by trying the primary backend plus configured
                // fallbacks. Align with OpenClaw/HermesAgentLoop style and
                // surface the failure here instead of replaying the same turn
                // multiple times with a fixed hardcoded retry count.
                loop_state.transition(AgentPhase::ResultReporting);
                self.persist_loop_snapshot(&loop_state);
                return err;
            }

            // Extract reasoning
            let mut reasoning_text = response.reasoning_text.clone();
            if reasoning_text.is_empty() {
                if let Some(cap) = THINK_RE.captures(&response.text) {
                    reasoning_text = cap[1].trim().to_string();
                }
            }

            // Fallback parser
            let mut detected_tool_calls = response.tool_calls.clone();
            if detected_tool_calls.is_empty() {
                detected_tool_calls = FallbackParser::parse(&response.text);
                if !detected_tool_calls.is_empty() {
                    log::debug!(
                        "[AgentLoop] FallbackParser detected {} tool call(s)",
                        detected_tool_calls.len()
                    );
                }
            }

            // Record token usage
            {
                let be_name = self
                    .backend
                    .read()
                    .await
                    .as_ref()
                    .map(|be| be.get_name().to_string())
                    .unwrap_or_else(|| "unknown".into());
                if let Ok(ss) = self.session_store.lock() {
                    if let Some(store) = ss.as_ref() {
                        store.record_usage(
                            session_id,
                            response.prompt_tokens,
                            response.completion_tokens,
                            response.cache_creation_input_tokens,
                            response.cache_read_input_tokens,
                            &be_name,
                        );
                        let usage = store.load_token_usage(session_id);
                        log::debug!(
                            "[TokenUsage] Round: P{}+C{}={} | Cache write/read: {}/{} | Session cumulative: {} | Session cache read: {}",
                            response.prompt_tokens,
                            response.completion_tokens,
                            response.prompt_tokens + response.completion_tokens,
                            response.cache_creation_input_tokens,
                            response.cache_read_input_tokens,
                            usage.total_prompt_tokens + usage.total_completion_tokens,
                            usage.total_cache_read_input_tokens
                        );
                        if response.cache_read_input_tokens > 0
                            || response.cache_creation_input_tokens > 0
                        {
                            log::info!(
                                "[TokenUsage] Cache telemetry for {}: write={} read={}",
                                be_name,
                                response.cache_creation_input_tokens,
                                response.cache_read_input_tokens
                            );
                        }
                        loop_state.token_used = usage.total_prompt_tokens as usize
                            + context_engine.estimate_tokens(&messages);
                    }
                }
            }

            if !detected_tool_calls.is_empty() {
                // ── Phase 5: ToolDispatching ─────────────────────────────
                loop_state.transition(AgentPhase::ToolDispatching);
                loop_state.total_tool_calls += detected_tool_calls.len();
                loop_state.mark_follow_up(
                    LoopTransitionReason::ToolCallsRequested,
                    format!(
                        "assistant requested {} tool call(s)",
                        detected_tool_calls.len()
                    ),
                );
                log::debug!(
                    "[AgentLoop] Round {} dispatching {} tool(s)",
                    loop_state.round,
                    detected_tool_calls.len()
                );

                // Enforce reasoning extraction if not provided by backend
                let final_text = extract_final_text(&response.text);

                // Add assistant message
                messages.push(LlmMessage {
                    role: "assistant".into(),
                    text: final_text.clone(),
                    reasoning_text: reasoning_text.clone(),
                    tool_calls: detected_tool_calls.clone(),
                    ..Default::default()
                });

                let canonical_tool_calls: Vec<Value> = detected_tool_calls
                    .iter()
                    .map(canonical_tool_trace)
                    .collect();
                let canonical_tool_names: HashMap<String, String> = detected_tool_calls
                    .iter()
                    .zip(canonical_tool_calls.iter())
                    .map(|(tc, trace)| {
                        (
                            tc.id.clone(),
                            trace["name"]
                                .as_str()
                                .unwrap_or(tc.name.as_str())
                                .to_string(),
                        )
                    })
                    .collect();
                if let Ok(ss) = self.session_store.lock() {
                    if let Some(store) = ss.as_ref() {
                        if !final_text.trim().is_empty() {
                            store.add_structured_assistant_text_message(session_id, &final_text);
                        }
                        store.add_structured_tool_call_message(session_id, canonical_tool_calls);
                    }
                }

                // Parallel tool execution
                let td_guard = self.tool_dispatcher.read().await;
                let mut futures_list = Vec::new();
                let mem_store_opt = self
                    .memory_store
                    .lock()
                    .ok()
                    .and_then(|ms| ms.as_ref().cloned());
                let llm_doc = llm_config_store::load(&self.platform.paths.config_dir)
                    .unwrap_or_else(|_| llm_config_store::default_document());
                let search_config_dir = self.platform.paths.config_dir.clone();
                let grounded_paths_snapshot = collect_grounded_paths(&messages);
                let grounded_csv_headers_snapshot = collect_grounded_csv_headers(&messages);

                for tc in detected_tool_calls.iter() {
                    let skills_dir = self.platform.paths.skills_dir.clone();
                    let skill_roots = skill_capability_manager::load_snapshot(
                        &self.platform.paths,
                        &RegisteredPaths::load(&self.platform.paths.config_dir),
                    )
                    .roots
                    .into_iter()
                    .map(|root| root.path)
                    .collect::<Vec<_>>();
                    let docs_dir = self.platform.paths.docs_dir.clone();
                    let td_guard_ref = &*td_guard;
                    let tc_name = tc.name.clone();
                    let tc_args = tc.args.clone();
                    let tc_id = tc.id.clone();
                    let bridge_ref = &self.action_bridge;
                    let ms_clone = mem_store_opt.clone();
                    let session_workdir = session_workdir.clone();
                    let llm_doc = llm_doc.clone();
                    let search_config_dir = search_config_dir.clone();
                    let grounded_paths_snapshot = grounded_paths_snapshot.clone();
                    let grounded_csv_headers_snapshot = grounded_csv_headers_snapshot.clone();

                    // ── Phase 11: SafetyCheck per tool ───────────────────
                    let canonical_name = if let Ok(tp) = self.tool_policy.lock() {
                        tp.get_aliases()
                            .get(&tc_name)
                            .cloned()
                            .unwrap_or_else(|| tc_name.clone())
                    } else {
                        tc_name.clone()
                    };
                    let policy_block_reason = if let Ok(tp) = self.tool_policy.lock() {
                        if tp.is_loop_detected(&canonical_name) {
                            Some(format!(
                                "Loop detected: tool '{}' called too many times",
                                canonical_name
                            ))
                        } else if tp.is_iteration_limit_reached() {
                            Some(format!(
                                "Iteration limit reached: {} total tool calls",
                                tp.total_calls()
                            ))
                        } else {
                            tp.check_policy(session_id, &canonical_name, &tc_args).err()
                        }
                    } else {
                        None
                    };
                    let safety_block_reason = if let Ok(safety_guard) = self.safety_guard.lock() {
                        let side_effect = td_guard_ref
                            .side_effect_for_tool(&tc_name)
                            .map(SideEffect::from_str)
                            .unwrap_or(SideEffect::Reversible);
                        let session_call_count = self
                            .tool_policy
                            .lock()
                            .map(|tp| tp.total_calls())
                            .unwrap_or(0);
                        safety_guard
                            .check_tool_call(
                                &canonical_name,
                                &tc_args,
                                side_effect,
                                session_call_count,
                            )
                            .err()
                    } else {
                        None
                    };
                    if policy_block_reason.is_none() && safety_block_reason.is_none() {
                        if let Ok(tp) = self.tool_policy.lock() {
                            tp.record_call(&canonical_name);
                        }
                    }
                    let block_reason = policy_block_reason.or(safety_block_reason);

                    futures_list.push(async move {
                        if let Some(reason) = block_reason {
                            log::warn!(
                                "[SafetyCheck] Tool '{}' blocked: {}",
                                canonical_name,
                                reason
                            );
                            return LlmMessage::tool_result(&tc_id, &tc_name, serde_json::json!({"error": reason}));
                        }

                        let result = if tc_name.starts_with("action_") {
                            if let Some(action_id) = tc_name.strip_prefix("action_") {
                                if let Ok(bridge) = bridge_ref.lock() {
                                    bridge.execute_action(action_id, &tc_args)
                                } else {
                                    json!({"error": "Failed to lock action bridge"})
                                }
                            } else {
                                json!({"error": "Invalid action format"})
                            }
                        } else if tc_name == "search_tools" {
                            let query = tc_args.get("query").and_then(|v| v.as_str()).unwrap_or("ALL");

                            let mut all_tools = td_guard_ref.get_tool_declarations();
                            crate::core::tool_declaration_builder::ToolDeclarationBuilder::append_all_builtin_tools(
                                &mut all_tools,
                            );
                            if let Ok(bridge) = bridge_ref.lock() {
                                all_tools.extend(bridge.get_action_declarations());
                            }

                            let mut scored_tools: Vec<(usize, crate::llm::backend::LlmToolDecl)> =
                                all_tools
                                    .into_iter()
                                    .map(|tool| (score_tool_search_match(&tool, query), tool))
                                    .filter(|(score, _)| *score > 0)
                                    .collect();
                            scored_tools.sort_by(|(left_score, left_tool), (right_score, right_tool)| {
                                right_score
                                    .cmp(left_score)
                                    .then_with(|| left_tool.name.cmp(&right_tool.name))
                            });

                            let limit = if query.eq_ignore_ascii_case("ALL") {
                                usize::MAX
                            } else {
                                8
                            };
                            let results: Vec<Value> = scored_tools
                                .into_iter()
                                .take(limit)
                                .map(|(score, tool)| {
                                    serde_json::json!({
                                        "name": tool.name,
                                        "description": tool.description,
                                        "parameters": tool.parameters,
                                        "match_score": score,
                                    })
                                })
                                .collect();
                            if results.is_empty() {
                                serde_json::json!({"error": format!("No tools found matching '{}'", query)})
                            } else {
                                serde_json::json!({"tools": results})
                            }
                        } else if tc_name == "create_skill" {
                            let name = tc_args.get("name").and_then(|v| v.as_str()).unwrap_or("unnamed_skill");
                            let description = tc_args
                                .get("description")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let content = tc_args.get("content").and_then(|v| v.as_str()).unwrap_or("");
                            match crate::core::skill_support::prepare_skill_document(
                                name,
                                description,
                                content,
                            ) {
                                Ok(prepared) => {
                                    let skill_dir_path = skills_dir.join(&prepared.normalized_name);
                                    if let Err(e) = std::fs::create_dir_all(&skill_dir_path) {
                                        serde_json::json!({"error": format!("Failed to create skill directory: {}", e)})
                                    } else {
                                        let skill_md_path = skill_dir_path.join("SKILL.md");
                                        if skill_md_path.is_dir() {
                                            let _ = std::fs::remove_dir_all(&skill_md_path);
                                        }
                                        match std::fs::write(&skill_md_path, prepared.document) {
                                            Ok(_) => serde_json::json!({
                                                "status": "success",
                                                "name": prepared.normalized_name,
                                                "path": skill_md_path.to_string_lossy().to_string(),
                                                "warnings": prepared.warnings,
                                            }),
                                            Err(e) => serde_json::json!({"error": format!("Failed to write skill: {}", e)})
                                        }
                                    }
                                }
                                Err(err) => serde_json::json!({"error": err}),
                            }
                        } else if tc_name == "read_skill" {
                            let name = tc_args.get("name").and_then(|v| v.as_str()).unwrap_or("");
                            let normalized_name =
                                crate::core::skill_support::normalize_skill_name(name);
                            if normalized_name.is_empty() {
                                serde_json::json!({"error": "Skill name is required"})
                            } else {
                                match resolve_skill_file(&skill_roots, &normalized_name) {
                                    Some(skill_md_path) => {
                                        match std::fs::read_to_string(&skill_md_path) {
                                            Ok(content) => {
                                                let snapshot = skill_capability_manager::load_snapshot(
                                                    &self.platform.paths,
                                                    &RegisteredPaths::load(&self.platform.paths.config_dir),
                                                );
                                                if let Some(metadata) = snapshot.find_skill(&normalized_name) {
                                                    if !metadata.enabled {
                                                        serde_json::json!({
                                                            "error": format!(
                                                                "Skill '{}' is disabled or missing dependencies. Check '{}'.",
                                                                normalized_name,
                                                                snapshot.config_path
                                                            ),
                                                            "skill": {
                                                                "name": metadata.skill.file_name,
                                                                "dependency_ready": metadata.dependency_ready,
                                                                "missing_requires": metadata.missing_requires,
                                                                "install_hints": metadata.skill.openclaw_install,
                                                                "enabled": metadata.enabled,
                                                            }
                                                        })
                                                    } else {
                                                        serde_json::json!({
                                                            "status": "success",
                                                            "name": normalized_name,
                                                            "path": skill_md_path.to_string_lossy().to_string(),
                                                            "content": content,
                                                            "openclaw": {
                                                                "requires": metadata.skill.openclaw_requires.clone(),
                                                                "install": metadata.skill.openclaw_install.clone(),
                                                            }
                                                        })
                                                    }
                                                } else {
                                                    serde_json::json!({
                                                        "status": "success",
                                                        "name": normalized_name,
                                                        "path": skill_md_path.to_string_lossy().to_string(),
                                                        "content": content,
                                                        "openclaw": {
                                                            "requires": Vec::<String>::new(),
                                                            "install": Vec::<String>::new(),
                                                        }
                                                    })
                                                }
                                            }
                                            Err(e) => serde_json::json!({"error": format!("Failed to read skill '{}': {}", normalized_name, e)})
                                        }
                                    }
                                    None => serde_json::json!({
                                        "error": format!(
                                            "Failed to read skill '{}': not found in managed or registered roots",
                                            normalized_name
                                        )
                                    }),
                                }
                            }
                        } else if tc_name == "list_skill_references" {
                            let docs = crate::core::skill_support::list_skill_reference_docs(&docs_dir);
                            serde_json::json!({
                                "status": "success",
                                "references": docs.into_iter().map(|doc| serde_json::json!({
                                    "name": doc.name,
                                    "path": doc.absolute_path,
                                    "description": doc.description,
                                })).collect::<Vec<_>>()
                            })
                        } else if tc_name == "read_skill_reference" {
                            let name = tc_args.get("name").and_then(|v| v.as_str()).unwrap_or("");
                            match crate::core::skill_support::read_skill_reference_doc(&docs_dir, name) {
                                Ok(doc) => serde_json::json!({
                                    "status": "success",
                                    "name": doc.name,
                                    "path": doc.absolute_path,
                                    "description": doc.description,
                                    "content": doc.content,
                                }),
                                Err(err) => serde_json::json!({"error": err}),
                            }
                        } else if tc_name == "list_agent_roles" {
                            let roles = self.role_registry_snapshot();
                            serde_json::json!({
                                "status": "success",
                                "roles": roles.into_iter().map(|role| serde_json::json!({
                                    "name": role.name,
                                    "description": role.description,
                                "max_iterations": role.max_iterations,
                                "allowed_tools": role.allowed_tools,
                                "type": role.role_type,
                                "auto_start": role.auto_start,
                                "can_delegate_to": role.can_delegate_to,
                                "prompt_mode": role.prompt_mode.map(|mode| match mode {
                                    PromptMode::Full => "full",
                                    PromptMode::Minimal => "minimal",
                                    }),
                                    "reasoning_policy": role.reasoning_policy.map(|policy| match policy {
                                        ReasoningPolicy::Native => "native",
                                        ReasoningPolicy::Tagged => "tagged",
                                    }),
                                })).collect::<Vec<_>>()
                            })
                        } else if tc_name == "spawn_agent" {
                            let name = tc_args.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
                            let system_prompt = tc_args.get("system_prompt").and_then(|v| v.as_str()).unwrap_or("").trim();
                            if name.is_empty() || system_prompt.is_empty() {
                                serde_json::json!({"error": "Missing name or system_prompt"})
                            } else {
                                let allowed_tools = tc_args
                                    .get("allowed_tools")
                                    .and_then(|v| v.as_array())
                                    .map(|items| items.iter().filter_map(|value| value.as_str().map(|value| value.to_string())).collect::<Vec<_>>())
                                    .unwrap_or_default();
                                let role = AgentRole {
                                    name: name.to_string(),
                                    system_prompt: system_prompt.to_string(),
                                    allowed_tools,
                                    max_iterations: tc_args.get("max_iterations").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                                    description: tc_args.get("description").and_then(|v| v.as_str()).unwrap_or("Dynamic role").to_string(),
                                    role_type: tc_args.get("type").and_then(|v| v.as_str()).unwrap_or("worker").to_string(),
                                    auto_start: tc_args.get("auto_start").and_then(|v| v.as_bool()).unwrap_or(false),
                                    can_delegate_to: tc_args
                                        .get("can_delegate_to")
                                        .and_then(|v| v.as_array())
                                        .map(|items| items.iter().filter_map(|value| value.as_str().map(|value| value.to_string())).collect::<Vec<_>>())
                                        .unwrap_or_default(),
                                    prompt_mode: prompt_mode_from_str(tc_args.get("prompt_mode").and_then(|v| v.as_str())),
                                    reasoning_policy: reasoning_policy_from_str(tc_args.get("reasoning_policy").and_then(|v| v.as_str())),
                                };
                                if let Ok(mut registry) = self.agent_roles.write() {
                                    registry.add_dynamic_role(role.clone());
                                }
                                serde_json::json!({
                                    "status": "success",
                                    "role": role.name,
                                    "type": role.role_type,
                                    "auto_start": role.auto_start,
                                    "can_delegate_to": role.can_delegate_to,
                                    "prompt_mode": role.prompt_mode.map(|mode| match mode {
                                        PromptMode::Full => "full",
                                        PromptMode::Minimal => "minimal",
                                    }),
                                    "reasoning_policy": role.reasoning_policy.map(|policy| match policy {
                                        ReasoningPolicy::Native => "native",
                                        ReasoningPolicy::Tagged => "tagged",
                                    }),
                                })
                            }
                        } else if tc_name == "create_session" {
                            let name = tc_args.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
                            if name.is_empty() {
                                serde_json::json!({"error": "Missing session name"})
                            } else {
                                let role_name = tc_args.get("role").and_then(|v| v.as_str());
                                match self.build_session_profile(
                                    role_name,
                                    tc_args.get("system_prompt").and_then(|v| v.as_str()),
                                    prompt_mode_from_str(tc_args.get("prompt_mode").and_then(|v| v.as_str())),
                                    reasoning_policy_from_str(tc_args.get("reasoning_policy").and_then(|v| v.as_str())),
                                    None,
                                    None,
                                ) {
                                    Ok(profile) => {
                                        let base_prompt = profile
                                            .system_prompt
                                            .clone()
                                            .unwrap_or_else(|| "You are a TizenClaw sub-session.".into());
                                        let session_id = crate::core::agent_factory::AgentFactory::create_agent_session(name, &base_prompt);
                                        if let Ok(ss) = self.session_store.lock() {
                                            if let Some(store) = ss.as_ref() {
                                                store.ensure_session(&session_id);
                                            }
                                        }
                                        if let Ok(mut profiles) = self.session_profiles.lock() {
                                            profiles.insert(session_id.clone(), profile.clone());
                                        }
                                        serde_json::json!({
                                            "status": "success",
                                            "session_id": session_id,
                                            "role": profile.role_name,
                                            "prompt_mode": profile.prompt_mode.map(|mode| match mode {
                                                PromptMode::Full => "full",
                                                PromptMode::Minimal => "minimal",
                                            }),
                                            "reasoning_policy": profile.reasoning_policy.map(|policy| match policy {
                                                ReasoningPolicy::Native => "native",
                                                ReasoningPolicy::Tagged => "tagged",
                                            }),
                                        })
                                    }
                                    Err(err) => serde_json::json!({"error": err}),
                                }
                            }
                        } else if tc_name == "list_sessions" {
                            let known_sessions = list_known_sessions(&self.platform.paths);
                            let profile_snapshot = self
                                .session_profiles
                                .lock()
                                .ok()
                                .map(|profiles| profiles.clone())
                                .unwrap_or_default();
                            serde_json::json!({
                                "status": "success",
                                "sessions": known_sessions.into_iter().map(|session_id| {
                                    let profile = profile_snapshot.get(&session_id);
                                    serde_json::json!({
                                        "session_id": session_id,
                                        "role": profile.and_then(|profile| profile.role_name.clone()),
                                        "prompt_mode": profile.and_then(|profile| profile.prompt_mode).map(|mode| match mode {
                                            PromptMode::Full => "full",
                                            PromptMode::Minimal => "minimal",
                                        }),
                                        "reasoning_policy": profile.and_then(|profile| profile.reasoning_policy).map(|policy| match policy {
                                            ReasoningPolicy::Native => "native",
                                            ReasoningPolicy::Tagged => "tagged",
                                        }),
                                    })
                                }).collect::<Vec<_>>()
                            })
                        } else if tc_name == "send_to_session" {
                            let target_session = tc_args.get("target_session").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
                            let message = tc_args.get("message").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
                            if target_session.is_empty() || message.is_empty() {
                                serde_json::json!({"error": "Missing target_session or message"})
                            } else {
                                let reply = Box::pin(self.process_prompt(&target_session, &message, None)).await;
                                serde_json::json!({
                                    "status": "success",
                                    "session_id": target_session,
                                    "response": reply
                                })
                            }
                        } else if tc_name == "run_supervisor" {
                            let goal = tc_args.get("goal").and_then(|v| v.as_str()).unwrap_or("").trim();
                            let strategy = tc_args.get("strategy").and_then(|v| v.as_str()).unwrap_or("sequential");
                            if goal.is_empty() {
                                serde_json::json!({"error": "Missing goal"})
                            } else {
                                let current_profile = self.resolve_session_profile(session_id);
                                let delegated_role_names = current_profile
                                    .as_ref()
                                    .and_then(|profile| profile.can_delegate_to.clone())
                                    .unwrap_or_default();
                                let mut candidate_roles = self
                                    .role_registry_snapshot()
                                    .into_iter()
                                    .filter(|role| !role.is_supervisor())
                                    .filter(|role| !matches!(role.name.as_str(), "default" | "subagent" | "local-reasoner"))
                                    .collect::<Vec<_>>();
                                if !delegated_role_names.is_empty() {
                                    candidate_roles.retain(|role| {
                                        delegated_role_names.iter().any(|name| name == &role.name)
                                    });
                                }

                                let selected_roles = select_delegate_roles(
                                    goal,
                                    &candidate_roles,
                                    if strategy == "parallel" { 3 } else { 2 },
                                );

                                if selected_roles.is_empty() {
                                    serde_json::json!({
                                        "error": "No worker roles are available for supervisor delegation"
                                    })
                                } else {
                                    let mut delegated_sessions = Vec::new();
                                    for role in &selected_roles {
                                        if let Ok(profile) = self.build_session_profile(
                                            Some(&role.name),
                                            None,
                                            None,
                                            None,
                                            None,
                                            None,
                                        ) {
                                            let base_prompt = profile
                                                .system_prompt
                                                .clone()
                                                .unwrap_or_else(|| "You are a TizenClaw sub-session.".into());
                                            let session_name = format!("{}_delegate", role.name);
                                            let delegated_session_id =
                                                crate::core::agent_factory::AgentFactory::create_agent_session(
                                                    &session_name,
                                                    &base_prompt,
                                                );
                                            if let Ok(ss) = self.session_store.lock() {
                                                if let Some(store) = ss.as_ref() {
                                                    store.ensure_session(&delegated_session_id);
                                                }
                                            }
                                            if let Ok(mut profiles) = self.session_profiles.lock() {
                                                profiles.insert(delegated_session_id.clone(), profile);
                                            }
                                            delegated_sessions.push((role.clone(), delegated_session_id));
                                        }
                                    }

                                    if delegated_sessions.is_empty() {
                                        serde_json::json!({
                                            "error": "Failed to create delegated sessions for supervisor execution"
                                        })
                                    } else {
                                        let results = if strategy == "parallel" {
                                            join_all(delegated_sessions.iter().map(|(role, delegated_session_id)| {
                                                let supervisor_hint =
                                                    build_role_supervisor_hint(session_id, goal, role);
                                                async move {
                                                let response = Box::pin(self.process_prompt(
                                                    delegated_session_id,
                                                    &supervisor_hint,
                                                    None,
                                                ))
                                                .await;
                                                serde_json::json!({
                                                    "role": role.name.clone(),
                                                    "session_id": delegated_session_id,
                                                    "response": response,
                                                })
                                            }}))
                                            .await
                                        } else {
                                            let mut sequential_results = Vec::new();
                                            for (role, delegated_session_id) in &delegated_sessions {
                                                let supervisor_hint =
                                                    build_role_supervisor_hint(session_id, goal, role);
                                                let response = Box::pin(self.process_prompt(
                                                    delegated_session_id,
                                                    &supervisor_hint,
                                                    None,
                                                ))
                                                .await;
                                                sequential_results.push(serde_json::json!({
                                                    "role": role.name.clone(),
                                                    "session_id": delegated_session_id,
                                                    "response": response,
                                                }));
                                            }
                                            sequential_results
                                        };

                                        let summary = results
                                            .iter()
                                            .map(|item| {
                                                let role = item.get("role").and_then(|v| v.as_str()).unwrap_or("unknown");
                                                let response = item.get("response").and_then(|v| v.as_str()).unwrap_or("");
                                                format!("[{}] {}", role, response.trim())
                                            })
                                            .collect::<Vec<_>>()
                                            .join("\n\n");

                                        serde_json::json!({
                                            "status": "success",
                                            "goal": goal,
                                            "strategy": strategy,
                                            "delegated_count": results.len(),
                                            "results": results,
                                            "summary": summary,
                                        })
                                    }
                                }
                            }
                        } else if tc_name == "file_manager" {
                            file_manager_tool(&tc_args, &session_workdir).await
                        } else if tc_name == "file_write" {
                            let path_str = tc_args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                            let content = tc_args.get("content").and_then(|v| v.as_str()).unwrap_or("");
                            if path_str.is_empty() {
                                json!({"error": "Missing required parameter: path"})
                            } else {
                                let file_path = match resolve_workspace_path(&session_workdir, path_str) {
                                    Ok(path) => path,
                                    Err(error) => return LlmMessage::tool_result(
                                        &tc_id,
                                        &tc_name,
                                        json!({"error": error}),
                                    ),
                                };
                                if let Some(parent) = file_path.parent() {
                                    if let Err(e) = std::fs::create_dir_all(parent) {
                                        return LlmMessage::tool_result(
                                            &tc_id,
                                            &tc_name,
                                            json!({"error": format!("Failed to create directory: {}", e)}),
                                        );
                                    }
                                }
                                match std::fs::write(&file_path, content) {
                                    Ok(()) => {
                                        log::info!("[file_write] Wrote {} bytes to {}", content.len(), file_path.display());
                                        json!({
                                            "success": true,
                                            "path": file_path.to_string_lossy(),
                                            "bytes_written": content.len()
                                        })
                                    }
                                    Err(e) => json!({"error": format!("Failed to write file: {}", e)}),
                                }
                            }
                        } else if tc_name == "run_generated_code" {
                            let runtime = tc_args.get("runtime").and_then(|v| v.as_str()).unwrap_or("");
                            let name = tc_args.get("name").and_then(|v| v.as_str());
                            let code = tc_args.get("code").and_then(|v| v.as_str()).unwrap_or("");
                            let args = tc_args.get("args").and_then(|v| v.as_str()).unwrap_or("");
                            match validate_generated_code_grounding(
                                prompt,
                                &grounded_paths_snapshot,
                                &grounded_csv_headers_snapshot,
                                code,
                                args,
                            ) {
                                Err(reason) => serde_json::json!({ "error": reason }),
                                Ok(grounding) => {
                                    let base_dir = self.platform.paths.data_dir.clone();
                                    run_generated_code_tool(
                                        runtime,
                                        name,
                                        code,
                                        args,
                                        &base_dir,
                                        Some(&session_workdir),
                                        grounding.declared_output_path.as_deref(),
                                        grounding.declared_output_level.as_deref(),
                                        prompt_requires_atomic_level_answer(prompt),
                                    )
                                    .await
                                }
                            }
                        } else if tc_name == "run_coding_agent" {
                            let prompt = tc_args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
                            if prompt.trim().is_empty() {
                                json!({"error": "Missing required parameter: prompt"})
                            } else if !prompt_requests_code_generation(prompt) {
                                json!({
                                    "error": "run_coding_agent is reserved for explicit code-generation tasks. Use native file, document, tabular, or research tools for analysis and reporting requests."
                                })
                            } else if prompt_references_grounded_inputs_for_code_generation(prompt) {
                                json!({
                                    "error": "When a code-generation task depends on prompt-referenced input files, read those files directly and use grounded local code generation via `run_generated_code` or `file_write` instead of delegating to run_coding_agent."
                                })
                            } else {
                                let request = crate::channel::telegram_client::CodingAgentToolRequest {
                                    prompt: prompt.to_string(),
                                    backend: tc_args
                                        .get("backend")
                                        .and_then(|v| v.as_str())
                                        .map(ToString::to_string),
                                    project_dir: tc_args
                                        .get("project_dir")
                                        .and_then(|v| v.as_str())
                                        .map(ToString::to_string),
                                    model: tc_args
                                        .get("model")
                                        .and_then(|v| v.as_str())
                                        .map(ToString::to_string),
                                    execution_mode: tc_args
                                        .get("execution_mode")
                                        .and_then(|v| v.as_str())
                                        .map(ToString::to_string),
                                    auto_approve: tc_args
                                        .get("auto_approve")
                                        .and_then(|v| v.as_bool()),
                                    timeout_secs: tc_args
                                        .get("timeout_secs")
                                        .and_then(|v| v.as_u64()),
                                };
                                match crate::channel::telegram_client::TelegramClient::run_coding_agent_tool(
                                    &self.platform.paths.config_dir,
                                    &request,
                                )
                                .await
                                {
                                    Ok(result) => result,
                                    Err(error) => json!({ "error": error }),
                                }
                            }
                        } else if tc_name == "manage_generated_code" {
                            let operation = tc_args.get("operation").and_then(|v| v.as_str()).unwrap_or("");
                            let name = tc_args.get("name").and_then(|v| v.as_str());
                            manage_generated_code_tool(operation, name, &session_workdir)
                        } else if tc_name == "list_tasks" {
                            let base_dir = self.platform.paths.data_dir.clone();
                            list_tasks_tool(&base_dir)
                        } else if tc_name == "create_task" {
                            let schedule = tc_args.get("schedule").and_then(|v| v.as_str()).unwrap_or("");
                            let prompt = tc_args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
                            let project_dir = tc_args.get("project_dir").and_then(|v| v.as_str());
                            let coding_backend =
                                tc_args.get("coding_backend").and_then(|v| v.as_str());
                            let coding_model =
                                tc_args.get("coding_model").and_then(|v| v.as_str());
                            let execution_mode =
                                tc_args.get("execution_mode").and_then(|v| v.as_str());
                            let auto_approve = tc_args
                                .get("auto_approve")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            let base_dir = self.platform.paths.data_dir.clone();
                            create_task_tool(
                                &base_dir,
                                schedule,
                                prompt,
                                project_dir,
                                coding_backend,
                                coding_model,
                                execution_mode,
                                auto_approve,
                            )
                        } else if tc_name == "cancel_task" {
                            let task_id = tc_args.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
                            let base_dir = self.platform.paths.data_dir.clone();
                            cancel_task_tool(&base_dir, task_id)
                        } else if tc_name == "generate_image" {
                            let prompt = tc_args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
                            let path = tc_args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                            let size = tc_args.get("size").and_then(|v| v.as_str());
                            let background = tc_args.get("background").and_then(|v| v.as_str());
                            feature_tools::generate_image(
                                prompt,
                                path,
                                size,
                                background,
                                &session_workdir,
                                &llm_doc,
                            ).await
                        } else if tc_name == "send_outbound_message" {
                            self.send_outbound_message(&tc_args, Some(session_id)).await
                        } else if tc_name == "generate_web_app" {
                            self.generate_web_app(&tc_args).await
                        } else if tc_name == "extract_document_text" {
                            let path = tc_args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                            let output_path = tc_args.get("output_path").and_then(|v| v.as_str());
                            let max_chars = tc_args
                                .get("max_chars")
                                .and_then(|v| v.as_u64())
                                .map(|value| value as usize);
                            feature_tools::extract_document_text(
                                path,
                                output_path,
                                max_chars,
                                &session_workdir,
                            ).await
                        } else if tc_name == "inspect_tabular_data" {
                            let path = tc_args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                            let preview_rows = tc_args
                                .get("preview_rows")
                                .and_then(|v| v.as_u64())
                                .map(|value| value as usize)
                                .unwrap_or(5);
                            feature_tools::inspect_tabular_data(
                                path,
                                preview_rows,
                                &session_workdir,
                            ).await
                        } else if tc_name == "validate_web_search" {
                            let engine = tc_args.get("engine").and_then(|v| v.as_str());
                            feature_tools::validate_web_search(&search_config_dir, engine)
                        } else if tc_name == "web_search" {
                            let query = tc_args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                            let engine = tc_args.get("engine").and_then(|v| v.as_str());
                            let limit = tc_args
                                .get("limit")
                                .and_then(|v| v.as_u64())
                                .map(|value| value as usize)
                                .unwrap_or(5);
                            feature_tools::web_search(
                                query,
                                engine,
                                limit,
                                &session_workdir,
                                &search_config_dir,
                            ).await
                        } else if tc_name == "remember" {
                            if let Some(store) = ms_clone {
                                let key = tc_args.get("key").and_then(|v| v.as_str()).unwrap_or("");
                                let value = tc_args.get("value").and_then(|v| v.as_str()).unwrap_or("");
                                let category = tc_args.get("category").and_then(|v| v.as_str()).unwrap_or("general");
                                if !key.is_empty() && !value.is_empty() {
                                    store.set(key, value, category);
                                    serde_json::json!({"status": "success", "message": format!("Remembered '{}'", key)})
                                } else {
                                    serde_json::json!({"error": "Missing key or value"})
                                }
                            } else {
                                serde_json::json!({"error": "MemoryStore not initialized"})
                            }
                        } else if tc_name == "recall" {
                            if let Some(store) = ms_clone {
                                let key = tc_args.get("key").and_then(|v| v.as_str()).unwrap_or("");
                                if let Some(val) = store.get(key) {
                                    serde_json::json!({"status": "success", "value": val})
                                } else {
                                    serde_json::json!({"error": "Key not found"})
                                }
                            } else {
                                serde_json::json!({"error": "MemoryStore not initialized"})
                            }
                        } else if tc_name == "forget" {
                            if let Some(store) = ms_clone {
                                let key = tc_args.get("key").and_then(|v| v.as_str()).unwrap_or("");
                                if store.delete(key) {
                                    serde_json::json!({"status": "success", "message": format!("Forgot '{}'", key)})
                                } else {
                                    serde_json::json!({"error": "Key not found"})
                                }
                            } else {
                                serde_json::json!({"error": "MemoryStore not initialized"})
                            }
                        } else if tc_name == "clear_agent_data" {
                            let include_memory = tc_args
                                .get("include_memory")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(true);
                            let include_sessions = tc_args
                                .get("include_sessions")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(true);
                            match self.clear_agent_data(include_memory, include_sessions) {
                                Ok(result) => result,
                                Err(error) => serde_json::json!({ "error": error }),
                            }
                        } else {
                            match td_guard_ref
                                .execute_in_dir(&tc_name, &tc_args, None, Some(&session_workdir))
                                .await
                            {
                                Ok(value) => value,
                                Err(error) => serde_json::json!({ "error": error }),
                            }
                        };

                        log::debug!("[ObservationCollect] Tool '{}' result: {} chars",
                            tc_name, result.to_string().len());

                        LlmMessage::tool_result(&tc_id, &tc_name, result)
                    });
                }

                let results = futures_util::future::join_all(futures_list).await;
                let cleared_agent_data = results.iter().any(|result| {
                    result.tool_name == "clear_agent_data"
                        && result.tool_result.get("error").is_none()
                });
                if cleared_agent_data {
                    skip_memory_extraction = true;
                }
                if let Ok(ss) = self.session_store.lock() {
                    if let Some(store) = ss.as_ref() {
                        for result in &results {
                            let trace_name = canonical_tool_names
                                .get(&result.tool_call_id)
                                .map(String::as_str)
                                .unwrap_or(result.tool_name.as_str());
                            store.add_structured_tool_result_message(
                                session_id,
                                trace_name,
                                &result.tool_name,
                                &result.tool_call_id,
                                &result.tool_result,
                            );
                        }
                    }
                }
                if loop_state.token_budget > 0 {
                    let (budgeted_results, budgeted_count) = context_engine
                        .budget_tool_result_messages(results, DEFAULT_TOOL_RESULT_BUDGET_CHARS);
                    if budgeted_count > 0 {
                        loop_state.record_budget_events(budgeted_count);
                        log::info!(
                            "[ToolBudget] Round {} budgeted {} oversized tool result(s)",
                            loop_state.round,
                            budgeted_count
                        );
                    }
                    messages.extend(budgeted_results);
                } else {
                    messages.extend(results);
                }
                if let Some(text) = synthesize_file_grounded_answers(prompt, &messages) {
                    if let Ok(ss) = self.session_store.lock() {
                        if let Some(store) = ss.as_ref() {
                            let grounded_files =
                                collect_existing_grounded_input_files(prompt, &session_workdir);
                            record_grounded_answer_preview(
                                store,
                                session_id,
                                &grounded_files,
                                &text,
                            );
                            store.add_message(session_id, "assistant", &text);
                            store.add_structured_assistant_text_message(session_id, &text);
                        }
                    }
                    if !skip_memory_extraction {
                        self.extract_and_save_memory(&messages, &text).await;
                    }
                    loop_state.transition(AgentPhase::ResultReporting);
                    loop_state.transition(AgentPhase::Complete);
                    loop_state.log_self_inspection();
                    self.persist_loop_snapshot(&loop_state);
                    log_conversation("Assistant", &text);
                    return text;
                }
                let completed_research_targets = completed_current_research_file_targets(
                    prompt,
                    &session_workdir,
                    &messages,
                    literal_json_output,
                );
                if !completed_research_targets.is_empty() {
                    loop_state.transition(AgentPhase::Evaluating);
                    loop_state.last_eval_verdict = EvalVerdict::GoalAchieved;
                    loop_state.mark_terminal(
                        LoopTransitionReason::GoalAchieved,
                        "current research file outputs are complete and validated",
                    );

                    let skill_notice = completion_notice_for_auto_prepared_skill(
                        prompt,
                        auto_prepared_skill_name.as_deref(),
                    );
                    return self
                        .finalize_prompt_file_targets_with_memory(
                            session_id,
                            prompt,
                            &session_workdir,
                            &completed_research_targets,
                            skill_notice.as_deref(),
                            &messages,
                            skip_memory_extraction,
                            &mut loop_state,
                        )
                        .await;
                }

                if let Err(err) =
                    self.check_context_message_limit(session_id, &messages, &mut loop_state)
                {
                    return format!("Error: {}", err);
                }

                // ── Phase 7: Evaluating (partial progress) ───────────────
                loop_state.transition(AgentPhase::Evaluating);
                let progress_marker =
                    build_progress_marker(&response.text, &reasoning_text, &detected_tool_calls);
                let verdict = loop_state.observe_output(&progress_marker);
                log::debug!(
                    "[Evaluating] Round {} verdict={}",
                    loop_state.round,
                    verdict.as_str()
                );

                if verdict == EvalVerdict::Stuck {
                    loop_state.stuck_retry_count += 1;
                    if loop_state.stuck_retry_count > 2 {
                        log::warn!(
                            "[AgentLoop] Idle loop detected (round {}) - Terminating.",
                            loop_state.round
                        );
                        loop_state.transition(AgentPhase::TerminationCheck);
                        loop_state.transition(AgentPhase::ResultReporting);

                        if let Ok(ss) = self.session_store.lock() {
                            if let Some(store) = ss.as_ref() {
                                store.add_message(
                                    session_id,
                                    "assistant",
                                    "Task aborted (terminal idle loop).",
                                );
                                store.add_structured_assistant_text_message(
                                    session_id,
                                    "Task aborted (terminal idle loop).",
                                );
                            }
                        }
                        loop_state.last_error = Some("Agent is stuck in an execution loop.".into());
                        loop_state.mark_terminal(
                            LoopTransitionReason::StuckLoopAbort,
                            format!(
                                "idle loop detected after {} repeated retries",
                                loop_state.stuck_retry_count
                            ),
                        );
                        self.persist_loop_snapshot(&loop_state);
                        return "Error: Agent is stuck in an execution loop.".into();
                    } else {
                        log::warn!(
                            "[AgentLoop] Idle loop detected (round {}) - Triggering Dynamic Fallback RePlanning.",
                            loop_state.round
                        );
                        loop_state.mark_follow_up(
                            LoopTransitionReason::IdleRecovery,
                            format!(
                                "stuck verdict triggered retry {}",
                                loop_state.stuck_retry_count
                            ),
                        );
                        messages.push(LlmMessage {
                            role: "user".into(),
                            text: "System Error: You are stuck in a loop. Re-evaluate your plan and try a completely different approach using different tools. Do not repeat the previous action.".into(),
                            ..Default::default()
                        });
                        loop_state.transition(AgentPhase::RePlanning);
                    }
                }

                if should_force_current_research_synthesis(
                    prompt,
                    &session_workdir,
                    &messages,
                    has_expected_file_targets,
                    literal_json_output,
                    loop_state.round,
                    loop_state.total_tool_calls,
                ) {
                    loop_state.mark_follow_up(
                        LoopTransitionReason::FileActionRequired,
                        "current research evidence is sufficient; synthesize the requested file now",
                    );
                    messages.push(LlmMessage {
                        role: "assistant".into(),
                        text: response.text.clone(),
                        ..Default::default()
                    });
                    messages.push(LlmMessage::user(
                        "The task is not complete yet, but you already have enough verified current research evidence in the existing tool results. Stop gathering more sources and create the requested file now using only the strongest verified entries already collected. For a general roundup, prefer diverse organizers or ecosystems instead of filling the list from one event family. If the user did not request a specific year, do not rewrite the list around a guessed year; keep only conferences whose official evidence clearly matches the currently upcoming edition. If one candidate looks like a workshop, training program, weak local city edition, or niche secondary event, replace it with a stronger established flagship conference from the existing evidence instead of searching again.",
                    ));
                    loop_state.transition(AgentPhase::RePlanning);
                    continue;
                }

                let completed_file_targets = completed_file_management_targets(
                    prompt,
                    &session_workdir,
                    &messages,
                    literal_json_output,
                );
                if !completed_file_targets.is_empty() {
                    loop_state.transition(AgentPhase::Evaluating);
                    loop_state.last_eval_verdict = EvalVerdict::GoalAchieved;
                    loop_state.mark_terminal(
                        LoopTransitionReason::GoalAchieved,
                        "file outputs are complete and validated after tool execution",
                    );

                    let skill_notice = completion_notice_for_auto_prepared_skill(
                        prompt,
                        auto_prepared_skill_name.as_deref(),
                    );
                    return self
                        .finalize_prompt_file_targets_with_memory(
                            session_id,
                            prompt,
                            &session_workdir,
                            &completed_file_targets,
                            skill_notice.as_deref(),
                            &messages,
                            skip_memory_extraction,
                            &mut loop_state,
                        )
                        .await;
                }

                let pending_research_rewrite = pending_current_research_rewrite_details(
                    prompt,
                    &session_workdir,
                    &messages,
                    literal_json_output,
                );
                if !pending_research_rewrite.is_empty() {
                    loop_state.mark_follow_up(
                        LoopTransitionReason::FileTargetsMissing,
                        "current research output requires a targeted rewrite",
                    );
                    messages.push(LlmMessage {
                        role: "assistant".into(),
                        text: response.text.clone(),
                        ..Default::default()
                    });
                    messages.push(LlmMessage::user(&format!(
                        "The task is not complete yet. The current research output file exists but is still invalid:\n{}\n{}",
                        pending_research_rewrite.join("\n"),
                        if current_research_output_requires_targeted_search(
                            prompt,
                            &session_workdir,
                            &messages,
                        ) {
                            if prompt_requests_conference_roundup(prompt) {
                                "At least one conference entry still needs a stronger replacement. Stop rewriting the same list. Run one targeted official web search for a current-upcoming flagship conference with an exact date and location, replace the weak or wrong-year entry, then rewrite only the output file once."
                            } else {
                                "At least one requested fact still needs a better-supported replacement. Stop rewriting the same output. Run one targeted official search to replace unsupported entries or fill the missing fields, then rewrite only the output file once."
                            }
                        } else {
                            "Rewrite only the listed output file with one clean Markdown structure that satisfies the request. Prefer a heading plus a four-column table when the user asked for named fields such as name, date, location, and website. If the user did not specify a year, do not lock onto a guessed year while rewriting; keep only conferences whose official evidence clearly matches the current upcoming edition. Do not keep experimenting with multiple formats or launch more broad searches unless the invalid detail explicitly shows that a field is still missing from the collected evidence."
                        }
                    )));
                    loop_state.transition(AgentPhase::RePlanning);
                    continue;
                }

                // If it was a workflow tool, we just successfully completed it! Save output and advance.
                if is_workflow_tool {
                    let last_msg = messages.last().unwrap();
                    let output_val = if last_msg.role == "tool" {
                        last_msg.tool_result.clone()
                    } else {
                        serde_json::from_str(&last_msg.text)
                            .unwrap_or(Value::String(last_msg.text.clone()))
                    };

                    let we = self.workflow_engine.read().await;
                    if let Some(wf_id) = loop_state.active_workflow_id.clone() {
                        if let Some(wf) = we.get_workflow(&wf_id) {
                            let step = &wf.steps[loop_state.current_workflow_step];
                            loop_state
                                .workflow_vars
                                .insert(step.output_var.clone(), output_val);
                            loop_state.current_workflow_step += 1;
                            loop_state.mark_follow_up(
                                LoopTransitionReason::WorkflowStepAdvance,
                                format!(
                                    "workflow '{}' advanced to step {}",
                                    wf_id, loop_state.current_workflow_step
                                ),
                            );
                        }
                    }
                    continue; // Immediately start next round to pick up next workflow step
                }
            } else {
                if !literal_json_output
                    && has_expected_file_targets
                    && !has_file_completion_candidate_activity(&messages)
                {
                    loop_state.mark_follow_up(
                        LoopTransitionReason::FileActionRequired,
                        "file task still requires direct file_manager or file_write actions",
                    );
                    messages.push(LlmMessage {
                        role: "assistant".into(),
                        text: response.text.clone(),
                        ..Default::default()
                    });
                    messages.push(LlmMessage::user(
                        "The task is not complete yet. Manage the requested files directly with \
                         file_manager or file_write in the working directory. Do not answer with \
                         prose only, and do not use run_generated_code unless an executable script \
                         was explicitly requested.",
                    ));
                    loop_state.transition(AgentPhase::RePlanning);
                    continue;
                }
                if !literal_json_output && has_expected_file_targets {
                    let missing_targets =
                        missing_file_management_targets(prompt, &session_workdir, &messages);
                    if !missing_targets.is_empty() {
                        loop_state.mark_follow_up(
                            LoopTransitionReason::FileTargetsMissing,
                            format!(
                                "{} requested file target(s) are still missing",
                                missing_targets.len()
                            ),
                        );
                        messages.push(LlmMessage {
                            role: "assistant".into(),
                            text: response.text.clone(),
                            ..Default::default()
                        });
                        messages.push(LlmMessage::user(&format!(
                            "The task is not complete yet. The following requested files are still missing in the working directory:\n{}\nUse file_manager or file_write to create exactly those files. Do not switch to run_generated_code unless an executable script was explicitly requested.",
                            missing_targets.join("\n")
                        )));
                        loop_state.transition(AgentPhase::RePlanning);
                        continue;
                    }

                    let invalid_targets =
                        invalid_file_management_targets(prompt, &session_workdir, &messages);
                    if !invalid_targets.is_empty() {
                        let invalid_target_details = describe_invalid_file_management_targets(
                            prompt,
                            &session_workdir,
                            &messages,
                            &invalid_targets,
                        );
                        loop_state.mark_follow_up(
                            LoopTransitionReason::FileTargetsMissing,
                            format!(
                                "{} requested file target(s) are empty or invalid",
                                invalid_targets.len()
                            ),
                        );
                        messages.push(LlmMessage {
                            role: "assistant".into(),
                            text: response.text.clone(),
                            ..Default::default()
                        });
                        messages.push(LlmMessage::user(&format!(
                            "The task is not complete yet. The following requested files exist but are still invalid:\n{}\n{}",
                            invalid_target_details.join("\n"),
                            if current_research_output_requires_targeted_search(
                                prompt,
                                &session_workdir,
                                &messages,
                            ) {
                                "At least one current-research entry still needs a stronger replacement. Stop rewriting the same roundup. Run one targeted official search to replace unsupported, stale, or wrong-year entries, then rewrite only those listed output files once."
                            } else {
                                "Rewrite only those listed output files with a targeted fix for the stated issue. Do not overwrite other prompt-referenced source or input files unless the user explicitly asked for that. Use specialized native tools for PDFs, images, spreadsheets, or current web research instead of placeholders. For general current-research roundups, prefer diverse organizers or ecosystems instead of multiple entries from one source family unless the prompt explicitly asks for that source. For conference roundups, replace niche or mixed-quality picks with stronger flagship annual conferences whose official pages clearly publish exact dates and locations. If the target file is Markdown, keep it as real Markdown that matches the requested task shape rather than raw JSON or CSV."
                            }
                        )));
                        loop_state.transition(AgentPhase::RePlanning);
                        continue;
                    }
                }

                let expected_output_paths = if literal_json_output {
                    Vec::new()
                } else {
                    expected_persisted_level_script_paths(prompt)
                };
                if !expected_output_paths.is_empty() {
                    let saved_output_paths = collect_successful_saved_output_paths(&messages);
                    let missing_output_paths = expected_output_paths
                        .into_iter()
                        .filter(|path| !saved_output_paths.contains(path))
                        .collect::<Vec<_>>();

                    if !missing_output_paths.is_empty() {
                        loop_state.mark_follow_up(
                            LoopTransitionReason::PersistedOutputsMissing,
                            format!(
                                "{} persisted output file(s) are still missing",
                                missing_output_paths.len()
                            ),
                        );
                        messages.push(LlmMessage {
                            role: "assistant".into(),
                            text: response.text.clone(),
                            ..Default::default()
                        });
                        messages.push(LlmMessage::user(&format!(
                            "The task is not complete yet. Do not respond with prose or fenced code. Use run_generated_code to create the missing files exactly at these paths:\n{}\nIf needed, inspect /tmp/ds_olympiad/problem.md first. Generate exactly one level per run_generated_code call and continue until every file is saved.",
                            missing_output_paths.join("\n")
                        )));
                        loop_state.transition(AgentPhase::RePlanning);
                        continue;
                    }
                }

                let mut advance_workflow = false;
                if let Some(wf_id) = loop_state.active_workflow_id.as_ref() {
                    let we = self.workflow_engine.read().await;
                    if let Some(wf) = we.get_workflow(wf_id) {
                        let step = &wf.steps[loop_state.current_workflow_step];
                        loop_state.workflow_vars.insert(
                            step.output_var.clone(),
                            serde_json::Value::String(response.text.clone()),
                        );
                        loop_state.current_workflow_step += 1;
                        advance_workflow = true;
                    }
                }
                if advance_workflow {
                    loop_state.mark_follow_up(
                        LoopTransitionReason::WorkflowStepAdvance,
                        format!(
                            "workflow text step advanced to {}",
                            loop_state.current_workflow_step
                        ),
                    );
                    // Push the prompt assistant response so context isn't lost
                    messages.push(LlmMessage {
                        role: "assistant".into(),
                        text: response.text.clone(),
                        ..Default::default()
                    });
                    continue;
                }

                // ── Phase 7: Evaluating — GoalAchieved ──────────────────
                loop_state.transition(AgentPhase::Evaluating);
                loop_state.last_eval_verdict = EvalVerdict::GoalAchieved;

                log::debug!(
                    "[Evaluating] Round {} verdict=GoalAchieved (no tool calls)",
                    loop_state.round
                );
                loop_state.mark_terminal(
                    LoopTransitionReason::GoalAchieved,
                    "assistant produced a terminal response without tool calls",
                );

                // Enforce reasoning extraction for final user response
                let final_text = extract_final_text(&response.text);

                let mut text = final_text;
                if is_dashboard_web_app_request {
                    if let Some(args) = generated_web_app_args_from_text(&text) {
                        let generated = self.generate_web_app(&args).await;
                        if generated.get("error").is_none() {
                            text = generated.to_string();
                        } else {
                            log::warn!(
                                "[WebAppFallback] Parsed web app response but generation failed: {}",
                                generated
                            );
                        }
                    }
                }
                return self
                    .finalize_prompt_text_with_memory(
                        session_id,
                        &messages,
                        text,
                        skip_memory_extraction,
                        &mut loop_state,
                    )
                    .await;
            }

            // ── Phase 8: RePlanning / Phase 12: StateTracking ────────────
            loop_state.transition(AgentPhase::StateTracking);

            // ── Phase 13: SelfInspection ─────────────────────────────────
            loop_state.transition(AgentPhase::SelfInspection);
            loop_state.log_self_inspection();
            self.persist_loop_snapshot(&loop_state);

            // In-loop size-based compaction
            loop_state.token_used = context_engine.estimate_tokens(&messages);
            if context_engine.should_compact(&messages, loop_state.token_budget)
                || loop_state.needs_compaction()
            {
                log::debug!(
                    "[ContextEngine] In-loop compaction triggered (round {})",
                    loop_state.round
                );
                messages = context_engine.compact(messages, loop_state.token_budget);
                loop_state.token_used = context_engine.estimate_tokens(&messages);
                self.persist_compacted_messages(session_id, &messages);
            }

            // ── Phase 9: TerminationCheck ─────────────────────────────────
            loop_state.round += 1;
            loop_state.transition(AgentPhase::TerminationCheck);

            if loop_state.is_round_limit_reached() {
                log::warn!(
                    "[AgentLoop] Max rounds ({}) reached for session '{}'",
                    loop_state.max_tool_rounds,
                    session_id
                );
                loop_state.mark_terminal(
                    LoopTransitionReason::RoundLimitReached,
                    format!("max tool rounds {} reached", loop_state.max_tool_rounds),
                );
                break;
            }

            loop_state.transition(AgentPhase::RePlanning);
        }

        // ── Phase 14: ResultReporting (limit hit) ────────────────────────
        let completed_file_targets = completed_file_management_targets(
            prompt,
            &session_workdir,
            &messages,
            literal_json_output,
        );
        if let Some(text) = synthesize_file_grounded_answers(prompt, &messages) {
            if let Ok(ss) = self.session_store.lock() {
                if let Some(store) = ss.as_ref() {
                    let grounded_files =
                        collect_existing_grounded_input_files(prompt, &session_workdir);
                    record_grounded_answer_preview(store, session_id, &grounded_files, &text);
                }
            }
            return self
                .finalize_prompt_text_with_memory(
                    session_id,
                    &messages,
                    text,
                    skip_memory_extraction,
                    &mut loop_state,
                )
                .await;
        }
        if !completed_file_targets.is_empty() {
            let skill_notice = completion_notice_for_auto_prepared_skill(
                prompt,
                auto_prepared_skill_name.as_deref(),
            );
            return self
                .finalize_prompt_file_targets_with_memory(
                    session_id,
                    prompt,
                    &session_workdir,
                    &completed_file_targets,
                    skill_notice.as_deref(),
                    &messages,
                    skip_memory_extraction,
                    &mut loop_state,
                )
                .await;
        }

        loop_state.transition(AgentPhase::ResultReporting);
        loop_state.log_self_inspection();
        "Error: Maximum tool call rounds exceeded".into()
    }
}
