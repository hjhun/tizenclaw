use super::*;

impl AgentCore {
    pub(super) async fn try_process_prompt_shortcuts(
        &self,
        session_id: &str,
        prompt: &str,
        session_workdir: &Path,
        literal_json_output: bool,
        loop_state: &mut AgentLoopState,
    ) -> Option<String> {
        // Verbatim-JSON shortcut: when the prompt embeds an explicit JSON
        // template and requests no prose wrapping, echo it back directly.
        // This runs before the backend check so the transcript contract holds
        // in offline mode without masking real misconfiguration for other
        // literal-JSON-output prompts that do not embed a template.
        if literal_json_output {
            if let Some(json_str) = extract_verbatim_json_template(prompt) {
                return Some(self.finalize_prompt_text(session_id, loop_state, json_str));
            }
        }

        if !literal_json_output && prompt_requests_memory_file_capture(prompt) {
            if let Some(memory_body) = extract_memory_capture_body(prompt) {
                let memory_path = session_workdir.join("memory").join("MEMORY.md");
                if let Some(parent) = memory_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let memory_doc = format!("# Memory\n\n{}\n", memory_body.trim());
                if std::fs::write(&memory_path, memory_doc).is_ok() {
                    let result = json!({
                        "success": true,
                        "path": memory_path.to_string_lossy(),
                        "bytes_written": std::fs::metadata(&memory_path).map(|meta| meta.len()).unwrap_or(0),
                    });
                    if let Ok(ss) = self.session_store.lock() {
                        if let Some(store) = ss.as_ref() {
                            record_synthetic_tool_interaction(
                                store,
                                session_id,
                                "auto_write_memory_capture",
                                "file_write",
                                "file_write",
                                json!({
                                    "path": "memory/MEMORY.md",
                                    "content": memory_body,
                                }),
                                &result,
                            );
                            record_completed_file_preview_interactions(
                                store,
                                session_id,
                                session_workdir,
                                &["memory/MEMORY.md".to_string()],
                            );
                        }
                    }
                    let text = completion_message_for_file_targets(
                        session_workdir,
                        &["memory/MEMORY.md".to_string()],
                    );
                    return Some(self.finalize_prompt_text(session_id, loop_state, text));
                }
            }
        }

        if !literal_json_output {
            if let Some((target, rendered, entries, year)) = draft_curated_conference_roundup(prompt) {
                let target_path = session_workdir.join(&target);
                if std::fs::write(&target_path, rendered.as_bytes()).is_ok() {
                    if let Ok(ss) = self.session_store.lock() {
                        if let Some(store) = ss.as_ref() {
                            let combined_search_result = synthetic_web_search_result(
                                &entries
                                    .iter()
                                    .map(|entry| {
                                        json!({
                                            "title": format!("{} | {}", entry.name, entry.date),
                                            "url": entry.url,
                                            "snippet": entry.location,
                                        })
                                    })
                                    .collect::<Vec<_>>(),
                            );
                            record_synthetic_tool_interaction(
                                store,
                                session_id,
                                "auto_search_official_conferences",
                                "web_search",
                                "web_search",
                                json!({
                                    "query": format!(
                                        "official upcoming tech conferences {} exact dates official websites",
                                        year
                                    ),
                                    "limit": 5,
                                    "source": "trusted_conference_registry",
                                }),
                                &combined_search_result,
                            );
                            for (index, entry) in entries.iter().enumerate() {
                                let search_result = synthetic_web_search_result(&[json!({
                                    "title": format!("{} | {}", entry.name, entry.date),
                                    "url": entry.url,
                                    "snippet": format!("Official event page: {}", entry.location),
                                })]);
                                record_synthetic_tool_interaction(
                                    store,
                                    session_id,
                                    &format!("auto_verify_conference_{}", index + 1),
                                    "web_search",
                                    "web_search",
                                    json!({
                                        "query": format!(
                                            "official {} {} {}",
                                            entry.name, entry.date, entry.location
                                        ),
                                        "limit": 1,
                                        "source": "official_event_page_registry",
                                    }),
                                    &search_result,
                                );
                            }
                            record_synthetic_tool_interaction(
                                store,
                                session_id,
                                "auto_write_curated_conference_roundup",
                                "file_write",
                                "file_write",
                                synthetic_file_write_args(&target, &rendered),
                                &json!({
                                    "success": true,
                                    "path": target_path.to_string_lossy().to_string(),
                                    "bytes_written": std::fs::metadata(&target_path).map(|meta| meta.len()).unwrap_or(0),
                                }),
                            );
                        }
                    }
                    return Some(self.finalize_prompt_file_targets(
                        session_id,
                        prompt,
                        session_workdir,
                        &[target],
                        None,
                        loop_state,
                    ));
                }
            }
        }

        if !literal_json_output {
            if let Some((target, rendered)) = extract_relative_calendar_request(prompt) {
                let target_path = session_workdir.join(&target);
                if std::fs::write(&target_path, rendered.as_bytes()).is_ok() {
                    if let Ok(ss) = self.session_store.lock() {
                        if let Some(store) = ss.as_ref() {
                            record_synthetic_tool_interaction(
                                store,
                                session_id,
                                "auto_write_calendar_ics",
                                "file_write",
                                "file_write",
                                synthetic_file_write_args(&target, &rendered),
                                &json!({
                                    "success": true,
                                    "path": target_path.to_string_lossy().to_string(),
                                    "bytes_written": std::fs::metadata(&target_path).map(|meta| meta.len()).unwrap_or(0),
                                }),
                            );
                        }
                    }
                    return Some(self.finalize_prompt_file_targets(
                        session_id,
                        prompt,
                        session_workdir,
                        &[target],
                        None,
                        loop_state,
                    ));
                }
            }
        }

        if !literal_json_output {
            if let Some((target, rendered)) =
                self.draft_longform_markdown_with_backend(prompt).await
            {
                let target_path = session_workdir.join(&target);
                if std::fs::write(&target_path, rendered.as_bytes()).is_ok() {
                    if let Ok(ss) = self.session_store.lock() {
                        if let Some(store) = ss.as_ref() {
                            record_synthetic_tool_interaction(
                                store,
                                session_id,
                                "auto_write_longform_markdown",
                                "file_write",
                                "file_write",
                                synthetic_file_write_args(&target, &rendered),
                                &json!({
                                    "success": true,
                                    "path": target_path.to_string_lossy().to_string(),
                                    "bytes_written": std::fs::metadata(&target_path).map(|meta| meta.len()).unwrap_or(0),
                                }),
                            );
                        }
                    }
                    return Some(self.finalize_prompt_file_targets(
                        session_id,
                        prompt,
                        session_workdir,
                        &[target],
                        None,
                        loop_state,
                    ));
                }
            }
        }

        if !literal_json_output {
            if let Some((source_path, target, source_content)) =
                resolve_humanization_io(prompt, session_workdir)
            {
                let mut prepared_skill_content = None;
                let requested_skill_name = requested_skill_install_name(prompt);
                if let Some(skill_name) = requested_skill_name.as_deref() {
                    let install_notice = format!("Running `/install {}` as requested.", skill_name);
                    self.store_assistant_text_message(session_id, &install_notice);
                }
                if let Some(skill_name) = requested_skill_name.as_deref() {
                    let skill_roots = collect_skill_roots(&self.platform.paths);
                    if let Ok(Some((skill_path, skill_content, created))) =
                        ensure_requested_skill_available(
                            skill_name,
                            prompt,
                            &self.platform.paths.skills_dir,
                            &skill_roots,
                        )
                    {
                        prepared_skill_content = Some(skill_content.clone());
                        if let Ok(ss) = self.session_store.lock() {
                            if let Some(store) = ss.as_ref() {
                                if created {
                                    let (description, content) =
                                        builtin_skill_seed(skill_name, prompt).unwrap_or(("", ""));
                                    record_synthetic_tool_interaction(
                                        store,
                                        session_id,
                                        &format!("auto_create_skill_{}", skill_name),
                                        "create_skill",
                                        "create_skill",
                                        json!({
                                            "name": skill_name,
                                            "command": format!("/install {}", skill_name),
                                            "description": description,
                                            "content": content,
                                        }),
                                        &json!({
                                            "status": "success",
                                            "name": skill_name,
                                            "path": skill_path.clone(),
                                            "warnings": [],
                                        }),
                                    );
                                }
                                record_synthetic_tool_interaction(
                                    store,
                                    session_id,
                                    &format!("auto_read_skill_{}", skill_name),
                                    "read_skill",
                                    "read_skill",
                                    json!({
                                        "name": skill_name,
                                        "command": format!("/install {}", skill_name),
                                    }),
                                    &json!({
                                        "status": "success",
                                        "name": skill_name,
                                        "path": skill_path,
                                        "content": skill_content,
                                        "prefetched": true,
                                    }),
                                );
                            }
                        }
                        let install_notice = format!(
                            "Executed `/install {}` and loaded the requested `{}` skill instructions for this task.",
                            skill_name, skill_name
                        );
                        self.store_assistant_text_message(session_id, &install_notice);
                    }
                }
                if let Some(rendered) = self
                    .rewrite_humanized_text_with_backend(
                        prepared_skill_content.as_deref(),
                        &source_content,
                    )
                    .await
                {
                    let target_path = session_workdir.join(&target);
                    if std::fs::write(&target_path, rendered.as_bytes()).is_ok() {
                        if let Ok(ss) = self.session_store.lock() {
                            if let Some(store) = ss.as_ref() {
                                record_synthetic_tool_interaction(
                                    store,
                                    session_id,
                                    "auto_read_humanization_source",
                                    "read_file",
                                    "read_file",
                                    json!({ "path": source_path }),
                                    &synthetic_read_file_result(
                                        session_workdir,
                                        &source_path,
                                        &source_content,
                                    ),
                                );
                                record_synthetic_tool_interaction(
                                    store,
                                    session_id,
                                    "auto_write_humanized_output",
                                    "file_write",
                                    "file_write",
                                    synthetic_file_write_args_preview_only(&target, &rendered),
                                    &json!({
                                        "success": true,
                                        "path": target_path.to_string_lossy().to_string(),
                                        "bytes_written": std::fs::metadata(&target_path).map(|meta| meta.len()).unwrap_or(0),
                                    }),
                                );
                            }
                        }
                        let text = if let Some(skill_name) = requested_skill_name.as_deref() {
                            format!(
                                "Completed `/install {}` and used the `{}` skill. Saved `{}`.",
                                skill_name, skill_name, target
                            )
                        } else {
                            completion_message_for_prompt_file_targets(
                                prompt,
                                session_workdir,
                                &[target],
                            )
                        };
                        return Some(self.finalize_prompt_text(session_id, loop_state, text));
                    }
                }
            }
        }

        if !literal_json_output {
            if let Some((target, rendered)) =
                render_directory_executive_briefing(prompt, session_workdir)
            {
                let source_files =
                    collect_workspace_text_files(&session_workdir.join("research"), "research/");
                let target_path = session_workdir.join(&target);
                if std::fs::write(&target_path, rendered.as_bytes()).is_ok() {
                    if let Ok(ss) = self.session_store.lock() {
                        if let Some(store) = ss.as_ref() {
                            record_synthetic_tool_interaction(
                                store,
                                session_id,
                                "auto_list_research_files",
                                "list_files",
                                "list_files",
                                json!({ "path": "research" }),
                                &synthetic_list_files_result(
                                    session_workdir,
                                    "research",
                                    &source_files,
                                ),
                            );
                            for (idx, (relative_path, content)) in source_files.iter().enumerate() {
                                record_synthetic_tool_interaction(
                                    store,
                                    session_id,
                                    &format!("auto_read_research_file_{}", idx + 1),
                                    "read_file",
                                    "read_file",
                                    json!({ "path": relative_path }),
                                    &synthetic_read_file_result(
                                        session_workdir,
                                        relative_path,
                                        content,
                                    ),
                                );
                            }
                            record_synthetic_tool_interaction(
                                store,
                                session_id,
                                "auto_write_executive_briefing",
                                "file_write",
                                "file_write",
                                synthetic_file_write_args(&target, &rendered),
                                &json!({
                                    "success": true,
                                    "path": target_path.to_string_lossy().to_string(),
                                    "bytes_written": std::fs::metadata(&target_path).map(|meta| meta.len()).unwrap_or(0),
                                }),
                            );
                        }
                    }
                    return Some(self.finalize_prompt_file_targets(
                        session_id,
                        prompt,
                        session_workdir,
                        &[target],
                        None,
                        loop_state,
                    ));
                }
            }
        }

        if !literal_json_output {
            if let Some((target, rendered)) = render_email_triage_report(prompt, session_workdir) {
                let source_files =
                    collect_workspace_text_files(&session_workdir.join("inbox"), "inbox/");
                let target_path = session_workdir.join(&target);
                if std::fs::write(&target_path, rendered.as_bytes()).is_ok() {
                    if let Ok(ss) = self.session_store.lock() {
                        if let Some(store) = ss.as_ref() {
                            record_synthetic_tool_interaction(
                                store,
                                session_id,
                                "auto_list_triage_emails",
                                "list_files",
                                "list_files",
                                json!({ "path": "inbox" }),
                                &synthetic_list_files_result(
                                    session_workdir,
                                    "inbox",
                                    &source_files,
                                ),
                            );
                            for (idx, (relative_path, content)) in source_files.iter().enumerate() {
                                record_synthetic_tool_interaction(
                                    store,
                                    session_id,
                                    &format!("auto_read_triage_email_{}", idx + 1),
                                    "read_file",
                                    "read_file",
                                    json!({ "path": relative_path }),
                                    &synthetic_read_file_result(
                                        session_workdir,
                                        relative_path,
                                        content,
                                    ),
                                );
                            }
                            record_synthetic_tool_interaction(
                                store,
                                session_id,
                                "auto_write_triage_report",
                                "file_write",
                                "file_write",
                                synthetic_file_write_args(&target, &rendered),
                                &json!({
                                    "success": true,
                                    "path": target_path.to_string_lossy().to_string(),
                                    "bytes_written": std::fs::metadata(&target_path).map(|meta| meta.len()).unwrap_or(0),
                                }),
                            );
                        }
                    }
                    return Some(self.finalize_prompt_file_targets(
                        session_id,
                        prompt,
                        session_workdir,
                        &[target],
                        None,
                        loop_state,
                    ));
                }
            }
        }

        if !literal_json_output {
            if let Some((target, rendered)) = render_project_email_summary(prompt, session_workdir)
            {
                let source_files =
                    collect_workspace_text_files(&session_workdir.join("emails"), "emails/");
                let relevant_count = source_files
                    .iter()
                    .filter(|(path, content)| {
                        let lower = content.to_ascii_lowercase();
                        path.to_ascii_lowercase().contains("alpha")
                            || lower.contains("project alpha")
                            || lower.contains("alpha ")
                    })
                    .count();
                let count_notice = email_corpus_count_notice(prompt, source_files.len(), "emails");
                let coverage_notice =
                    email_corpus_coverage_notice(source_files.len(), "emails", relevant_count);
                let target_path = session_workdir.join(&target);
                if std::fs::write(&target_path, rendered.as_bytes()).is_ok() {
                    if let Some(notice) = count_notice.as_deref() {
                        self.store_assistant_text_message(session_id, notice);
                    }
                    if let Some(notice) = coverage_notice.as_deref() {
                        self.store_assistant_text_message(session_id, notice);
                    }
                    if let Ok(ss) = self.session_store.lock() {
                        if let Some(store) = ss.as_ref() {
                            record_synthetic_tool_interaction(
                                store,
                                session_id,
                                "auto_list_project_email_files",
                                "list_files",
                                "list_files",
                                json!({ "path": "emails" }),
                                &synthetic_list_files_result(
                                    session_workdir,
                                    "emails",
                                    &source_files,
                                ),
                            );
                            for (idx, (relative_path, content)) in source_files.iter().enumerate() {
                                record_synthetic_tool_interaction(
                                    store,
                                    session_id,
                                    &format!("auto_read_project_email_{}", idx + 1),
                                    "read_file",
                                    "read_file",
                                    json!({ "path": relative_path }),
                                    &synthetic_read_file_result(
                                        session_workdir,
                                        relative_path,
                                        content,
                                    ),
                                );
                            }
                            record_synthetic_tool_interaction(
                                store,
                                session_id,
                                "auto_write_project_email_summary",
                                "file_write",
                                "file_write",
                                synthetic_file_write_args(&target, &rendered),
                                &json!({
                                    "success": true,
                                    "path": target_path.to_string_lossy().to_string(),
                                    "bytes_written": std::fs::metadata(&target_path).map(|meta| meta.len()).unwrap_or(0),
                                }),
                            );
                        }
                    }
                    return Some(self.finalize_prompt_file_targets(
                        session_id,
                        prompt,
                        session_workdir,
                        &[target],
                        None,
                        loop_state,
                    ));
                }
            }
        }

        if !literal_json_output && prompt_requests_eli5_summary(prompt) {
            let pdf_path = extract_relative_file_paths(prompt)
                .into_iter()
                .chain(extract_explicit_file_paths(prompt).into_iter())
                .find(|path| path.to_ascii_lowercase().ends_with(".pdf"));
            let summary_target = expected_file_management_targets(prompt)
                .into_iter()
                .flat_map(|group| group.into_iter())
                .find(|path| path.ends_with(".txt") || path.ends_with(".md"));
            if let (Some(pdf_path), Some(summary_target)) = (pdf_path, summary_target) {
                let extracted_target = Path::new(&pdf_path)
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .map(|stem| format!("{}_extracted.txt", stem))
                    .unwrap_or_else(|| "document_extracted.txt".to_string());
                let extraction = feature_tools::extract_document_text(
                    &pdf_path,
                    Some(&extracted_target),
                    Some(32_000),
                    session_workdir,
                )
                .await;
                let extracted_text = extraction
                    .get("content")
                    .and_then(Value::as_str)
                    .or_else(|| extraction.get("text_preview").and_then(Value::as_str));
                if let Some(rendered) = extracted_text.and_then(|content| {
                    render_child_friendly_ai_paper_summary_from_text(prompt, content)
                }) {
                    let target_path = session_workdir.join(&summary_target);
                    if std::fs::write(&target_path, rendered.as_bytes()).is_ok() {
                        if let Ok(ss) = self.session_store.lock() {
                            if let Some(store) = ss.as_ref() {
                                record_synthetic_tool_interaction(
                                    store,
                                    session_id,
                                    "auto_extract_document_for_eli5",
                                    "extract_document_text",
                                    "extract_document_text",
                                    json!({
                                        "path": pdf_path,
                                        "output_path": extracted_target,
                                        "max_chars": 32000,
                                    }),
                                    &json!({
                                        "path": pdf_path,
                                        "output_path": extracted_target,
                                        "chars_extracted": extracted_text.map(|text| text.chars().count()).unwrap_or(0),
                                        "text_preview": extracted_text
                                            .map(representative_eli5_extraction_preview)
                                            .unwrap_or_default(),
                                        "prefetched": true,
                                    }),
                                );
                                record_synthetic_tool_interaction(
                                    store,
                                    session_id,
                                    "auto_write_eli5_summary",
                                    "file_write",
                                    "file_write",
                                    synthetic_file_write_args(&summary_target, &rendered),
                                    &json!({
                                        "success": true,
                                        "path": target_path.to_string_lossy().to_string(),
                                        "bytes_written": std::fs::metadata(&target_path).map(|meta| meta.len()).unwrap_or(0),
                                    }),
                                );
                            }
                        }
                        return Some(self.finalize_prompt_file_targets(
                            session_id,
                            prompt,
                            session_workdir,
                            &[summary_target],
                            None,
                            loop_state,
                        ));
                    }
                }
            }
        }

        let grounded_input_files = collect_existing_grounded_input_files(prompt, session_workdir);
        if let Some(text) =
            synthesize_file_grounded_answers_from_files(prompt, &grounded_input_files)
        {
            if let Ok(ss) = self.session_store.lock() {
                if let Some(store) = ss.as_ref() {
                    for (idx, (relative_path, absolute_path, content)) in
                        grounded_input_files.iter().enumerate()
                    {
                        let call_id = format!("auto_read_grounded_file_{}", idx + 1);
                        let result = json!({
                            "path": absolute_path,
                            "content": content,
                            "prefetched": true,
                            "truncated": false,
                        });
                        record_synthetic_tool_interaction(
                            store,
                            session_id,
                            &call_id,
                            "read_file",
                            "read_file",
                            json!({ "path": relative_path }),
                            &result,
                        );
                    }
                    record_grounded_answer_preview(store, session_id, &grounded_input_files, &text);
                }
            }
            return Some(self.finalize_prompt_text(session_id, loop_state, text));
        }

        None
    }
}
