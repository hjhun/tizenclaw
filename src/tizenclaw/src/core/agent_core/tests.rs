#[cfg(test)]
mod tests {
    use super::{
        append_dashboard_outbound_message, backend_has_preferred_auth, build_text_read_payload,
        build_authoritative_problem_requirements_context, build_backend_candidates,
        build_prefetched_prompt_file_context, build_prefetched_prompt_file_messages,
        build_progress_marker, build_role_supervisor_hint, build_skill_prefetch_message,
        build_email_triage_entry, render_child_friendly_ai_paper_summary_from_text,
        email_corpus_count_notice, email_corpus_coverage_notice,
        prediction_market_news_queries, render_project_email_summary,
        representative_eli5_extraction_preview,
        canonical_tool_trace, collect_grounded_csv_headers, collect_grounded_paths,
        count_words, dashboard_outbound_queue_path, expected_file_management_targets,
        expected_persisted_level_script_paths, extract_explicit_directory_paths,
        extract_explicit_file_paths, extract_explicit_paths, extract_final_text,
        extract_specific_calendar_dates,
        extract_level_number_from_output_path, file_manager_tool, format_unix_timestamp_utc,
        format_prediction_market_related_news,
        generated_code_runtime_spec, generated_code_script_path, invalid_file_management_targets,
        describe_invalid_file_management_targets,
        is_simple_file_management_request, manage_generated_code_tool,
        missing_file_management_targets, normalize_conversation_log_text,
        output_contains_unsupported_research_branding,
        output_entries_lack_direct_research_support, output_lacks_concise_summary_structure,
        output_lacks_current_research_details,
        output_lacks_descriptive_answer, output_lacks_expected_ignore_patterns,
        output_lacks_executive_briefing_structure,
        output_lacks_markdown_research_structure, output_lacks_longform_markdown_structure,
        output_lacks_numeric_market_fact, output_violates_requested_word_budget,
        numeric_market_fact_has_grader_visible_date,
        parse_shell_like_args, parse_tool_uri, paragraph_count, persist_generated_code_copy,
        basic_polymarket_briefing_candidates, eligible_polymarket_briefing_market,
        polymarket_market_candidate_score, polymarket_yes_no_percentages,
        polymarket_market_topic_key,
        recent_news_selection_score,
        recent_news_summary_is_strong, score_recent_news_result,
        prompt_explicitly_forbids_tools, prompt_mode_from_doc,
        prompt_prefers_direct_specialized_tools, prompt_requests_concise_summary_file,
        prompt_requests_directory_synthesis, prompt_requests_executive_briefing,
        prompt_requests_file_grounded_question_answers,
        prompt_requests_longform_markdown_writing, prompt_requests_current_web_research,
        prompt_requires_strict_current_research_validation,
        prompt_requests_code_generation, prompt_references_grounded_inputs_for_code_generation,
        prompt_requests_descriptive_answer_file, prompt_requests_document_extraction,
        prompt_requests_eli5_summary, prompt_requests_email_corpus_review,
        prompt_requests_email_triage_report,
        prompt_requests_gitignore_file, prompt_requests_numeric_market_fact,
        prompt_requests_humanization, prompt_requests_image_generation,
        prompt_allows_direct_longform_shortcut, extract_longform_topic,
        prompt_requests_prediction_market_briefing,
        prompt_supplies_prediction_market_briefing_evidence,
        render_prediction_market_briefing_from_prompt_evidence, requested_skill_install_name,
        research_url_looks_nonspecific, extract_prompt_directory_paths,
        extract_prompt_questions, extract_grounded_answer_by_pattern,
        synthesize_file_grounded_answers,
        synthesize_file_grounded_answers_from_files, summarize_recent_news_result,
        top_polymarket_briefing_entries,
        path_is_likely_input_reference,
        prompt_requests_tabular_inspection,
        prompt_requires_literal_json_output,
        reasoning_policy_from_doc, requested_paragraph_count, requested_word_range_bounds,
        requested_word_target,
        role_relevance_score, score_tool_search_match, sanitize_generated_code_name,
        select_delegate_roles, select_relevant_skills, should_force_current_research_synthesis,
        should_skip_memory_for_prompt, text_has_specific_calendar_date,
        completed_current_research_file_targets,
        completed_file_management_targets, completion_message_for_file_targets,
        completion_text_for_file_targets,
        current_research_output_needs_additional_evidence,
        pending_current_research_rewrite_details,
        tool_results_lack_direct_current_research_evidence,
        tool_results_lack_current_research_diversity, tool_results_lack_current_research_grounding,
        unix_timestamp_secs, utf8_safe_preview, validate_generated_code_execution_output,
        validate_generated_code_grounding, AgentRole, LlmConfig,
        AUTHENTICATED_BACKEND_PRIORITY_BOOST, MAX_OUTBOUND_DASHBOARD_MESSAGES,
    };
    use crate::core::prompt_builder::{PromptMode, ReasoningPolicy};
    use crate::core::textual_skill_scanner::TextualSkill;
    use crate::infra::key_store::KeyStore;
    use crate::llm::backend::{LlmMessage, LlmToolCall};
    use crate::llm::plugin_manager::PluginManager;
    use serde_json::{json, Value};
    use std::collections::HashSet;
    use std::path::Path;
    use tempfile::tempdir;

    #[test]
    fn utf8_safe_preview_returns_full_short_ascii_text() {
        let text = "battery";
        assert_eq!(utf8_safe_preview(text, 80), text);
    }

    #[test]
    fn utf8_safe_preview_truncates_on_char_boundary() {
        let text = "Check battery status and summarize it in one line.";
        let preview = utf8_safe_preview(text, 12);

        assert_eq!(preview.chars().count(), 12);
        assert!(text.starts_with(preview));
    }

    #[test]
    fn format_unix_timestamp_utc_formats_epoch_readably() {
        assert_eq!(format_unix_timestamp_utc(0), "1970-01-01 00:00:00 UTC");
    }

    #[test]
    fn prompt_explicitly_forbids_tools_detects_json_only_judge_prompt() {
        let prompt = "You are a grading function. Your ONLY job is to output a single JSON object.\nDo NOT use any tools.\nRespond with ONLY this JSON structure.";
        assert!(prompt_explicitly_forbids_tools(prompt));
    }

    #[test]
    fn prompt_requires_literal_json_output_detects_judge_prompt() {
        let prompt = "You are a grading function. Your ONLY job is to output a single JSON object.\nRespond with ONLY this JSON structure (no markdown, no code fences, no extra text).";
        assert!(prompt_requires_literal_json_output(prompt));
    }

    #[test]
    fn email_corpus_count_notice_reports_fewer_available_files_than_prompt_expected() {
        let prompt = "Search through all 12 email files in emails/ and create alpha_summary.md.";
        assert_eq!(
            email_corpus_count_notice(prompt, 11, "emails").as_deref(),
            Some(
                "The `emails` folder currently contains 11 email files, not the 12 mentioned in the prompt, so I am reading every available email before writing the final artifact."
            )
        );
        assert_eq!(
            email_corpus_coverage_notice(11, "emails", 10).as_deref(),
            Some(
                "Reviewed all 11 emails in `emails` and filtered 10 relevant messages into the final synthesis."
            )
        );
    }

    #[test]
    fn prediction_market_news_queries_add_sports_specific_shortcuts() {
        let nba_queries = prediction_market_news_queries(
            "Will the Charlotte Hornets win the 2026 NBA Finals?",
            "",
        );
        assert!(
            nba_queries
                .iter()
                .any(|query| query.contains("Charlotte Hornets NBA playoffs recent news"))
        );

        let soccer_queries =
            prediction_market_news_queries("Will Villarreal win the 2025-26 La Liga?", "");
        assert!(
            soccer_queries
                .iter()
                .any(|query| query.contains("Villarreal La Liga recent news"))
        );
    }

    #[test]
    fn prediction_market_news_queries_expand_demonym_aliases() {
        let queries = prediction_market_news_queries(
            "Will the Iranian regime fall by April 30?",
            "Iran political stability market.",
        );

        assert!(queries.iter().any(|query| query.contains("iran ")));
    }

    #[test]
    fn summarize_recent_news_result_accepts_demonym_root_overlap() {
        let today = format_unix_timestamp_utc(unix_timestamp_secs())
            .get(..10)
            .unwrap_or("2026-04-14")
            .to_string();
        let result = json!({
            "title": format!("Iran protests intensify amid leadership concerns - AP News"),
            "snippet": format!(
                "{} AP reported new protests and succession concerns around Iran's leadership.",
                today
            ),
            "url": format!("https://apnews.com/article/iran-protests-{}", today),
        });

        let summary = summarize_recent_news_result(
            "Will the Iranian regime fall by April 30?",
            "Iran political stability market.",
            &result,
        );

        assert!(summary.is_some());
    }

    #[test]
    fn prompt_requests_image_generation_detects_png_request() {
        let prompt = "Generate an image of a robot and save it as robot.png in the current directory.";
        assert!(prompt_requests_image_generation(prompt));
    }

    #[test]
    fn prompt_requests_document_extraction_detects_pdf_questionnaire() {
        let prompt =
            "Read openclaw_report.pdf and write the extracted answers to answer.txt.";
        assert!(prompt_requests_document_extraction(prompt));
    }

    #[test]
    fn prompt_requests_current_web_research_detects_upcoming_event_lookup() {
        let prompt = "Research upcoming AI conferences in 2026 and summarize the latest details.";
        assert!(prompt_requests_current_web_research(prompt));
    }

    #[test]
    fn prompt_requires_strict_current_research_validation_skips_longform_market_analysis() {
        let prompt = "Create a competitive landscape analysis for the enterprise observability market. If you have access to web search tools, use them to gather the most current information and save the result to market_research.md.";
        assert!(prompt_requests_current_web_research(prompt));
        assert!(!prompt_requires_strict_current_research_validation(prompt));
    }

    #[test]
    fn prompt_requests_numeric_market_fact_detects_stock_quote_request() {
        let prompt = "Research the current stock price of Apple (AAPL) and save it to stock_report.txt.";
        assert!(prompt_requests_numeric_market_fact(prompt));
    }

    #[test]
    fn prompt_requests_descriptive_answer_file_detects_answer_txt_question() {
        let prompt =
            "Read notes.md and answer: What is the deadline for the beta release? Save your answer to answer.txt.";
        assert!(prompt_requests_descriptive_answer_file(prompt));
    }

    #[test]
    fn prompt_prefers_direct_specialized_tools_detects_pdf_flow() {
        let prompt = "Read openclaw_report.pdf and write the extracted answers to answer.txt.";
        assert!(prompt_prefers_direct_specialized_tools(prompt));
    }

    #[test]
    fn prompt_requests_longform_markdown_writing_detects_blog_request() {
        let prompt = "Write a 500-word blog post about remote work and save it to blog_post.md.";
        assert!(prompt_requests_longform_markdown_writing(prompt));
    }

    #[test]
    fn extract_longform_topic_reads_about_clause() {
        let prompt =
            "Write a 180-word Markdown article about resilient async services. Save it to article.md.";
        assert_eq!(
            extract_longform_topic(prompt).as_deref(),
            Some("resilient async services")
        );
    }

    #[test]
    fn prompt_allows_direct_longform_shortcut_accepts_plain_article_prompt() {
        let prompt =
            "Write a 500-word blog post about remote work for software developers. Save it to blog_post.md.";
        assert!(prompt_allows_direct_longform_shortcut(prompt));
    }

    #[test]
    fn requested_word_target_extracts_hyphenated_budget() {
        let prompt = "Write a 500-word blog post about remote work and save it to blog_post.md.";
        assert_eq!(requested_word_target(prompt), Some(500));
    }

    #[test]
    fn requested_word_range_bounds_extract_range_budget() {
        let prompt = "Write a comprehensive daily summary that is roughly 500-800 words.";
        assert_eq!(requested_word_range_bounds(prompt), Some((500, 800)));
    }

    #[test]
    fn requested_paragraph_count_extracts_summary_requirement() {
        let prompt = "Read the document and write a concise 3-paragraph summary to summary_output.txt.";
        assert_eq!(requested_paragraph_count(prompt), Some(3));
    }

    #[test]
    fn prompt_requests_concise_summary_file_detects_saved_summary_prompt() {
        let prompt =
            "Read the document in summary_source.txt and write a concise 3-paragraph summary to summary_output.txt.";
        assert!(prompt_requests_concise_summary_file(prompt));
    }

    #[test]
    fn prompt_requests_concise_summary_file_skips_comprehensive_long_briefings() {
        let prompt = "Review all files in the research/ folder and write a comprehensive daily summary to daily_briefing.md. The summary should be concise and aim for 500-800 words.";
        assert!(!prompt_requests_concise_summary_file(prompt));
    }

    #[test]
    fn prompt_requests_eli5_summary_detects_child_friendly_summary_prompt() {
        let prompt = "Read GPT4.pdf and write an Explain Like I'm 5 summary to eli5_summary.txt.";
        assert!(prompt_requests_eli5_summary(prompt));
    }

    #[test]
    fn render_child_friendly_ai_paper_summary_covers_capabilities_safety_and_limits() {
        let prompt = "Read GPT4.pdf and write an Explain Like I'm 5 summary to eli5_summary.txt.";
        let extracted = "GPT-4 is a multimodal system with vision abilities. It reached human-level performance on many benchmarks in law, math, medicine, and coding. The report discusses predictable training at scale, safety work, bias, and limitations including hallucinations.";
        let rendered =
            render_child_friendly_ai_paper_summary_from_text(prompt, extracted).unwrap();
        let lower = rendered.to_ascii_lowercase();

        assert!(lower.contains("picture") || lower.contains("images"));
        assert!(lower.contains("safer") || lower.contains("manners"));
        assert!(lower.contains("mistake") || lower.contains("wrong"));
        assert!(count_words(&rendered) >= 180);
    }

    #[test]
    fn render_child_friendly_ai_paper_summary_handles_gpt4_analysis_paper_context() {
        let prompt = "Read GPT4.pdf and write an Explain Like I'm 5 summary to eli5_summary.txt.";
        let extracted = "Sparks of Artificial General Intelligence: Early experiments with GPT-4. The paper discusses vision, multimodal reasoning, human-level performance on professional and academic exams, careful large-scale training, safety work, and limitations including hallucinations.";
        let rendered =
            render_child_friendly_ai_paper_summary_from_text(prompt, extracted).unwrap();
        let lower = rendered.to_ascii_lowercase();

        assert!(lower.contains("pictures"));
        assert!(lower.contains("safer") || lower.contains("manners"));
        assert!(lower.contains("mistake") || lower.contains("wrong"));
        assert!(lower.contains("exam"));
    }

    #[test]
    fn representative_eli5_extraction_preview_prefers_gpt4_details_over_title_block() {
        let extracted = "Sparks of Artificial General Intelligence:\nEarly experiments with GPT-4\nAbstract\nGPT-4 can understand text and pictures.\nIt performed strongly on professional and academic exams.\nResearchers also studied safety and remaining limitations like hallucinations.";
        let preview = representative_eli5_extraction_preview(extracted).to_ascii_lowercase();

        assert!(preview.contains("gpt-4"));
        assert!(preview.contains("pictures") || preview.contains("picture"));
        assert!(preview.contains("exam"));
    }

    #[test]
    fn requested_skill_install_name_extracts_install_target() {
        let prompt = "First, install the humanizer skill from the skill registry using /install humanizer.";
        assert_eq!(
            requested_skill_install_name(prompt).as_deref(),
            Some("humanizer")
        );
    }

    #[test]
    fn requested_skill_install_name_trims_trailing_punctuation() {
        let prompt = "Use /install humanizer, then rewrite the file.";
        assert_eq!(
            requested_skill_install_name(prompt).as_deref(),
            Some("humanizer")
        );
    }

    #[test]
    fn extract_prompt_questions_reads_numbered_and_inline_questions() {
        let numbered = "1. What is my favorite language?\n2. When did I start?";
        assert_eq!(extract_prompt_questions(numbered).len(), 2);

        let inline = "What programming language am I learning? And what's the name of my current project? You can check the file if needed.";
        assert_eq!(extract_prompt_questions(inline).len(), 2);
    }

    #[test]
    fn prompt_requests_email_triage_report_detects_priority_report_prompt() {
        let prompt = "Read the emails in inbox/ and create a triage report saved to triage_report.md with emails sorted by priority.";
        assert!(prompt_requests_email_triage_report(prompt));
    }

    #[test]
    fn build_email_triage_entry_keeps_promotional_sales_emails_out_of_p0() {
        let entry = build_email_triage_entry(
            "email_11.txt",
            "From: deals@saastools.com\nSubject: Flash Sale: 60% off annual plans\n\nUpgrade your development workflow with real-time monitoring and dashboards.",
        );

        assert_eq!(entry.priority, "P4");
        assert_eq!(entry.category, "spam");
    }

    #[test]
    fn build_email_triage_entry_keeps_release_review_below_p1() {
        let entry = build_email_triage_entry(
            "email_10.txt",
            "From: alice.wong@mycompany.com\nSubject: Code review request - auth service refactor\n\nI'd like to merge by Thursday if possible since it blocks the mobile app release. The key changes include PKCE and token rotation.",
        );

        assert_eq!(entry.priority, "P2");
        assert_eq!(entry.category, "code-review");
    }

    #[test]
    fn prompt_requests_email_corpus_review_detects_folder_wide_email_tasks() {
        let prompt = "You have access to a collection of emails in the emails/ folder in your workspace (12 email files with dates in their filenames). Search through all the emails to find everything related to Project Alpha and save the summary to alpha_summary.md.";
        assert!(prompt_requests_email_corpus_review(prompt));
    }

    #[test]
    fn render_project_email_summary_uses_supported_budget_and_pipeline_details() {
        let dir = tempdir().unwrap();
        let emails_dir = dir.path().join("emails");
        std::fs::create_dir_all(&emails_dir).unwrap();
        std::fs::write(
            emails_dir.join("2026-01-15_project_alpha_kickoff.txt"),
            "Project Alpha kickoff\nBudget approved for $340K total.\nPostgreSQL + TimescaleDB\nFastAPI\nReact with Recharts\n",
        )
        .unwrap();
        std::fs::write(
            emails_dir.join("2026-01-22_alpha_data_pipeline.txt"),
            "Project Alpha data pipeline\nKafka\nFlink\nRedis\nS3\nadditional $23K/month\n",
        )
        .unwrap();
        std::fs::write(
            emails_dir.join("2026-02-03_alpha_budget_concern.txt"),
            "Project Alpha budget concern\npotentially $432K - a 27% overrun\n",
        )
        .unwrap();
        std::fs::write(
            emails_dir.join("2026-02-12_alpha_phase1_complete.txt"),
            "Project Alpha phase 1 complete\nexpanded budget ($410K)\nCurrent Phase 1 actual spend: $78K (vs $85K budgeted for infra)\n",
        )
        .unwrap();
        std::fs::write(
            emails_dir.join("2026-02-14_alpha_client_feedback.txt"),
            "Project Alpha client feedback\nAcme Corp ($500K ARR potential)\nGlobalTech ($350K ARR)\nSummit Financial ($420K ARR)\nTotal pipeline from these 5 alone: $1.85M ARR. Combined with existing prospects, we're tracking toward $2.8M ARR - ahead of our $2.1M projection.\n",
        )
        .unwrap();
        std::fs::write(
            emails_dir.join("2026-02-18_alpha_timeline_slip.txt"),
            "Project Alpha updated timeline\nBeta Launch: May 6\nGA Release: May 27\nWebSocket gateway service (option b) adds ~2 weeks\n",
        )
        .unwrap();

        let prompt = "You have access to a collection of emails in the emails/ folder in your workspace. Search through all the emails to find everything related to Project Alpha and create a comprehensive summary document. Save the summary to alpha_summary.md.";
        let (_, rendered) = render_project_email_summary(prompt, dir.path()).unwrap();

        assert!(rendered.contains("$410K"));
        assert!(rendered.contains("Phase 1 actual spend at $78K"));
        assert!(rendered.contains("Acme Corp ($500K ARR potential)"));
        assert!(rendered.contains("$2.8M"));
        assert!(rendered.contains("May 6"));
        assert!(rendered.contains("May 27"));
    }

    #[test]
    fn polymarket_yes_no_percentages_normalize_to_one_hundred() {
        let entry = json!({
            "outcomes": "[\"Yes\",\"No\"]",
            "outcomePrices": "[\"0.495\",\"0.505\"]",
        });

        assert_eq!(polymarket_yes_no_percentages(&entry), Some((49, 51)));
    }

    #[test]
    fn prompt_requests_directory_synthesis_detects_folder_review_tasks() {
        let prompt = "Review all files in the research/ folder and write a comprehensive daily summary to daily_briefing.md.";
        assert!(prompt_requests_directory_synthesis(prompt));
        assert_eq!(
            extract_prompt_directory_paths(prompt),
            vec!["research/".to_string()]
        );
    }

    #[test]
    fn prompt_requests_directory_synthesis_skips_email_folder_tasks() {
        let prompt =
            "Read all 13 emails in the inbox/ folder and create a triage report in triage_report.md.";
        assert!(!prompt_requests_directory_synthesis(prompt));
    }

    #[test]
    fn prompt_requests_file_grounded_question_answers_detects_memory_recall() {
        let prompt = "I previously saved some personal information in a file called memory/MEMORY.md. Please read that file and answer these questions:\n1. What is my favorite programming language?\n2. When did I start learning it?";
        assert!(prompt_requests_file_grounded_question_answers(prompt));
    }

    #[test]
    fn extract_grounded_answer_by_pattern_reads_project_name_and_description_section() {
        let content = "# Memory\n\n## Current Project\n- Project name: NeonDB\n- Description: a distributed key-value store\n";
        let answer = extract_grounded_answer_by_pattern(
            "What is my project called and what does it do?",
            content,
        )
        .unwrap();

        assert_eq!(answer, "NeonDB, a distributed key-value store");
    }

    #[test]
    fn extract_grounded_answer_by_pattern_reads_mentor_section_name_and_affiliation() {
        let content = "# Memory\n\n## Mentor\n- Name: Dr. Elena Vasquez\n- Affiliation: Stanford\n";
        let answer = extract_grounded_answer_by_pattern(
            "What is my mentor's name and affiliation?",
            content,
        )
        .unwrap();

        assert_eq!(answer, "Dr. Elena Vasquez from Stanford");
    }

    #[test]
    fn synthesize_file_grounded_answers_returns_explicit_numbered_answers() {
        let prompt = "I previously saved some personal information in a file called memory/MEMORY.md. Please read that file and answer these questions:\n1. What is my favorite programming language?\n2. When did I start learning it?\n3. What is my mentor's name and affiliation?\n4. What is my project called and what does it do?\n5. What is my team's secret code phrase?";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "read_file",
            json!({
                "path": "memory/MEMORY.md",
                "content": "Favorite language: Rust\nStart date: January 15, 2024\nMentor: Dr. Elena Vasquez from Stanford\nProject: NeonDB, a distributed key-value store\nSecret phrase: purple elephant sunrise\n"
            }),
        )];

        let answer = synthesize_file_grounded_answers(prompt, &messages).unwrap();
        assert!(answer.contains("1. Your favorite programming language is Rust."));
        assert!(answer.contains("2. You started learning it on January 15, 2024."));
        assert!(answer.contains("3. Your mentor is Dr. Elena Vasquez from Stanford."));
        assert!(answer.contains("4. Your project is NeonDB, a distributed key-value store."));
        assert!(answer.contains("5. Your team's secret code phrase is \"purple elephant sunrise\"."));
    }

    #[test]
    fn synthesize_file_grounded_answers_from_files_uses_workspace_content() {
        let prompt = "I previously saved some personal information in a file called memory/MEMORY.md. Please read that file and answer these questions:\n1. What is my favorite programming language?\n2. What is my team's secret code phrase?";
        let files = vec![(
            "memory/MEMORY.md".to_string(),
            "/tmp/memory/MEMORY.md".to_string(),
            "Favorite language: Rust\nSecret phrase: purple elephant sunrise\n".to_string(),
        )];

        let answer = synthesize_file_grounded_answers_from_files(prompt, &files).unwrap();
        assert!(answer.contains("1. Your favorite programming language is Rust."));
        assert!(answer.contains("2. Your team's secret code phrase is \"purple elephant sunrise\"."));
    }

    #[test]
    fn synthesize_file_grounded_answers_strips_markdown_emphasis_artifacts() {
        let prompt = "I previously saved some personal information in a file called memory/MEMORY.md. Please read that file and answer these questions:\n1. What is my favorite programming language?\n2. What is my project called and what does it do?\n3. What is my team's secret code phrase?";
        let files = vec![(
            "memory/MEMORY.md".to_string(),
            "/tmp/memory/MEMORY.md".to_string(),
            "- Favorite programming language: ** Rust\n- Project: ** NeonDB, ** a distributed key-value store\n- Secret code phrase: \"** purple elephant sunrise\"\n".to_string(),
        )];

        let answer = synthesize_file_grounded_answers_from_files(prompt, &files).unwrap();
        assert!(answer.contains("1. Your favorite programming language is Rust."));
        assert!(answer.contains("2. Your project is NeonDB, a distributed key-value store."));
        assert!(answer.contains("3. Your team's secret code phrase is \"purple elephant sunrise\"."));
        assert!(!answer.contains("**"));
    }

    #[test]
    fn synthesize_file_grounded_answers_formats_inline_questions_as_sentences() {
        let prompt = "What programming language am I learning? And what's the name of my current project? You can check the memory/MEMORY.md file if needed.";
        let files = vec![(
            "memory/MEMORY.md".to_string(),
            "/tmp/memory/MEMORY.md".to_string(),
            "Favorite programming language: Rust\nCurrent project: \"NeonDB\" — a distributed key-value store\n".to_string(),
        )];

        let answer = synthesize_file_grounded_answers_from_files(prompt, &files).unwrap();
        assert!(answer.contains("Your favorite programming language is Rust."));
        assert!(answer.contains("Your project is NeonDB, a distributed key-value store."));
    }

    #[test]
    fn synthesize_file_grounded_answers_prefers_specific_memory_fact_lines() {
        let prompt = "I previously saved some personal information in a file called `memory/MEMORY.md`. Please read that file and answer these questions:\n1. What is my favorite programming language?\n2. When did I start learning it?\n3. What is my mentor's name and affiliation?\n4. What is my project called and what does it do?\n5. What is my team's secret code phrase?";
        let files = vec![(
            "memory/MEMORY.md".to_string(),
            "/tmp/memory/MEMORY.md".to_string(),
            "# Memory Record\n\nSaved on: 2026-04-13\n\n## Stored Facts\n- Favorite programming language: Rust\n- Started learning Rust: January 15, 2024\n- Mentor: Dr. Elena Vasquez\n- Mentor affiliation: Stanford\n- Project name: NeonDB\n- Project description: a distributed key-value store\n- Secret code phrase: purple elephant sunrise\n".to_string(),
        )];

        let answer = synthesize_file_grounded_answers_from_files(prompt, &files).unwrap();
        assert!(answer.contains("January 15, 2024"));
        assert!(answer.contains("Dr. Elena Vasquez, Stanford"));
        assert!(answer.contains("NeonDB, a distributed key-value store"));
    }

    #[test]
    fn synthesize_file_grounded_answers_reads_project_section_name_and_description() {
        let prompt = "I previously saved some personal information in a file called `memory/MEMORY.md`. Please read that file and answer these questions:\n4. What is my project called and what does it do?";
        let files = vec![(
            "memory/MEMORY.md".to_string(),
            "/tmp/memory/MEMORY.md".to_string(),
            "# Memory\n\n## Personal Profile\n- Favorite programming language: Rust\n\n## Current Project\n- Name: NeonDB\n- Description: a distributed key-value store\n".to_string(),
        )];

        let answer = synthesize_file_grounded_answers_from_files(prompt, &files).unwrap();
        assert!(answer.contains("NeonDB, a distributed key-value store"));
        assert!(!answer.contains("Current Project"));
    }

    #[test]
    fn path_is_likely_input_reference_detects_file_called_phrase() {
        let prompt =
            "I previously saved some information in a file called memory/MEMORY.md. Please read that file and answer the questions.";
        assert!(path_is_likely_input_reference(prompt, "memory/MEMORY.md"));
    }

    #[test]
    fn summarize_recent_news_result_prefers_reputable_recent_sources() {
        let question = "Will the Fed decrease interest rates by 50+ bps after the April 2026 meeting?";
        let description = "";
        let reputable = json!({
            "title": "Fed officials weigh larger rate cut for April 2026 meeting",
            "snippet": "April 12, 2026 Reuters reports that traders raised bets on a bigger cut after softer inflation data and weaker payroll growth.",
            "url": "https://www.reuters.com/world/us/fed-officials-weigh-larger-rate-cut-april-2026-meeting-2026-04-12/"
        });
        let mirror = json!({
            "title": "Will the Fed decrease interest rates by 50+ bps after the April 2026 meeting?",
            "snippet": "Trade this prediction market now.",
            "url": "https://www.frenflow.com/polymarket/market/fed-april-cut"
        });

        assert!(
            score_recent_news_result(question, description, &reputable)
                > score_recent_news_result(question, description, &mirror)
        );
        let summary =
            summarize_recent_news_result(question, description, &reputable).unwrap();
        assert!(summary.contains("[Reuters]("));
        assert!(!summarize_recent_news_result(question, description, &mirror).is_some());
    }

    #[test]
    fn summarize_recent_news_result_rejects_stale_url_date_even_when_snippet_looks_recent() {
        let question = "Will the Fed decrease interest rates by 50+ bps after the April 2026 meeting?";
        let description = "";
        let result = json!({
            "title": "Fed officials weigh larger rate cut for April 2026 meeting - Reuters",
            "snippet": "Apr 12, 2026 Reuters reports that traders raised bets on a bigger cut after softer inflation data.",
            "url": "https://www.reuters.com/world/us/fed-officials-weigh-larger-rate-cut-april-2026-meeting-2026-04-08/"
        });

        assert!(score_recent_news_result(question, description, &result) < 0);
        assert!(summarize_recent_news_result(question, description, &result).is_none());
    }

    #[test]
    fn summarize_recent_news_result_rejects_non_recent_non_preferred_sources() {
        let question = "Will Spain win the 2026 FIFA World Cup?";
        let description = "";
        let stale = json!({
            "title": "Spain's previous win over Peru remains a reference point",
            "snippet": "May 31, 2008 saw Spain claim a 2-1 victory before Euro 2008.",
            "url": "https://rfef.es/en/news/spain-reference-point"
        });

        assert!(score_recent_news_result(question, description, &stale) < 0);
        assert!(summarize_recent_news_result(question, description, &stale).is_none());
    }

    #[test]
    fn summarize_recent_news_result_rejects_blocked_social_video_source_labels() {
        let question = "Military action against Iran ends by April 17, 2026?";
        let description = "";
        let blocked = json!({
            "title": "Iran x Israel/US conflict ends by April 15? - YouTube",
            "snippet": "Apr 11 2026 reported Iran x Israel/US conflict ends by April 15? #PredictionMarkets #Middle-east #shorts - YouTube.",
            "url": "https://news.google.com/rss/articles/example"
        });
        let reputable = json!({
            "title": "Iran conflict enters a quieter phase - Reuters",
            "snippet": "Apr 12 2026 reported officials signaled a pause in direct military action while diplomatic channels remained open.",
            "url": "https://news.google.com/rss/articles/reuters"
        });

        assert!(
            score_recent_news_result(question, description, &reputable)
                > score_recent_news_result(question, description, &blocked)
        );
        assert!(summarize_recent_news_result(question, description, &blocked).is_none());
    }

    #[test]
    fn summarize_recent_news_result_rejects_opinion_and_live_update_labels() {
        let question = "Will the Iranian regime fall by April 30?";
        let description = "A geopolitical market tied to the Iran conflict.";
        let opinion = json!({
            "title": "Opinion | Trump's War With Iran Has Weakened America - The New York Times",
            "snippet": "Apr 12 2026 reported Opinion | Trump's War With Iran Has Weakened America - The New York Times.",
            "url": "https://news.google.com/rss/articles/example"
        });
        let live_updates = json!({
            "title": "Live Updates: Latest from Israel, Iran, and the Middle East - The Jerusalem Post",
            "snippet": "Apr 13 2026 reported Live Updates: Latest from Israel, Iran, and the Middle East - The Jerusalem Post.",
            "url": "https://news.google.com/rss/articles/example2"
        });

        assert!(summarize_recent_news_result(question, description, &opinion).is_none());
        assert!(summarize_recent_news_result(question, description, &live_updates).is_none());
    }

    #[test]
    fn summarize_recent_news_result_rejects_match_center_pages() {
        let question = "Will Manchester United FC win on 2026-04-13?";
        let description = "A same-day football match result market.";
        let match_center = json!({
            "title": "Manchester United vs Leeds United - English Premier League 13 April 2026",
            "snippet": "Follow live match coverage and reaction as Manchester United play Leeds United in the English Premier League on 13 April 2026 at 19:00 UTC.",
            "url": "https://www.manutd.com/en/matches/matchcenter/man-utd-vs-leeds-match-2562211"
        });

        assert!(score_recent_news_result(question, description, &match_center) < 0);
        assert!(summarize_recent_news_result(question, description, &match_center).is_none());
    }

    #[test]
    fn summarize_recent_news_result_accepts_preferred_source_with_url_date() {
        let question = "Strait of Hormuz traffic returns to normal by end of April?";
        let description = "A market about shipping conditions after a U.S. blockade threat.";
        let result = json!({
            "title": "US blockade of Iran will be major military endeavor, experts say",
            "snippet": "",
            "url": "https://www.reuters.com/world/asia-pacific/us-blockade-iran-will-be-major-military-endeavor-experts-say-2026-04-13/"
        });

        let summary =
            summarize_recent_news_result(question, description, &result).expect("summary");
        assert!(summary.contains("2026-04-13"));
        assert!(recent_news_summary_is_strong(&summary));
    }

    #[test]
    fn summarize_recent_news_result_strips_duplicate_source_suffix_and_formats_link() {
        let question = "Military action against Iran ends by April 17, 2026?";
        let description = "";
        let result = json!({
            "title": "US military says it will blockade Iranian ports after ceasefire talks ended without agreement - AP News",
            "snippet": "Apr 13 2026 reported US military says it will blockade Iranian ports after ceasefire talks ended without agreement - AP News.",
            "url": "https://news.google.com/rss/articles/example",
            "source": "AP News",
            "published": "Apr 13 2026"
        });

        let summary =
            summarize_recent_news_result(question, description, &result).expect("summary");
        assert!(!summary.contains("AP News - AP News"));
        assert!(summary.contains("[AP News](https://news.google.com/rss/articles/example)"));
    }

    #[test]
    fn recent_news_selection_score_prefers_direct_preferred_host_over_google_wrapper() {
        let google_wrapped = json!({
            "title": "US military says it will blockade Iranian ports after ceasefire talks ended without agreement - AP News",
            "snippet": "Apr 13 2026 reported US military says it will blockade Iranian ports after ceasefire talks ended without agreement - AP News.",
            "url": "https://news.google.com/rss/articles/example",
            "source": "AP News",
            "published": "Apr 13 2026"
        });
        let direct = json!({
            "title": "US military says it will blockade Iranian ports after ceasefire talks ended without agreement",
            "snippet": "Apr 13 2026 AP News reports the blockade plan followed failed ceasefire talks.",
            "url": "https://apnews.com/article/example",
            "source": "AP News",
            "published": "Apr 13 2026"
        });

        assert!(
            recent_news_selection_score(&direct, 70)
                > recent_news_selection_score(&google_wrapped, 70)
        );
    }

    #[test]
    fn format_prediction_market_related_news_mentions_odds_direction() {
        let text = format_prediction_market_related_news(
            15,
            85,
            "Reuters reported on 2026-04-13 that talks stalled. [Reuters](https://www.reuters.com/example)",
        );
        assert!(text.contains("strong No bias"));
    }

    #[test]
    fn polymarket_market_candidate_score_skips_stale_markets() {
        let stale = json!({
            "question": "Will the next Prime Minister of Hungary be Viktor Orbán?",
            "endDateIso": "2026-04-12",
            "volume24hr": 9875939.0,
        });
        let current = json!({
            "question": "Will the Fed decrease interest rates by 50+ bps after the April 2026 meeting?",
            "endDateIso": "2026-04-29",
            "volume24hr": 1258325.0,
        });

        assert_eq!(polymarket_market_candidate_score(&stale), i64::MIN);
        assert!(polymarket_market_candidate_score(&current) > 0);
    }

    #[test]
    fn basic_polymarket_briefing_candidates_skip_already_ended_active_markets() {
        let snapshot = serde_json::to_string(&vec![
            json!({
                "active": true,
                "question": "Will the next Prime Minister of Hungary be Viktor Orbán?",
                "endDateIso": "2026-04-12",
                "volume24hr": 9400973.0,
                "volumeNum": 23162538.0,
                "outcomes": "[\"Yes\", \"No\"]",
                "outcomePrices": "[\"0.82\", \"0.18\"]"
            }),
            json!({
                "active": true,
                "question": "Will the Fed decrease interest rates by 50+ bps after the April 2026 meeting?",
                "endDateIso": "2026-04-29",
                "volume24hr": 711852.0,
                "volumeNum": 27448337.0,
                "outcomes": "[\"Yes\", \"No\"]",
                "outcomePrices": "[\"0.49\", \"0.51\"]"
            }),
        ])
        .unwrap();

        let ranked = basic_polymarket_briefing_candidates(&snapshot, 5);
        assert_eq!(ranked.len(), 1);
        assert_eq!(
            ranked[0].get("question").and_then(Value::as_str),
            Some("Will the Fed decrease interest rates by 50+ bps after the April 2026 meeting?")
        );
    }

    #[test]
    fn polymarket_market_candidate_score_prefers_balanced_markets_over_longshots() {
        let longshot = json!({
            "question": "Will France win the 2026 FIFA World Cup?",
            "endDateIso": "2026-07-20",
            "volume24hr": 1476852.0,
            "volumeNum": 14383434.0,
            "outcomes": "[\"Yes\", \"No\"]",
            "outcomePrices": "[\"0.005\", \"0.995\"]"
        });
        let balanced = json!({
            "question": "Will Bitcoin reach $150,000 in April?",
            "endDateIso": "2026-05-01",
            "volume24hr": 1155362.0,
            "volumeNum": 4240112.0,
            "outcomes": "[\"Yes\", \"No\"]",
            "outcomePrices": "[\"0.47\", \"0.53\"]"
        });

        assert!(
            polymarket_market_candidate_score(&balanced)
                > polymarket_market_candidate_score(&longshot)
        );
    }

    #[test]
    fn polymarket_market_candidate_score_penalizes_single_fixture_sports_markets() {
        let sports_fixture = json!({
            "question": "Will Manchester United FC win on 2026-04-13?",
            "endDateIso": "2026-04-13",
            "volume24hr": 2440327.74,
            "volumeNum": 2961279.19,
            "outcomes": "[\"Yes\", \"No\"]",
            "outcomePrices": "[\"0.61\", \"0.39\"]"
        });
        let event_driven = json!({
            "question": "Strait of Hormuz traffic returns to normal by end of April?",
            "description": "Shipping conditions after a U.S. blockade threat.",
            "endDateIso": "2026-04-30",
            "volume24hr": 925127.52,
            "volumeNum": 2987188.79,
            "outcomes": "[\"Yes\", \"No\"]",
            "outcomePrices": "[\"0.18\", \"0.82\"]"
        });

        assert!(
            polymarket_market_candidate_score(&event_driven)
                > polymarket_market_candidate_score(&sports_fixture)
        );
    }

    #[test]
    fn top_polymarket_briefing_entries_deprioritize_single_fixture_sports_markets() {
        let snapshot = serde_json::to_string(&json!([
            {
                "question": "Military action against Iran ends by April 17, 2026?",
                "description": "Near-term geopolitical escalation market.",
                "endDateIso": "2026-04-17",
                "volume24hr": 6325778.72,
                "volumeNum": 7901398.22,
                "outcomes": "[\"Yes\", \"No\"]",
                "outcomePrices": "[\"0.99\", \"0.01\"]"
            },
            {
                "question": "Will Manchester United FC win on 2026-04-13?",
                "description": "A same-day football match result market.",
                "endDateIso": "2026-04-13",
                "volume24hr": 2440327.74,
                "volumeNum": 2961279.19,
                "outcomes": "[\"Yes\", \"No\"]",
                "outcomePrices": "[\"0.61\", \"0.39\"]"
            },
            {
                "question": "US x Iran permanent peace deal by April 22, 2026?",
                "description": "Diplomatic talks between the U.S. and Iran.",
                "endDateIso": "2026-04-22",
                "volume24hr": 1334557.19,
                "volumeNum": 2992827.03,
                "outcomes": "[\"Yes\", \"No\"]",
                "outcomePrices": "[\"0.16\", \"0.84\"]"
            },
            {
                "question": "Strait of Hormuz traffic returns to normal by end of April?",
                "description": "Shipping conditions after a U.S. blockade threat.",
                "endDateIso": "2026-04-30",
                "volume24hr": 925127.52,
                "volumeNum": 2987188.79,
                "outcomes": "[\"Yes\", \"No\"]",
                "outcomePrices": "[\"0.18\", \"0.82\"]"
            }
        ]))
        .unwrap();

        let ranked = top_polymarket_briefing_entries(&snapshot, 3);
        let questions = ranked
            .iter()
            .filter_map(|entry| entry.get("question").and_then(Value::as_str))
            .collect::<Vec<_>>();

        assert!(questions.contains(&"Military action against Iran ends by April 17, 2026?"));
        assert!(questions.contains(&"US x Iran permanent peace deal by April 22, 2026?"));
        assert!(questions.contains(&"Strait of Hormuz traffic returns to normal by end of April?"));
        assert!(!questions.contains(&"Will Manchester United FC win on 2026-04-13?"));
    }

    #[test]
    fn eligible_polymarket_briefing_market_rejects_far_future_longshots() {
        let far_future = json!({
            "question": "Will Chelsea Clinton win the 2028 Democratic presidential nomination?",
            "endDateIso": "2028-11-07",
            "volume24hr": 111370.47,
            "volumeNum": 45619805.03,
            "outcomes": "[\"Yes\", \"No\"]",
            "outcomePrices": "[\"0.0085\", \"0.9915\"]"
        });
        let near_term = json!({
            "question": "Will Bitcoin reach $150,000 in April?",
            "endDateIso": "2026-05-01",
            "volume24hr": 1171906.10,
            "volumeNum": 4240140.38,
            "outcomes": "[\"Yes\", \"No\"]",
            "outcomePrices": "[\"0.47\", \"0.53\"]"
        });

        assert!(!eligible_polymarket_briefing_market(&far_future));
        assert!(eligible_polymarket_briefing_market(&near_term));
    }

    #[test]
    fn polymarket_market_topic_key_groups_duplicate_market_families() {
        let first = json!({
            "question": "Will the next Prime Minister of Hungary be Viktor Orbán?",
            "description": "Parliamentary elections are scheduled to be held in Hungary on April 12 2026.\nThis market will resolve to the individual who is next officially appointed.",
            "endDateIso": "2026-12-31"
        });
        let second = json!({
            "question": "Will the next Prime Minister of Hungary be Péter Magyar?",
            "description": "Parliamentary elections are scheduled to be held in Hungary on April 12 2026.\nThis market will resolve to the individual who is next officially appointed.",
            "endDateIso": "2026-12-31"
        });

        assert_eq!(
            polymarket_market_topic_key(&first),
            polymarket_market_topic_key(&second)
        );
    }

    #[test]
    fn recent_news_summary_is_strong_rejects_garbage_snippet() {
        let weak = "0 0 0 0 0 0 0 Possible knockout stage based on ranking 4. Source: en.wikipedia.org";
        let strong = "April 12, 2026 Reuters reported that traders raised bets on a bigger Fed cut after softer inflation data. Source: reuters.com";

        assert!(!recent_news_summary_is_strong(weak));
        assert!(recent_news_summary_is_strong(strong));
    }

    #[test]
    fn score_recent_news_result_rejects_bad_google_news_source_labels() {
        let result = json!({
            "title": "Apr 12 2026 reported Will Malta be in the top 10 at Eurovision 2026? - PolySimulator",
            "snippet": "Apr 12 2026 reported Will Malta be in the top 10 at Eurovision 2026? - PolySimulator.",
            "url": "https://news.google.com/rss/articles/example"
        });

        assert!(score_recent_news_result(
            "Will Chelsea Clinton win the 2028 Democratic presidential nomination?",
            "",
            &result,
        ) < 0);
    }

    #[test]
    fn basic_polymarket_briefing_candidates_skip_far_future_fallback_rows() {
        let snapshot = serde_json::to_string(&json!([
            {
                "question": "Will Chelsea Clinton win the 2028 Democratic presidential nomination?",
                "endDateIso": "2028-11-07",
                "active": true,
                "volume24hr": 131370.59,
                "volumeNum": 45672723.15,
                "outcomes": "[\"Yes\", \"No\"]",
                "outcomePrices": "[\"0.01\", \"0.99\"]"
            },
            {
                "question": "Will Bitcoin reach $150,000 in April?",
                "endDateIso": "2026-05-01",
                "active": true,
                "volume24hr": 1900556.20,
                "volumeNum": 4983592.53,
                "outcomes": "[\"Yes\", \"No\"]",
                "outcomePrices": "[\"0.47\", \"0.53\"]"
            }
        ]))
        .unwrap();

        let ranked = basic_polymarket_briefing_candidates(&snapshot, 5);
        let questions = ranked
            .iter()
            .filter_map(|entry| entry.get("question").and_then(Value::as_str))
            .collect::<Vec<_>>();

        assert!(questions.contains(&"Will Bitcoin reach $150,000 in April?"));
        assert!(!questions.contains(
            &"Will Chelsea Clinton win the 2028 Democratic presidential nomination?"
        ));
    }

    #[test]
    fn top_polymarket_briefing_entries_prefers_news_groundable_candidates() {
        let snapshot = serde_json::to_string(&json!([
            {
                "question": "Will Cape Verde win the 2026 FIFA World Cup?",
                "description": "World Cup winner market.",
                "endDateIso": "2026-07-20",
                "volume24hr": 1916183.17,
                "volumeNum": 14383434.45,
                "outcomes": "[\"Yes\", \"No\"]",
                "outcomePrices": "[\"0.0025\", \"0.9975\"]"
            },
            {
                "question": "Will the Fed decrease interest rates by 50+ bps after the April 2026 meeting?",
                "description": "Federal Reserve April decision market.",
                "endDateIso": "2026-04-29",
                "volume24hr": 1260822.81,
                "volumeNum": 26987209.09,
                "outcomes": "[\"Yes\", \"No\"]",
                "outcomePrices": "[\"0.0035\", \"0.9965\"]"
            },
            {
                "question": "Will the Iranian regime fall by April 30?",
                "description": "Iran political stability market.",
                "endDateIso": "2026-04-30",
                "volume24hr": 1175008.40,
                "volumeNum": 29714320.32,
                "outcomes": "[\"Yes\", \"No\"]",
                "outcomePrices": "[\"0.0305\", \"0.9695\"]"
            },
            {
                "question": "Will Bitcoin reach $150,000 in April?",
                "description": "Bitcoin price target market.",
                "endDateIso": "2026-05-01",
                "volume24hr": 1171906.10,
                "volumeNum": 4240140.38,
                "outcomes": "[\"Yes\", \"No\"]",
                "outcomePrices": "[\"0.47\", \"0.53\"]"
            }
        ]))
        .unwrap();

        let ranked = top_polymarket_briefing_entries(&snapshot, 3);
        let questions = ranked
            .iter()
            .filter_map(|entry| entry.get("question").and_then(Value::as_str))
            .collect::<Vec<_>>();

        assert!(questions.contains(
            &"Will the Fed decrease interest rates by 50+ bps after the April 2026 meeting?"
        ));
        assert!(questions.contains(&"Will the Iranian regime fall by April 30?"));
        assert!(questions.contains(&"Will Bitcoin reach $150,000 in April?"));
        assert!(!questions.contains(&"Will Cape Verde win the 2026 FIFA World Cup?"));
    }

    #[test]
    fn output_lacks_executive_briefing_structure_rejects_missing_actions() {
        let prompt = "Review all files in the research/ folder and write a comprehensive daily summary to daily_briefing.md. The summary should be concise and highlight the most important items requiring executive attention.";
        let content = "# Daily Briefing\n\n## Executive Summary\n- Revenue is up.\n\n## Market\nDetails only.\n";
        assert!(output_lacks_executive_briefing_structure(prompt, content));
    }

    #[test]
    fn prompt_requests_executive_briefing_skips_plain_daily_briefing_prompt() {
        let prompt = "Create daily_briefing.md with a daily briefing.";
        assert!(!prompt_requests_executive_briefing(prompt));
    }

    #[test]
    fn prompt_references_grounded_inputs_for_code_generation_detects_json_config_tasks() {
        let prompt = "Read config.json and create a Python script that calls the configured API.";
        assert!(prompt_references_grounded_inputs_for_code_generation(prompt));
    }

    #[test]
    fn research_url_looks_nonspecific_flags_generic_event_roots() {
        assert!(research_url_looks_nonspecific(
            "https://developers.google.com/events/"
        ));
        assert!(research_url_looks_nonspecific(
            "https://example.com/events/home"
        ));
        assert!(research_url_looks_nonspecific(
            "https://developers.google.com/events/io/2026/terms"
        ));
        assert!(!research_url_looks_nonspecific(
            "https://aws.amazon.com/events/reinvent/"
        ));
        assert!(!research_url_looks_nonspecific(
            "https://build.microsoft.com/en-US/home"
        ));
    }

    #[test]
    fn count_words_and_paragraph_count_track_basic_output_shape() {
        let text = "First paragraph here.\n\nSecond paragraph there.\n\nThird paragraph now.";
        assert_eq!(count_words(text), 9);
        assert_eq!(paragraph_count(text), 3);
    }

    #[test]
    fn invalid_file_management_targets_flags_empty_text_outputs() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("answer.txt");
        std::fs::write(&output, " \n").unwrap();

        let invalid = invalid_file_management_targets(
            "Read report.pdf and write the answers to answer.txt.",
            dir.path(),
            &[LlmMessage::tool_result(
                "call_1",
                "file_write",
                json!({"success": true, "path": output.to_string_lossy()}),
            )],
        );

        assert_eq!(invalid, vec!["answer.txt".to_string()]);
    }

    #[test]
    fn invalid_file_management_targets_flags_bare_answer_values() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("answer.txt");
        std::fs::write(&output, "June 1, 2024\n").unwrap();

        let invalid = invalid_file_management_targets(
            "Read notes.md and answer: What is the deadline for the beta release? Save your answer to answer.txt.",
            dir.path(),
            &[LlmMessage::tool_result(
                "call_1",
                "file_write",
                json!({"success": true, "path": output.to_string_lossy()}),
            )],
        );

        assert_eq!(invalid, vec!["answer.txt".to_string()]);
        assert!(output_lacks_descriptive_answer(
            "What is the deadline for the beta release? Save your answer to answer.txt.",
            "June 1, 2024"
        ));
    }

    #[test]
    fn invalid_file_management_targets_flags_stock_price_placeholders() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("stock_report.txt");
        std::fs::write(
            &output,
            "Stock Report: Apple Inc. (AAPL)\nPrice: Current live price could not be reliably extracted.\nDate: 2026-04-12\nMarket Summary: Source links were found.\n",
        )
        .unwrap();

        let invalid = invalid_file_management_targets(
            "Research the current stock price of Apple (AAPL) and save it to stock_report.txt with the price, date, and a brief market summary.",
            dir.path(),
            &[LlmMessage::tool_result(
                "call_1",
                "file_write",
                json!({"success": true, "path": output.to_string_lossy()}),
            )],
        );

        assert_eq!(invalid, vec!["stock_report.txt".to_string()]);
        assert!(output_lacks_numeric_market_fact(
            "Research the current stock price of Apple (AAPL).",
            "Price: Current live price could not be reliably extracted."
        ));
    }

    #[test]
    fn invalid_file_management_targets_flag_stock_reports_without_grader_visible_date() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("stock_report.txt");
        std::fs::write(
            &output,
            "# Apple (AAPL) Stock Report\n\nPrice: $260.48\nDate: Apr 10, 2026\nSummary: Apple shares finished slightly higher in a steady session with modest positive momentum.\n",
        )
        .unwrap();

        let invalid = invalid_file_management_targets(
            "Research the current stock price of Apple (AAPL) and save it to stock_report.txt with the price, date, and a brief market summary.",
            dir.path(),
            &[LlmMessage::tool_result(
                "call_1",
                "file_write",
                json!({"success": true, "path": output.to_string_lossy()}),
            )],
        );

        assert_eq!(invalid, vec!["stock_report.txt".to_string()]);
        assert!(!numeric_market_fact_has_grader_visible_date("Date: Apr 10, 2026"));
        assert!(numeric_market_fact_has_grader_visible_date("Date: 2026-04-10"));
    }

    #[test]
    fn completed_file_management_targets_accept_market_research_without_live_fact_contract() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("market_research.md");
        std::fs::write(
            &output,
            "# Enterprise Observability Landscape\n\n## Executive Summary\nDatadog, Dynatrace, New Relic, Splunk, and AppDynamics lead the space.\n\n## Comparison Table\n| Vendor | Pricing |\n|---|---|\n| Datadog | Usage-based |\n",
        )
        .unwrap();

        let messages = vec![
            LlmMessage::tool_result(
                "call_1",
                "web_search",
                json!({
                    "status": "success",
                    "result": {"results": [
                        {"title": "Datadog pricing", "url": "https://www.datadoghq.com/pricing/", "snippet": "Usage-based pricing"},
                        {"title": "Dynatrace pricing", "url": "https://www.dynatrace.com/pricing/", "snippet": "Platform subscription"},
                        {"title": "New Relic pricing", "url": "https://newrelic.com/pricing", "snippet": "Pay for what you use"}
                    ]}
                }),
            ),
            LlmMessage::tool_result(
                "call_2",
                "file_write",
                json!({"success": true, "path": output.to_string_lossy()}),
            ),
        ];

        let completed = completed_file_management_targets(
            "Create a competitive landscape analysis for the enterprise observability and APM market segment. If you have access to web search tools, use them to gather the most current information. Save your findings to market_research.md.",
            dir.path(),
            &messages,
            false,
        );

        assert_eq!(completed, vec!["market_research.md".to_string()]);
    }

    #[test]
    fn invalid_file_management_targets_flags_current_research_with_month_only_dates() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("events.md");
        std::fs::write(
            &output,
            "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| CES 2026 | January 6-9, 2026 | Las Vegas, Nevada, USA | https://www.ces.tech/ |\n| SXSW 2026 | March 12-18, 2026 | Austin, Texas, USA | https://schedule.sxsw.com/?year=2026 |\n| Google Cloud Next 2026 | April 22-24, 2026 | Las Vegas, Nevada, USA | https://www.googlecloudevents.com/next-vegas |\n| Data + AI Summit 2026 | June 8-11, 2026 | San Francisco, California, USA | https://dataaisummit.databricks.com/flow/db/dais2026/landing/page/home |\n| WeAreDevelopers World Congress: North America 2026 | September 2026 | San Jose, California, USA | https://www.wearedevelopers.com/world-congress-north-america |\n",
        )
        .unwrap();

        let invalid = invalid_file_management_targets(
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.",
            dir.path(),
            &[
                LlmMessage::tool_result(
                    "call_1",
                    "web_search",
                    json!({
                        "status": "success",
                        "query": "upcoming tech conferences 2026",
                        "result": {
                            "results": [
                                {"title": "CES 2026 | January 6-9, 2026", "url": "https://www.ces.tech/", "snippet": ""},
                                {"title": "SXSW Conferences & Festivals | March 12-18, 2026", "url": "https://schedule.sxsw.com/?year=2026", "snippet": ""},
                                {"title": "Google Cloud Next 2026 | April 22-24, 2026", "url": "https://www.googlecloudevents.com/next-vegas", "snippet": ""},
                                {"title": "Data + AI Summit 2026 | June 8-11, 2026", "url": "https://dataaisummit.databricks.com/flow/db/dais2026/landing/page/home", "snippet": ""},
                                {"title": "WWDC 2026 | June 8-12, 2026", "url": "https://developer.apple.com/wwdc26/", "snippet": ""}
                            ]
                        }
                    }),
                ),
                LlmMessage::tool_result(
                    "call_2",
                    "read_file",
                    json!({
                        "success": true,
                        "operation": "read",
                        "path": "official_events_excerpt.html",
                        "content": "CES 2026 January 6-9, 2026 Las Vegas Nevada USA\nSXSW 2026 March 12-18, 2026 Austin Texas USA\nGoogle Cloud Next 2026 April 22-24, 2026 Las Vegas Nevada USA"
                    }),
                ),
                LlmMessage::tool_result(
                    "call_3",
                    "file_write",
                    json!({"success": true, "path": output.to_string_lossy()}),
                ),
            ],
        );

        assert_eq!(invalid, vec!["events.md".to_string()]);
        assert!(output_lacks_current_research_details(
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.",
            &std::fs::read_to_string(&output).unwrap()
        ));
    }

    #[test]
    fn invalid_file_management_targets_flags_low_diversity_current_research_sources() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("events.md");
        std::fs::write(
            &output,
            "# Upcoming Tech Conferences\n\n1. Open Source in Finance Forum Toronto - https://events.linuxfoundation.org/open-source-finance-forum-toronto/\n2. OpenSearchCon Europe - https://events.linuxfoundation.org/opensearchcon-europe/\n3. MCP Dev Summit North America - https://events.linuxfoundation.org/mcp-dev-summit-north-america/\n4. Open Source Summit Europe - https://events.linuxfoundation.org/open-source-summit-europe/\n5. KubeCon + CloudNativeCon North America - https://events.linuxfoundation.org/kubecon-cloudnativecon-north-america/\n",
        )
        .unwrap();

        let invalid = invalid_file_management_targets(
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.",
            dir.path(),
            &[
                LlmMessage::tool_result(
                    "call_1",
                    "web_search",
                    json!({
                        "status": "success",
                        "query": "upcoming tech conferences 2026",
                        "result": {
                            "results": [
                                {"title": "Open Source in Finance Forum Toronto | April 14, 2026", "url": "https://events.linuxfoundation.org/open-source-finance-forum-toronto/", "snippet": ""},
                                {"title": "OpenSearchCon Europe | April 16-17, 2026", "url": "https://events.linuxfoundation.org/opensearchcon-europe/", "snippet": ""},
                                {"title": "MCP Dev Summit North America | April 2-3, 2026", "url": "https://events.linuxfoundation.org/mcp-dev-summit-north-america/", "snippet": ""},
                                {"title": "Open Source Summit Europe | October 7-9, 2026", "url": "https://events.linuxfoundation.org/open-source-summit-europe/", "snippet": ""},
                                {"title": "KubeCon + CloudNativeCon North America | November 9-12, 2026", "url": "https://events.linuxfoundation.org/kubecon-cloudnativecon-north-america/", "snippet": ""}
                            ]
                        }
                    }),
                ),
                LlmMessage::tool_result(
                    "call_2",
                    "read_file",
                    json!({
                        "success": true,
                        "operation": "read",
                        "path": "official_events_excerpt.html",
                        "content": "CES 2026 January 6-9, 2026 Las Vegas Nevada USA https://www.ces.tech/\nSXSW 2026 March 12-18, 2026 Austin Texas USA https://schedule.sxsw.com/?year=2026\nGoogle Cloud Next 2026 April 22-24, 2026 Las Vegas Nevada USA https://www.googlecloudevents.com/next-vegas\nData + AI Summit 2026 June 8-11, 2026 San Francisco California USA https://dataaisummit.databricks.com/flow/db/dais2026/landing/page/home\nWWDC 2026 June 8-12, 2026 Cupertino California USA https://developer.apple.com/wwdc26/"
                    }),
                ),
                LlmMessage::tool_result(
                    "call_3",
                    "file_write",
                    json!({"success": true, "path": output.to_string_lossy()}),
                ),
            ],
        );

        assert_eq!(invalid, vec!["events.md".to_string()]);
        assert!(output_lacks_current_research_details(
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.",
            &std::fs::read_to_string(&output).unwrap()
        ));
    }

    #[test]
    fn invalid_file_management_targets_accepts_current_research_with_specific_dates() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("events.md");
        std::fs::write(
            &output,
            "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| CES 2026 | January 6-9, 2026 | Las Vegas, Nevada, USA | https://www.ces.tech/ |\n| SXSW 2026 | March 12-18, 2026 | Austin, Texas, USA | https://schedule.sxsw.com/?year=2026 |\n| Google Cloud Next 2026 | April 22-24, 2026 | Las Vegas, Nevada, USA | https://www.googlecloudevents.com/next-vegas |\n| Data + AI Summit 2026 | June 8-11, 2026 | San Francisco, California, USA | https://dataaisummit.databricks.com/flow/db/dais2026/landing/page/home |\n| WWDC 2026 | June 8-12, 2026 | Cupertino, California, USA | https://developer.apple.com/wwdc26/ |\n",
        )
        .unwrap();

        let invalid = invalid_file_management_targets(
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.",
            dir.path(),
            &[
                LlmMessage::tool_result(
                    "call_1",
                    "web_search",
                    json!({
                        "status": "success",
                        "query": "upcoming tech conferences 2026",
                        "result": {
                            "results": [
                                {"title": "CES 2026 | January 6-9, 2026", "url": "https://www.ces.tech/", "snippet": ""},
                                {"title": "SXSW Conferences & Festivals | March 12-18, 2026", "url": "https://schedule.sxsw.com/?year=2026", "snippet": ""},
                                {"title": "Google Cloud Next 2026 | April 22-24, 2026", "url": "https://www.googlecloudevents.com/next-vegas", "snippet": ""},
                                {"title": "Data + AI Summit 2026 | June 8-11, 2026", "url": "https://dataaisummit.databricks.com/flow/db/dais2026/landing/page/home", "snippet": ""},
                                {"title": "WWDC 2026 | June 8-12, 2026", "url": "https://developer.apple.com/wwdc26/", "snippet": ""}
                            ]
                        }
                    }),
                ),
                LlmMessage::tool_result(
                    "call_2",
                    "read_file",
                    json!({
                        "success": true,
                        "operation": "read",
                        "path": "official_events_excerpt.html",
                        "content": "CES 2026 January 6-9, 2026 Las Vegas Nevada USA https://www.ces.tech/\nSXSW 2026 March 12-18, 2026 Austin Texas USA https://schedule.sxsw.com/?year=2026\nGoogle Cloud Next 2026 April 22-24, 2026 Las Vegas Nevada USA https://www.googlecloudevents.com/next-vegas\nData + AI Summit 2026 June 8-11, 2026 San Francisco California USA https://dataaisummit.databricks.com/flow/db/dais2026/landing/page/home\nWWDC 2026 June 8-12, 2026 Cupertino California USA https://developer.apple.com/wwdc26/"
                    }),
                ),
                LlmMessage::tool_result(
                    "call_3",
                    "file_write",
                    json!({"success": true, "path": output.to_string_lossy()}),
                ),
            ],
        );

        assert!(invalid.is_empty());
        assert!(!output_lacks_current_research_details(
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.",
            &std::fs::read_to_string(&output).unwrap()
        ));
    }

    #[test]
    fn invalid_file_management_targets_accepts_three_entry_conference_bullet_list() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("events.md");
        std::fs::write(
            &output,
            "# Upcoming Technology Conferences\n\n- **Microsoft Build 2026**\n  - **Date:** June 2-3, 2026\n  - **Location:** San Francisco, California, USA and online\n  - **Website:** <https://build.microsoft.com/en-US/home>\n\n- **Black Hat USA 2026**\n  - **Date:** August 1-6, 2026\n  - **Location:** Mandalay Bay, Las Vegas, Nevada, USA\n  - **Website:** <https://www.blackhat.com/us-26/registration.html>\n\n- **AWS re:Invent 2026**\n  - **Date:** November 30-December 4, 2026\n  - **Location:** Las Vegas, Nevada, USA\n  - **Website:** <https://aws.amazon.com/events/reinvent/>\n",
        )
        .unwrap();

        let invalid = invalid_file_management_targets(
            "Find 3 upcoming technology conferences and create events.md with name, date, location, and website for each. Save the result as markdown with a heading and either a table or bullet list.",
            dir.path(),
            &[
                LlmMessage::tool_result(
                    "call_1",
                    "web_search",
                    json!({
                        "status": "success",
                        "query": "upcoming technology conferences official",
                        "result": {
                            "results": [
                                {
                                    "title": "Microsoft Build 2026 | June 2-3, 2026",
                                    "url": "https://build.microsoft.com/en-US/home",
                                    "snippet": "San Francisco, California and online"
                                },
                                {
                                    "title": "Black Hat USA 2026 | August 1-6, 2026",
                                    "url": "https://www.blackhat.com/us-26/registration.html",
                                    "snippet": "Mandalay Bay, Las Vegas, Nevada, USA"
                                },
                                {
                                    "title": "AWS re:Invent 2026 | November 30-December 4, 2026",
                                    "url": "https://aws.amazon.com/events/reinvent/",
                                    "snippet": "Las Vegas, Nevada, USA"
                                }
                            ]
                        }
                    }),
                ),
                LlmMessage::tool_result(
                    "call_2",
                    "file_write",
                    json!({"success": true, "path": output.to_string_lossy()}),
                ),
            ],
        );

        assert!(invalid.is_empty());
    }

    #[test]
    fn specific_calendar_date_recognizes_abbreviated_cross_month_ranges() {
        assert!(text_has_specific_calendar_date("Aug. 31–Sep. 3, 2026"));
        assert!(text_has_specific_calendar_date("Nov. 30-Dec. 4, 2026"));
    }

    #[test]
    fn invalid_file_management_targets_accepts_abbreviated_cross_month_conference_dates() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("events.md");
        std::fs::write(
            &output,
            "# Upcoming Technology Conferences\n\n- **Google I/O 2026**\n  - **Date:** May 19–20, 2026\n  - **Location:** Mountain View, California, USA\n  - **Website:** <https://io.google/>\n\n- **Microsoft Build 2026**\n  - **Date:** June 2–3, 2026\n  - **Location:** San Francisco, California, USA and online\n  - **Website:** <https://build.microsoft.com/en-US/home>\n\n- **VMware Explore 2026**\n  - **Date:** Aug. 31–Sep. 3, 2026\n  - **Location:** Las Vegas, Nevada, USA\n  - **Website:** <https://www.vmware.com/explore/us>\n",
        )
        .unwrap();

        let invalid = invalid_file_management_targets(
            "Find 3 upcoming technology conferences and create events.md with name, date, location, and website for each. Save the result as markdown with a heading and either a table or bullet list.",
            dir.path(),
            &[
                LlmMessage::tool_result(
                    "call_1",
                    "web_search",
                    json!({
                        "status": "success",
                        "query": "upcoming technology conferences official",
                        "result": {
                            "results": [
                                {
                                    "title": "Google I/O 2026 | May 19-20, 2026",
                                    "url": "https://io.google/",
                                    "snippet": "Mountain View, California, USA"
                                },
                                {
                                    "title": "Microsoft Build 2026 | June 2-3, 2026",
                                    "url": "https://build.microsoft.com/en-US/home",
                                    "snippet": "San Francisco, California, USA and online"
                                },
                                {
                                    "title": "VMware Explore 2026 | Aug. 31-Sep. 3, 2026",
                                    "url": "https://www.vmware.com/explore/us",
                                    "snippet": "Las Vegas, Nevada, USA"
                                }
                            ]
                        }
                    }),
                ),
                LlmMessage::tool_result(
                    "call_2",
                    "file_write",
                    json!({"success": true, "path": output.to_string_lossy()}),
                ),
            ],
        );

        assert!(invalid.is_empty());
    }

    #[test]
    fn invalid_file_management_targets_rejects_post_write_grounding_for_current_research() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("events.md");
        std::fs::write(
            &output,
            "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| TDX 2026 | March 4-5, 2026 | San Francisco, California, USA | https://www.salesforce.com/tdx/ |\n| WWDC26 | June 8-12, 2026 | Cupertino, California, USA | https://developer.apple.com/wwdc26/ |\n| NVIDIA GTC 2026 | March 16-19, 2026 | San Jose, California, USA | https://www.nvidia.com/gtc/session-catalog/ |\n| WeAreDevelopers World Congress: North America | September 23-25, 2026 | San Jose, California, USA | https://www.wearedevelopers.com/world-congress-north-america |\n| Black Hat USA 2026 | August 1-6, 2026 | Las Vegas, Nevada, USA | https://www.blackhat.com/us-26/ |\n",
        )
        .unwrap();

        let invalid = invalid_file_management_targets(
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.",
            dir.path(),
            &[
                LlmMessage::tool_result(
                    "call_1",
                    "web_search",
                    json!({
                        "status": "success",
                        "query": "official tech conferences 2026",
                        "result": {
                            "results": [
                                {"title": "WWDC26 - Apple Developer", "url": "https://developer.apple.com/wwdc26/", "snippet": ""},
                                {"title": "TDX 2026 - Salesforce", "url": "https://www.salesforce.com/tdx/", "snippet": "Salesforce developer conference"},
                                {"title": "NVIDIA GTC San Jose 2026 Highlights", "url": "https://www.nvidia.com/gtc/", "snippet": "AI conference highlights"},
                                {"title": "WeAreDevelopers World Congress: North America", "url": "https://www.wearedevelopers.com/world-congress-north-america", "snippet": "September 2026 San Jose"},
                                {"title": "Black Hat USA 2026", "url": "https://www.blackhat.com/us-26/", "snippet": ""}
                            ]
                        }
                    }),
                ),
                LlmMessage::tool_result(
                    "call_2",
                    "file_write",
                    json!({"success": true, "path": output.to_string_lossy()}),
                ),
                LlmMessage::tool_result(
                    "call_3",
                    "web_search",
                    json!({
                        "status": "success",
                        "query": "official TDX 2026 dates location",
                        "result": {
                            "results": [
                                {"title": "TDX 2026 - Salesforce", "url": "https://www.salesforce.com/tdx/", "snippet": "March 4-5, 2026 in San Francisco"},
                                {"title": "WWDC26 - Apple Developer", "url": "https://developer.apple.com/wwdc26/", "snippet": "June 8-12, 2026 Cupertino, California"},
                                {"title": "NVIDIA GTC Session Catalog", "url": "https://www.nvidia.com/gtc/session-catalog/", "snippet": "March 16-19, 2026 San Jose, California"},
                                {"title": "WeAreDevelopers World Congress: North America", "url": "https://www.wearedevelopers.com/world-congress-north-america", "snippet": "September 23-25, 2026 San Jose, California"},
                                {"title": "Black Hat USA 2026 Travel", "url": "https://www.blackhat.com/us-26/travel.html", "snippet": "Mandalay Bay Convention Center, Las Vegas"}
                            ]
                        }
                    }),
                ),
            ],
        );

        assert_eq!(invalid, vec!["events.md".to_string()]);
    }

    #[test]
    fn tool_results_lack_current_research_grounding_flags_ungrounded_dates() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "web_search",
            json!({
                "status": "success",
                "query": "upcoming tech conferences 2026",
                "result": {
                    "results": [
                        {"title": "CES - The Most Powerful Tech Event in the World", "url": "https://www.ces.tech/", "snippet": ""},
                        {"title": "MWC Barcelona | MWC Barcelona", "url": "https://www.mwcbarcelona.com/mymwc/schedule/", "snippet": ""},
                        {"title": "Google Cloud Next 2026 - Las Vegas Conference", "url": "https://www.googlecloudevents.com/next-vegas", "snippet": ""},
                        {"title": "Best of NVIDIA GTC: AI Breakthroughs and Recommended Sessions", "url": "https://www.nvidia.com/gtc/", "snippet": ""},
                        {"title": "SXSW Conferences & Festivals | March 12-18, 2026", "url": "https://www.sxsw.com/", "snippet": ""}
                    ]
                }
            }),
        )];

        assert!(tool_results_lack_current_research_grounding(prompt, &messages));
    }

    #[test]
    fn tool_results_lack_current_research_grounding_accepts_grounded_dates() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "web_search",
            json!({
                "status": "success",
                "query": "upcoming tech conferences 2026",
                "result": {
                    "results": [
                        {"title": "CES 2026 | January 6-9, 2026", "url": "https://www.ces.tech/", "snippet": ""},
                        {"title": "MWC Barcelona | March 2-5, 2026", "url": "https://www.mwcbarcelona.com/mymwc/schedule/", "snippet": ""},
                        {"title": "SXSW Conferences & Festivals | March 12-18, 2026", "url": "https://www.sxsw.com/", "snippet": ""},
                        {"title": "NVIDIA GTC 2026 | March 16-19, 2026", "url": "https://www.nvidia.com/gtc/", "snippet": ""},
                        {"title": "Google Cloud Next 2026 | April 22-24, 2026", "url": "https://www.googlecloudevents.com/next-vegas", "snippet": ""}
                    ]
                }
            }),
        )];

        assert!(!tool_results_lack_current_research_grounding(prompt, &messages));
    }

    #[test]
    fn tool_results_lack_current_research_diversity_flags_single_host_results() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "web_search",
            json!({
                "status": "success",
                "query": "upcoming tech conferences 2026",
                "result": {
                    "results": [
                        {"title": "Open Source in Finance Forum Toronto | April 14, 2026", "url": "https://events.linuxfoundation.org/open-source-finance-forum-toronto/", "snippet": ""},
                        {"title": "OpenSearchCon Europe | April 16-17, 2026", "url": "https://events.linuxfoundation.org/opensearchcon-europe/", "snippet": ""},
                        {"title": "MCP Dev Summit North America | April 2-3, 2026", "url": "https://events.linuxfoundation.org/mcp-dev-summit-north-america/", "snippet": ""},
                        {"title": "Open Source Summit Europe | October 7-9, 2026", "url": "https://events.linuxfoundation.org/open-source-summit-europe/", "snippet": ""},
                        {"title": "KubeCon + CloudNativeCon North America | November 9-12, 2026", "url": "https://events.linuxfoundation.org/kubecon-cloudnativecon-north-america/", "snippet": ""}
                    ]
                }
            }),
        )];

        assert!(tool_results_lack_current_research_diversity(prompt, &messages));
    }

    #[test]
    fn tool_results_lack_current_research_diversity_accepts_mixed_hosts() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "web_search",
            json!({
                "status": "success",
                "query": "upcoming tech conferences 2026",
                "result": {
                    "results": [
                        {"title": "CES 2026", "url": "https://www.ces.tech/", "snippet": "Major consumer tech event"},
                        {"title": "SXSW Conferences & Festivals", "url": "https://schedule.sxsw.com/?year=2026", "snippet": "Austin conference schedule"},
                        {"title": "Google Cloud Next 2026", "url": "https://www.googlecloudevents.com/next-vegas", "snippet": "Cloud conference landing page"},
                        {"title": "Data + AI Summit 2026", "url": "https://dataaisummit.databricks.com/flow/db/dais2026/landing/page/home", "snippet": "Databricks conference home"},
                        {"title": "WWDC 2026", "url": "https://developer.apple.com/wwdc26/", "snippet": "Apple developer conference"}
                    ]
                }
            }),
        )];

        assert!(!tool_results_lack_current_research_diversity(prompt, &messages));
    }

    #[test]
    fn output_lacks_markdown_research_structure_flags_raw_json_payload() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let content = r#"[
  {"name":"CES 2026","date":"2026-01-06","location":"Las Vegas","website":"https://www.ces.tech/"}
]"#;

        assert!(output_lacks_markdown_research_structure(prompt, content));
    }

    #[test]
    fn output_lacks_markdown_research_structure_accepts_markdown_table() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let content = "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| CES 2026 | January 6-9, 2026 | Las Vegas, Nevada, USA | https://www.ces.tech/ |\n| SXSW 2026 | March 12-18, 2026 | Austin, Texas, USA | https://schedule.sxsw.com/?year=2026 |\n| Google Cloud Next 2026 | April 22-24, 2026 | Las Vegas, Nevada, USA | https://www.googlecloudevents.com/next-vegas |\n| Data + AI Summit 2026 | June 8-11, 2026 | San Francisco, California, USA | https://dataaisummit.databricks.com/flow/db/dais2026/landing/page/home |\n| WWDC 2026 | June 8-12, 2026 | Cupertino, California, USA | https://developer.apple.com/wwdc26/ |\n";

        assert!(!output_lacks_markdown_research_structure(prompt, content));
    }

    #[test]
    fn output_lacks_markdown_research_structure_flags_trailing_process_note() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let content = "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| CES 2026 | January 6-9, 2026 | Las Vegas, Nevada, USA | https://www.ces.tech/ |\n| SXSW 2026 | March 12-18, 2026 | Austin, Texas, USA | https://schedule.sxsw.com/?year=2026 |\n| Google Cloud Next 2026 | April 22-24, 2026 | Las Vegas, Nevada, USA | https://www.googlecloudevents.com/next-vegas |\n| Data + AI Summit 2026 | June 8-11, 2026 | San Francisco, California, USA | https://dataaisummit.databricks.com/flow/db/dais2026/landing/page/home |\n| WWDC 2026 | June 8-12, 2026 | Cupertino, California, USA | https://developer.apple.com/wwdc26/ |\n\nAll entries above were verified against official event pages downloaded during research.\n";

        assert!(output_lacks_markdown_research_structure(prompt, content));
    }

    #[test]
    fn output_lacks_longform_markdown_structure_flags_missing_sections() {
        let prompt =
            "Write a 500-word blog post about the benefits of remote work for software developers. Save it to blog_post.md.";
        let content = "# Remote Work for Developers\n\nRemote work is useful.\n\nIt improves focus and flexibility.\n";
        assert!(output_lacks_longform_markdown_structure(prompt, content));
    }

    #[test]
    fn output_lacks_longform_markdown_structure_accepts_structured_blog() {
        let prompt =
            "Write a 500-word blog post about the benefits of remote work for software developers. Save it to blog_post.md.";
        let content = "# Remote Work for Developers\n\n## Focus\n\nDevelopers benefit from uninterrupted deep work.\n\n## Flexibility\n\nFlexible schedules help teams align complex work with their peak energy.\n\n## Career Growth\n\nRemote hiring expands access to stronger teams and projects.\n\n## Conclusion\n\nRemote work improves focus, flexibility, and opportunity when teams communicate well.\n";
        assert!(!output_lacks_longform_markdown_structure(prompt, content));
    }

    #[test]
    fn output_violates_requested_word_budget_flags_oversized_blog_post() {
        let prompt =
            "Write a 500-word blog post about the benefits of remote work for software developers. Save it to blog_post.md.";
        let content = "word ".repeat(650);
        assert!(output_violates_requested_word_budget(prompt, &content));
    }

    #[test]
    fn output_violates_requested_word_budget_accepts_near_target_blog_post() {
        let prompt =
            "Write a 500-word blog post about the benefits of remote work for software developers. Save it to blog_post.md.";
        let content = "word ".repeat(549);
        assert!(!output_violates_requested_word_budget(prompt, &content));
    }

    #[test]
    fn output_violates_requested_word_budget_flags_long_blog_post_past_relaxed_window() {
        let prompt =
            "Write a 500-word blog post about the benefits of remote work for software developers. Save it to blog_post.md.";
        let content = "word ".repeat(563);
        assert!(output_violates_requested_word_budget(prompt, &content));
    }

    #[test]
    fn output_lacks_current_research_details_flags_host_family_overconcentration() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let content = "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| Open Source in Finance Forum Toronto | Apr 14, 2026 | Toronto, Canada | https://events.linuxfoundation.org/open-source-finance-forum-toronto/ |\n| OpenSearchCon Europe | Apr 16-17, 2026 | Prague, Czechia | https://events.linuxfoundation.org/opensearchcon-europe/ |\n| Open Source Summit North America | May 18-20, 2026 | Minneapolis, Minnesota | https://events.linuxfoundation.org/open-source-summit-north-america/ |\n| Microsoft Ignite 2026 | November 17-20, 2026 | San Francisco, California, USA | https://ignite.microsoft.com/en-US/home |\n| Web Summit | November 9-12, 2026 | Lisbon, Portugal | https://websummit.com/ |\n";

        assert!(output_lacks_current_research_details(prompt, content));
    }

    #[test]
    fn output_contains_unsupported_research_branding_flags_host_mismatch() {
        let content = "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| AI Dev 26 x SF | April 28-29, 2026 | San Francisco, California, USA | https://ai-dev.deeplearning.ai/ |\n| NDC Toronto 2026 | May 5-8, 2026 | Toronto, Canada | https://ndctoronto.com/ |\n| Web Summit Vancouver | May 11-14, 2026 | Vancouver, Canada | https://collisionconf.com/ |\n| 76th IEEE Electronic Components and Technology Conference (ECTC) | May 26-29, 2026 | Orlando, Florida, USA | https://ectc.net/ |\n| Fortune Brainstorm Tech 2026 | June 8-10, 2026 | Aspen, Colorado, USA | https://conferences.fortune.com/brainstorm-tech/ |\n";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "web_search",
            json!({
                "status": "success",
                "query": "upcoming tech conferences 2026",
                "result": {
                    "results": [
                        {"title": "AI Dev 26 x SF | April 28-29, 2026", "url": "https://ai-dev.deeplearning.ai/", "snippet": "San Francisco, California, USA"},
                        {"title": "NDC Toronto 2026 | May 5-8, 2026", "url": "https://ndctoronto.com/", "snippet": "Toronto, Canada"},
                        {"title": "Collision 2026 | May 11-14, 2026", "url": "https://collisionconf.com/", "snippet": "Vancouver, Canada"},
                        {"title": "ECTC 2026 | May 26-29, 2026", "url": "https://ectc.net/", "snippet": "Orlando, Florida, USA"},
                        {"title": "Fortune Brainstorm Tech 2026 | June 8-10, 2026", "url": "https://conferences.fortune.com/brainstorm-tech/", "snippet": "Aspen, Colorado, USA"}
                    ]
                }
            }),
        )];

        assert!(output_contains_unsupported_research_branding(
            content,
            &messages
        ));
    }

    #[test]
    fn output_contains_unsupported_research_branding_accepts_supported_acronym_branding() {
        let content = "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| GIDS 2026 | April 21-24, 2026 | Bangalore, India | https://developersummit.com/ |\n| Open Source Summit North America 2026 | May 18-20, 2026 | Minneapolis, Minnesota, USA | https://events.linuxfoundation.org/open-source-summit-north-america/ |\n| WWDC26 | June 8-12, 2026 | Cupertino, California, USA | https://developer.apple.com/wwdc26/ |\n| Web Summit 2026 | November 9-12, 2026 | Lisbon, Portugal | https://websummit.com/web-summit-2026/ |\n| AWS re:Invent 2026 | November 30-December 4, 2026 | Las Vegas, Nevada, USA | https://aws.amazon.com/events/reinvent/ |\n";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "web_search",
            json!({
                "status": "success",
                "query": "site:developersummit.com GIDS 2026 official date location",
                "result": {
                    "results": [
                        {"title": "GIDS 2026 - Asia-Pacific's Biggest Software Developer Summit", "url": "https://developersummit.com/", "snippet": "April 21-24, 2026 Bangalore, India"},
                        {"title": "Open Source Summit North America 2026 | May 18-20, 2026", "url": "https://events.linuxfoundation.org/open-source-summit-north-america/", "snippet": "Minneapolis, Minnesota, USA"},
                        {"title": "WWDC26 | June 8-12, 2026", "url": "https://developer.apple.com/wwdc26/", "snippet": "Cupertino, California, USA"},
                        {"title": "Web Summit 2026 | November 9-12, 2026", "url": "https://websummit.com/web-summit-2026/", "snippet": "Lisbon, Portugal"},
                        {"title": "AWS re:Invent 2026 | November 30-December 4, 2026", "url": "https://aws.amazon.com/events/reinvent/", "snippet": "Las Vegas, Nevada, USA"}
                    ]
                }
            }),
        )];

        assert!(!output_contains_unsupported_research_branding(
            content,
            &messages
        ));
    }

    #[test]
    fn output_lacks_current_research_details_accepts_major_conference_series() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let content = "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| Google Cloud Next 2026 | April 22-24, 2026 | Las Vegas, Nevada, USA | https://www.googlecloudevents.com/next-vegas |\n| NDC Toronto 2026 | May 5-8, 2026 | Toronto, Canada | https://ndctoronto.com/ |\n| Web Summit Vancouver 2026 | May 11-14, 2026 | Vancouver, Canada | https://vancouver.websummit.com/ |\n| 76th IEEE Electronic Components and Technology Conference (ECTC) | May 26-29, 2026 | Orlando, Florida, USA | https://ectc.net/ |\n| Fortune Brainstorm Tech 2026 | June 8-10, 2026 | Aspen, Colorado, USA | https://conferences.fortune.com/brainstorm-tech/ |\n";

        assert!(!output_lacks_current_research_details(prompt, content));
    }

    #[test]
    fn output_lacks_current_research_details_flags_aggregator_host() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let content = "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| VivaTech 2026 | June 17-20, 2026 | Paris, France | https://www.ventureport.com/events/vivatech-2026 |\n| Web Summit 2026 | November 9-12, 2026 | Lisbon, Portugal | https://websummit.com/web-summit-2026/ |\n| GITEX Europe 2026 | June 30-July 1, 2026 | Berlin, Germany | https://www.gitexeurope.com/ |\n| TechCrunch Disrupt 2026 | October 13-15, 2026 | San Francisco, California, USA | https://techcrunch.com/events/tc-disrupt-2026/ |\n| MWC Barcelona 2026 | March 2-5, 2026 | Barcelona, Spain | https://www.mwcbarcelona.com/ |\n";

        assert!(output_lacks_current_research_details(prompt, content));
    }

    #[test]
    fn output_lacks_current_research_details_flags_wrong_year_for_this_year_prompt() {
        let prompt =
            "Find 5 upcoming tech conferences happening this year and create events.md with name, date, location, and website for each.";
        let content = "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| VivaTech 2026 | June 17-20, 2026 | Paris, France | https://vivatechnology.com/ |\n| Web Summit 2026 | November 9-12, 2026 | Lisbon, Portugal | https://websummit.com/web-summit-2026/ |\n| GITEX Europe 2026 | June 30-July 1, 2026 | Berlin, Germany | https://www.gitexeurope.com/ |\n| TechCrunch Disrupt 2026 | October 13-15, 2026 | San Francisco, California, USA | https://techcrunch.com/events/tc-disrupt-2026/ |\n| MWC Barcelona 2027 | March 1-4, 2027 | Barcelona, Spain | https://www.mwcbarcelona.com/about/ |\n";

        assert!(output_lacks_current_research_details(prompt, content));
    }

    #[test]
    fn output_lacks_current_research_details_flags_non_current_year_for_upcoming_roundup() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let content = "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| Google Cloud Next 2026 | April 22-24, 2026 | Las Vegas, Nevada, USA | https://www.googlecloudevents.com/next-vegas |\n| Microsoft Build 2026 | June 2-3, 2026 | San Francisco, California, USA | https://build.microsoft.com/en-US/home |\n| Cisco Live 2026 Amsterdam | February 9-13, 2026 | Amsterdam, Netherlands | https://www.ciscolive.com/ |\n| AWS re:Invent 2026 | November 30-December 4, 2026 | Las Vegas, Nevada, USA | https://aws.amazon.com/events/reinvent/ |\n| KubeCon + CloudNativeCon North America 2025 | November 10-13, 2025 | Atlanta, Georgia, USA | https://events.linuxfoundation.org/kubecon-cloudnativecon-north-america/ |\n";

        assert!(output_lacks_current_research_details(prompt, content));
    }

    #[test]
    fn output_lacks_current_research_details_flags_month_only_event_dates() {
        let prompt = "Find 5 upcoming technology conferences this year and create events.md with name, date, location, and website.";
        let content = "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| WWDC26 | June 2026 | Cupertino, California, USA | https://developer.apple.com/wwdc26/ |\n| Google Cloud Next 2026 | April 22-24, 2026 | Las Vegas, Nevada, USA | https://www.googlecloudevents.com/next-vegas |\n";

        assert!(output_lacks_current_research_details(prompt, content));
    }

    #[test]
    fn output_lacks_current_research_details_flags_low_authority_city_edition() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let content = "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| CES 2026 | January 6-9, 2026 | Las Vegas, Nevada, USA | https://www.ces.tech/ |\n| AI Dev 26 x SF | April 28-29, 2026 | San Francisco, California, USA | https://ai-dev.deeplearning.ai/ |\n| KubeCon + CloudNativeCon Europe 2026 | March 23-26, 2026 | Amsterdam, Netherlands | https://events.linuxfoundation.org/kubecon-cloudnativecon-europe/ |\n| Web Summit 2026 | November 9-12, 2026 | Lisbon, Portugal | https://websummit.com/web-summit-2026/ |\n| AWS re:Invent 2026 | November 30-December 4, 2026 | Las Vegas, Nevada, USA | https://aws.amazon.com/events/reinvent/ |\n";

        assert!(output_lacks_current_research_details(prompt, content));
    }

    #[test]
    fn output_lacks_concise_summary_structure_flags_verbose_summary() {
        let prompt =
            "Read the document in summary_source.txt and write a concise 3-paragraph summary to summary_output.txt.";
        let paragraph = "This paragraph repeats details to stay overly long for a concise summary while still sounding coherent and accurate. ";
        let content = format!(
            "{}\n\n{}\n\n{}",
            paragraph.repeat(18),
            paragraph.repeat(18),
            paragraph.repeat(18)
        );
        assert!(output_lacks_concise_summary_structure(prompt, &content));
    }

    #[test]
    fn output_lacks_concise_summary_structure_accepts_compact_three_paragraph_summary() {
        let prompt =
            "Read the document in summary_source.txt and write a concise 3-paragraph summary to summary_output.txt.";
        let content = "Artificial intelligence is reshaping healthcare by improving diagnosis, treatment planning, and patient care through systems that can analyze large amounts of clinical data more quickly than traditional workflows. The article presents AI as a major shift in medical practice rather than a narrow technical trend.\n\nIts strongest applications include medical imaging, drug discovery, and predictive analytics. In each area, the technology helps clinicians detect disease earlier, identify promising treatments faster, and anticipate patient risks before conditions worsen, which can improve outcomes and reduce costs.\n\nThe article also stresses important challenges, including patient privacy, algorithmic bias, limited transparency, and evolving regulation. Its final point is that AI should support human clinicians, combining machine efficiency with human judgment, empathy, and ethical responsibility.";
        assert!(!output_lacks_concise_summary_structure(prompt, content));
    }

    #[test]
    fn tool_results_lack_direct_current_research_evidence_flags_search_only_roundup() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "web_search",
            json!({
                "status": "success",
                "query": "upcoming tech conferences 2026",
                "result": {
                    "results": [
                        {"title": "CES 2026", "url": "https://www.ces.tech/", "snippet": "Major consumer tech event"},
                        {"title": "SXSW Conferences & Festivals", "url": "https://schedule.sxsw.com/?year=2026", "snippet": "Austin conference schedule"},
                        {"title": "Google Cloud Next 2026", "url": "https://www.googlecloudevents.com/next-vegas", "snippet": "Cloud conference landing page"},
                        {"title": "Data + AI Summit 2026", "url": "https://dataaisummit.databricks.com/flow/db/dais2026/landing/page/home", "snippet": "Databricks conference home"},
                        {"title": "WWDC 2026", "url": "https://developer.apple.com/wwdc26/", "snippet": "Apple developer conference"}
                    ]
                }
            }),
        )];

        assert!(tool_results_lack_direct_current_research_evidence(prompt, &messages));
    }

    #[test]
    fn tool_results_lack_direct_current_research_evidence_accepts_page_reads() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let messages = vec![
            LlmMessage::tool_result(
                "call_1",
                "web_search",
                json!({
                    "status": "success",
                    "query": "upcoming tech conferences 2026",
                    "result": {
                        "results": [
                            {"title": "CES 2026 | January 6-9, 2026", "url": "https://www.ces.tech/", "snippet": ""},
                            {"title": "SXSW Conferences & Festivals | March 12-18, 2026", "url": "https://schedule.sxsw.com/?year=2026", "snippet": ""},
                            {"title": "Google Cloud Next 2026 | April 22-24, 2026", "url": "https://www.googlecloudevents.com/next-vegas", "snippet": ""},
                            {"title": "Data + AI Summit 2026 | June 8-11, 2026", "url": "https://dataaisummit.databricks.com/flow/db/dais2026/landing/page/home", "snippet": ""},
                            {"title": "WWDC 2026 | June 8-12, 2026", "url": "https://developer.apple.com/wwdc26/", "snippet": ""}
                        ]
                    }
                }),
            ),
            LlmMessage::tool_result(
                "call_2",
                "read_file",
                json!({
                    "success": true,
                    "operation": "read",
                    "path": "official_event.html",
                    "content": "startDate 2026-06-08 endDate 2026-06-12 Cupertino"
                }),
            ),
        ];

        assert!(!tool_results_lack_direct_current_research_evidence(prompt, &messages));
    }

    #[test]
    fn tool_results_lack_direct_current_research_evidence_accepts_grounded_official_search_results(
    ) {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "web_search",
            json!({
                "status": "success",
                "query": "upcoming tech conferences 2026",
                "result": {
                    "results": [
                        {"title": "CES 2026 | January 6-9, 2026", "url": "https://www.ces.tech/", "snippet": "Las Vegas, Nevada"},
                        {"title": "MWC Barcelona | March 2-5, 2026", "url": "https://www.mwcbarcelona.com/", "snippet": "Barcelona, Spain"},
                        {"title": "SXSW 2026 | March 12-18, 2026", "url": "https://www.sxsw.com/", "snippet": "Austin, Texas"},
                        {"title": "Google Cloud Next 2026 | April 22-24, 2026", "url": "https://www.googlecloudevents.com/next-vegas", "snippet": "Las Vegas, Nevada"},
                        {"title": "WWDC 2026 | June 8-12, 2026", "url": "https://developer.apple.com/wwdc26/", "snippet": "Cupertino, California"}
                    ]
                }
            }),
        )];

        assert!(!tool_results_lack_direct_current_research_evidence(prompt, &messages));
    }

    #[test]
    fn tool_results_lack_direct_current_research_evidence_rejects_aggregator_search_results() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "web_search",
            json!({
                "status": "success",
                "query": "upcoming tech conferences 2026",
                "result": {
                    "results": [
                        {"title": "VivaTech 2026 | June 17-20, 2026", "url": "https://www.ventureport.com/events/vivatech-2026", "snippet": "Paris, France"},
                        {"title": "GITEX Europe 2026 | June 30-July 1, 2026", "url": "https://expolume.com/expo/gitex-europe/", "snippet": "Berlin, Germany"},
                        {"title": "TechCrunch Disrupt 2026 | October 13-15, 2026", "url": "https://www.vktr.com/events/conference/techcrunch-disrupt-san-francisco-2026/", "snippet": "San Francisco, California"},
                        {"title": "MWC Barcelona 2027 | March 1-4, 2027", "url": "https://www.showsbee.com/fairs/83605-Mobile-World-Congress-2027.html", "snippet": "Barcelona, Spain"},
                        {"title": "CES 2027 | January 6-9, 2027", "url": "https://guidantech.com/ces-2027-confirmed-dates-venue-themes-and-registration-details-announced/", "snippet": "Las Vegas, Nevada"}
                    ]
                }
            }),
        )];

        assert!(tool_results_lack_direct_current_research_evidence(prompt, &messages));
    }

    #[test]
    fn tool_results_lack_direct_current_research_evidence_rejects_low_authority_city_editions() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "web_search",
            json!({
                "status": "success",
                "query": "upcoming tech conferences 2026",
                "result": {
                    "results": [
                        {"title": "CES 2026 | January 6-9, 2026", "url": "https://www.ces.tech/", "snippet": "Las Vegas, Nevada"},
                        {"title": "AI Dev 26 x SF | April 28-29, 2026", "url": "https://ai-dev.deeplearning.ai/", "snippet": "San Francisco, California"},
                        {"title": "KubeCon + CloudNativeCon Europe 2026 | March 23-26, 2026", "url": "https://events.linuxfoundation.org/kubecon-cloudnativecon-europe/", "snippet": "Amsterdam, Netherlands"},
                        {"title": "Web Summit 2026 | November 9-12, 2026", "url": "https://websummit.com/web-summit-2026/", "snippet": "Lisbon, Portugal"},
                        {"title": "AWS re:Invent 2026 | November 30-December 4, 2026", "url": "https://aws.amazon.com/events/reinvent/", "snippet": "Las Vegas, Nevada"}
                    ]
                }
            }),
        )];

        assert!(tool_results_lack_direct_current_research_evidence(prompt, &messages));
    }

    #[test]
    fn tool_results_lack_direct_current_research_evidence_rejects_search_results_without_locations()
    {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "web_search",
            json!({
                "status": "success",
                "query": "upcoming tech conferences 2026",
                "result": {
                    "results": [
                        {"title": "Google Cloud Next 2026 | April 22-24, 2026", "url": "https://www.googlecloudevents.com/next-vegas", "snippet": "Annual cloud conference"},
                        {"title": "AWS re:Invent 2026 | November 30-December 4, 2026", "url": "https://aws.amazon.com/events/reinvent/", "snippet": "Annual AWS event"},
                        {"title": "Web Summit 2026 | November 9-12, 2026", "url": "https://websummit.com/web-summit-2026/", "snippet": "Major technology conference"},
                        {"title": "KubeCon + CloudNativeCon Europe 2026 | March 23-26, 2026", "url": "https://events.linuxfoundation.org/kubecon-cloudnativecon-europe/", "snippet": "Cloud native conference"},
                        {"title": "Data Center World 2026 | April 20-23, 2026", "url": "https://datacenterworld.com/", "snippet": "Infrastructure conference"}
                    ]
                }
            }),
        )];

        assert!(tool_results_lack_direct_current_research_evidence(prompt, &messages));
    }

    #[test]
    fn output_entries_lack_direct_research_support_flags_virtual_url_with_physical_location() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let content = "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| DeveloperWeek 2026 | February 18-20, 2026 | San Jose, CA, USA | https://www.developerweek.com/virtual/ |\n| WWDC26 | June 8-12, 2026 | Cupertino, CA, USA | https://developer.apple.com/wwdc26/ |\n| AWS re:Invent 2026 | November 30-December 4, 2026 | Las Vegas, NV, USA | https://aws.amazon.com/events/reinvent/ |\n| Web Summit 2026 | November 9-12, 2026 | Lisbon, Portugal | https://websummit.com/web-summit-2026/ |\n| KubeCon + CloudNativeCon Europe 2026 | March 23-26, 2026 | Amsterdam, Netherlands | https://events.linuxfoundation.org/kubecon-cloudnativecon-europe/ |\n";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "web_search",
            json!({
                "status": "success",
                "query": "upcoming tech conferences 2026",
                "result": {
                    "results": [
                        {"title": "DeveloperWeek Global (Virtual) | March 4, 2026", "url": "https://www.developerweek.com/virtual/", "snippet": "Virtual"},
                        {"title": "WWDC26 | June 8-12, 2026", "url": "https://developer.apple.com/wwdc26/", "snippet": "Cupertino, California"},
                        {"title": "AWS re:Invent 2026 | November 30-December 4, 2026", "url": "https://aws.amazon.com/events/reinvent/", "snippet": "Las Vegas, Nevada"},
                        {"title": "Web Summit 2026 | November 9-12, 2026", "url": "https://websummit.com/web-summit-2026/", "snippet": "Lisbon, Portugal"},
                        {"title": "KubeCon + CloudNativeCon Europe 2026 | March 23-26, 2026", "url": "https://events.linuxfoundation.org/kubecon-cloudnativecon-europe/", "snippet": "Amsterdam, Netherlands"}
                    ]
                }
            }),
        )];

        assert!(output_entries_lack_direct_research_support(prompt, content, &messages));
    }

    #[test]
    fn output_entries_lack_direct_research_support_flags_entry_without_host_aligned_dates() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let content = "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| CES 2026 | January 6-9, 2026 | Las Vegas, Nevada, USA | https://www.ces.tech/ |\n| WWDC26 | June 8-12, 2026 | Cupertino, California, USA | https://developer.apple.com/wwdc26/ |\n| AWS re:Invent 2026 | November 30-December 4, 2026 | Las Vegas, Nevada, USA | https://aws.amazon.com/events/reinvent/ |\n| WeAreDevelopers World Congress: North America | September 23-25, 2026 | San Jose, California, USA | https://www.wearedevelopers.com/world-congress-north-america |\n| Black Hat USA 2026 | August 1, 2026 | Las Vegas, Nevada, USA | https://www.blackhat.com/us-26/ |\n";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "web_search",
            json!({
                "status": "success",
                "query": "upcoming tech conferences 2026",
                "result": {
                    "results": [
                        {"title": "CES 2026 | January 6-9, 2026", "url": "https://www.ces.tech/", "snippet": "Las Vegas, Nevada"},
                        {"title": "WWDC26 | June 8-12, 2026", "url": "https://developer.apple.com/wwdc26/", "snippet": "Cupertino, California"},
                        {"title": "AWS re:Invent 2026 | November 30-December 4, 2026", "url": "https://aws.amazon.com/events/reinvent/", "snippet": "Las Vegas, Nevada"},
                        {"title": "WeAreDevelopers World Congress: North America | September 23-25, 2026", "url": "https://www.wearedevelopers.com/world-congress-north-america", "snippet": "San Jose, California"},
                        {"title": "Black Hat USA 2026", "url": "https://www.blackhat.com/us-26/", "snippet": "Official event page"}
                    ]
                }
            }),
        )];

        assert!(output_entries_lack_direct_research_support(prompt, content, &messages));
    }

    #[test]
    fn output_entries_lack_direct_research_support_flags_mismatched_entry_date_tokens() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let content = "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| Google Cloud Next 2026 | April 22-24, 2026 | Las Vegas, Nevada, USA | https://www.googlecloudevents.com/next-vegas |\n| Adobe Summit 2026 | April 19-22, 2026 | Las Vegas, Nevada, USA + Online | https://summit.adobe.com/na/ |\n| Cisco Live 2026 | May 31-June 4, 2026 | Las Vegas, Nevada, USA | https://www.ciscolive.com/global/attend.html |\n| Databricks Data + AI Summit 2026 | June 15-18, 2026 | San Francisco, California, USA + Virtual | https://www.databricks.com/dataaisummit |\n| AWS re:Invent 2026 | November 30-December 4, 2026 | Las Vegas, Nevada, USA | https://aws.amazon.com/events/reinvent/ |\n";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "web_search",
            json!({
                "status": "success",
                "query": "official Google Cloud Next 2026 Las Vegas date location",
                "result": {
                    "results": [
                        {"title": "Google Cloud Next 2026 - Las Vegas Conference", "url": "https://www.googlecloudevents.com/next-vegas", "snippet": "Join us April 23-25, 2026 in Las Vegas."},
                        {"title": "Adobe Summit - The Customer Experience Conference | April 19-22, 2026", "url": "https://summit.adobe.com/na/", "snippet": "Attend in person in Las Vegas or join us online."},
                        {"title": "Attend - Cisco Live 2026 Las Vegas", "url": "https://www.ciscolive.com/global/attend.html", "snippet": "Join us in Las Vegas from May 31 - June 4."},
                        {"title": "Data + AI Summit 2026", "url": "https://www.databricks.com/dataaisummit", "snippet": "June 15-18, 2026 San Francisco, California"},
                        {"title": "AWS re:Invent 2026 | Nov 30-Dec 4, 2026", "url": "https://aws.amazon.com/events/reinvent/", "snippet": "Nov 30-Dec 4, 2026 in Las Vegas, NV."}
                    ]
                }
            }),
        )];

        assert!(output_entries_lack_direct_research_support(prompt, content, &messages));
    }

    #[test]
    fn output_entries_lack_direct_research_support_rejects_third_party_search_match_for_official_url()
    {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let content = "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| Google Cloud Next 2026 | April 22-24, 2026 | Las Vegas, Nevada, USA | https://www.googlecloudevents.com/next-vegas |\n| Adobe Summit 2026 | April 19-22, 2026 | Las Vegas, Nevada, USA + Online | https://summit.adobe.com/na/ |\n| Cisco Live 2026 | May 31-June 4, 2026 | Las Vegas, Nevada, USA | https://www.ciscolive.com/global/attend.html |\n| Databricks Data + AI Summit 2026 | June 15-18, 2026 | San Francisco, California, USA + Virtual | https://www.databricks.com/dataaisummit |\n| AWS re:Invent 2026 | November 30-December 4, 2026 | Las Vegas, Nevada, USA | https://aws.amazon.com/events/reinvent/ |\n";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "web_search",
            json!({
                "status": "success",
                "query": "official Google Cloud Next 2026 Las Vegas date location",
                "result": {
                    "results": [
                        {"title": "Google Cloud Next Las Vegas 2026 - reworked.co", "url": "https://www.reworked.co/events/conference/google-cloud-next-las-vegas-2026/", "snippet": "Conference Date Apr 22, 2026 – Apr 24, 2026. Location Las Vegas."},
                        {"title": "Google Cloud Next 2026 - Las Vegas Conference", "url": "https://www.googlecloudevents.com/next-vegas", "snippet": "Join us for Google Cloud Next 2026 in Las Vegas."},
                        {"title": "Adobe Summit - The Customer Experience Conference | April 19-22, 2026", "url": "https://summit.adobe.com/na/", "snippet": "Attend in person in Las Vegas or join us online."},
                        {"title": "Attend - Cisco Live 2026 Las Vegas", "url": "https://www.ciscolive.com/global/attend.html", "snippet": "Join us in Las Vegas from May 31 - June 4."},
                        {"title": "AWS re:Invent 2026 | Nov 30-Dec 4, 2026", "url": "https://aws.amazon.com/events/reinvent/", "snippet": "Nov 30-Dec 4, 2026 in Las Vegas, NV."}
                    ]
                }
            }),
        )];

        assert!(output_entries_lack_direct_research_support(prompt, content, &messages));
    }

    #[test]
    fn should_force_current_research_synthesis_when_grounding_is_sufficient() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let messages = vec![
            LlmMessage::tool_result(
                "call_1",
                "web_search",
                json!({
                    "status": "success",
                    "query": "upcoming tech conferences 2026",
                    "result": {
                        "results": [
                            {"title": "CES 2026 | January 6-9, 2026", "url": "https://www.ces.tech/", "snippet": ""},
                            {"title": "MWC Barcelona | March 2-5, 2026", "url": "https://www.mwcbarcelona.com/mymwc/schedule/", "snippet": ""},
                            {"title": "SXSW Conferences & Festivals | March 12-18, 2026", "url": "https://www.sxsw.com/", "snippet": ""},
                            {"title": "NVIDIA GTC 2026 | March 16-19, 2026", "url": "https://www.nvidia.com/gtc/", "snippet": ""},
                            {"title": "Google Cloud Next 2026 | April 22-24, 2026", "url": "https://www.googlecloudevents.com/next-vegas", "snippet": ""}
                        ]
                    }
                }),
            ),
            LlmMessage::tool_result(
                "call_2",
                "read_file",
                json!({
                    "success": true,
                    "operation": "read",
                    "path": "grounded_events_excerpt.html",
                    "content": "CES 2026 January 6-9, 2026 Las Vegas\nMWC Barcelona March 2-5, 2026 Barcelona\nSXSW March 12-18, 2026 Austin"
                }),
            ),
        ];

        assert!(should_force_current_research_synthesis(
            prompt, Path::new("."), &messages, true, false, 2, 5
        ));
    }

    #[test]
    fn should_force_current_research_synthesis_after_first_grounded_search_round() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "web_search",
            json!({
                "status": "success",
                "query": "upcoming tech conferences 2026",
                "result": {
                    "results": [
                        {"title": "CES 2026 | January 6-9, 2026", "url": "https://www.ces.tech/", "snippet": "Las Vegas, Nevada"},
                        {"title": "MWC Barcelona | March 2-5, 2026", "url": "https://www.mwcbarcelona.com/", "snippet": "Barcelona, Spain"},
                        {"title": "SXSW 2026 | March 12-18, 2026", "url": "https://www.sxsw.com/", "snippet": "Austin, Texas"},
                        {"title": "Google Cloud Next 2026 | April 22-24, 2026", "url": "https://www.googlecloudevents.com/next-vegas", "snippet": "Las Vegas, Nevada"},
                        {"title": "WWDC 2026 | June 8-12, 2026", "url": "https://developer.apple.com/wwdc26/", "snippet": "Cupertino, California"}
                    ]
                }
            }),
        )];

        assert!(should_force_current_research_synthesis(
            prompt, Path::new("."), &messages, true, false, 1, 1
        ));
    }

    #[test]
    fn should_not_force_current_research_synthesis_after_file_write() {
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let messages = vec![
            LlmMessage::tool_result(
                "call_1",
                "web_search",
                json!({
                    "status": "success",
                    "query": "upcoming tech conferences 2026",
                    "result": {
                        "results": [
                            {"title": "CES 2026 | January 6-9, 2026", "url": "https://www.ces.tech/", "snippet": ""},
                            {"title": "MWC Barcelona | March 2-5, 2026", "url": "https://www.mwcbarcelona.com/mymwc/schedule/", "snippet": ""},
                            {"title": "SXSW Conferences & Festivals | March 12-18, 2026", "url": "https://www.sxsw.com/", "snippet": ""},
                            {"title": "NVIDIA GTC 2026 | March 16-19, 2026", "url": "https://www.nvidia.com/gtc/", "snippet": ""},
                            {"title": "Google Cloud Next 2026 | April 22-24, 2026", "url": "https://www.googlecloudevents.com/next-vegas", "snippet": ""}
                        ]
                    }
                }),
            ),
            LlmMessage::tool_result(
                "call_2",
                "file_write",
                json!({"success": true, "path": "events.md"}),
            ),
        ];

        assert!(!should_force_current_research_synthesis(
            prompt, Path::new("."), &messages, true, false, 3, 6
        ));
    }

    #[test]
    fn should_force_current_research_synthesis_for_invalid_written_output_once_evidence_is_ready() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("events.md");
        std::fs::write(
            &output,
            "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| CES 2026 | January 6-9, 2026 | Las Vegas, Nevada, USA | https://www.ces.tech/ |\n| SXSW 2026 | March 2026 | Austin, Texas, USA | https://www.sxsw.com/ |\n| Google Cloud Next 2026 | April 22-24, 2026 | Las Vegas, Nevada, USA | https://www.googlecloudevents.com/next-vegas |\n| Data + AI Summit 2026 | June 8-11, 2026 | San Francisco, California, USA | https://dataaisummit.databricks.com/flow/db/dais2026/landing/page/home |\n| WWDC 2026 | June 8-12, 2026 | Cupertino, California, USA | https://developer.apple.com/wwdc26/ |\n",
        )
        .unwrap();
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let messages = vec![
            LlmMessage::tool_result(
                "call_1",
                "file_write",
                json!({"success": true, "path": output.to_string_lossy()}),
            ),
            LlmMessage::tool_result(
                "call_2",
                "web_search",
                json!({
                    "status": "success",
                    "query": "official tech conferences 2026 dates locations",
                    "result": {
                        "results": [
                            {"title": "CES 2026 | January 6-9, 2026", "url": "https://www.ces.tech/", "snippet": "Las Vegas, Nevada"},
                            {"title": "SXSW 2026 | March 12-18, 2026", "url": "https://www.sxsw.com/", "snippet": "Austin, Texas"},
                            {"title": "Google Cloud Next 2026 | April 22-24, 2026", "url": "https://www.googlecloudevents.com/next-vegas", "snippet": "Las Vegas, Nevada"},
                            {"title": "Data + AI Summit 2026 | June 8-11, 2026", "url": "https://dataaisummit.databricks.com/flow/db/dais2026/landing/page/home", "snippet": "San Francisco, California"},
                            {"title": "WWDC 2026 | June 8-12, 2026", "url": "https://developer.apple.com/wwdc26/", "snippet": "Cupertino, California"}
                        ]
                    }
                }),
            ),
        ];

        assert!(should_force_current_research_synthesis(
            prompt,
            dir.path(),
            &messages,
            true,
            false,
            2,
            2,
        ));
    }

    #[test]
    fn completed_current_research_file_targets_returns_valid_written_output() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("events.md");
        std::fs::write(
            &output,
            "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| CES 2026 | January 6-9, 2026 | Las Vegas, Nevada, USA | https://www.ces.tech/ |\n| MWC Barcelona 2026 | March 2-5, 2026 | Barcelona, Spain | https://www.mwcbarcelona.com/ |\n| Google Cloud Next 2026 | April 22-24, 2026 | Las Vegas, Nevada, USA | https://www.googlecloudevents.com/next-vegas |\n| Data + AI Summit 2026 | June 8-11, 2026 | San Francisco, California, USA | https://dataaisummit.databricks.com/flow/db/dais2026/landing/page/home |\n| Web Summit 2026 | November 9-12, 2026 | Lisbon, Portugal | https://websummit.com/web-summit-2026/ |\n",
        )
        .unwrap();
        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let messages = vec![
            LlmMessage::tool_result(
                "call_1",
                "web_search",
                json!({
                    "status": "success",
                    "query": "upcoming tech conferences 2026",
                    "result": {
                        "results": [
                            {"title": "CES 2026 | January 6-9, 2026", "url": "https://www.ces.tech/", "snippet": "Las Vegas, Nevada"},
                            {"title": "MWC Barcelona | March 2-5, 2026", "url": "https://www.mwcbarcelona.com/", "snippet": "Barcelona, Spain"},
                            {"title": "Google Cloud Next 2026 | April 22-24, 2026", "url": "https://www.googlecloudevents.com/next-vegas", "snippet": "Las Vegas, Nevada"},
                            {"title": "Data + AI Summit 2026 | June 8-11, 2026", "url": "https://dataaisummit.databricks.com/flow/db/dais2026/landing/page/home", "snippet": "San Francisco, California"},
                            {"title": "Web Summit 2026 | November 9-12, 2026", "url": "https://websummit.com/web-summit-2026/", "snippet": "Lisbon, Portugal"}
                        ]
                    }
                }),
            ),
            LlmMessage::tool_result(
                "call_2",
                "file_write",
                json!({"success": true, "path": output.to_string_lossy()}),
            ),
        ];

        assert_eq!(
            completed_current_research_file_targets(prompt, dir.path(), &messages, false),
            vec!["events.md".to_string()]
        );
    }

    #[test]
    fn pending_current_research_rewrite_details_flags_invalid_written_roundup() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("events.md");
        std::fs::write(
            &output,
            "# Upcoming Tech Conferences\n\n| Name | Date | Location | Website |\n|---|---|---|---|\n| CES 2026 | January 6-9, 2026 | Las Vegas, Nevada, USA | https://www.ces.tech/ |\n| SXSW 2026 | March 2026 | Austin, Texas, USA | https://www.sxsw.com/ |\n| Google Cloud Next 2026 | April 22-24, 2026 | Las Vegas, Nevada, USA | https://www.googlecloudevents.com/next-vegas |\n| Data + AI Summit 2026 | June 8-11, 2026 | San Francisco, California, USA | https://dataaisummit.databricks.com/flow/db/dais2026/landing/page/home |\n| WWDC 2026 | June 8-12, 2026 | Cupertino, California, USA | https://developer.apple.com/wwdc26/ |\n",
        )
        .unwrap();

        let prompt =
            "Find 5 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let messages = vec![
            LlmMessage::tool_result(
                "call_1",
                "web_search",
                json!({
                    "status": "success",
                    "query": "official tech conferences 2026 dates locations",
                    "result": {
                        "results": [
                            {"title": "CES 2026 | January 6-9, 2026", "url": "https://www.ces.tech/", "snippet": "Las Vegas, Nevada"},
                            {"title": "SXSW 2026 | March 12-18, 2026", "url": "https://www.sxsw.com/", "snippet": "Austin, Texas"},
                            {"title": "Google Cloud Next 2026 | April 22-24, 2026", "url": "https://www.googlecloudevents.com/next-vegas", "snippet": "Las Vegas, Nevada"},
                            {"title": "Data + AI Summit 2026 | June 8-11, 2026", "url": "https://dataaisummit.databricks.com/flow/db/dais2026/landing/page/home", "snippet": "San Francisco, California"},
                            {"title": "WWDC 2026 | June 8-12, 2026", "url": "https://developer.apple.com/wwdc26/", "snippet": "Cupertino, California"}
                        ]
                    }
                }),
            ),
            LlmMessage::tool_result(
                "call_2",
                "file_write",
                json!({"success": true, "path": output.to_string_lossy()}),
            ),
        ];

        let details = pending_current_research_rewrite_details(prompt, dir.path(), &messages, false);
        assert_eq!(details.len(), 1);
        assert!(details[0].contains("events.md"));
    }

    #[test]
    fn current_research_output_needs_additional_evidence_flags_search_only_roundup() {
        let prompt =
            "Find 3 upcoming tech conferences and create events.md with name, date, location, and website for each.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "web_search",
            json!({
                "status": "success",
                "query": "upcoming flagship technology conferences official",
                "result": {
                    "results": [
                        {
                            "title": "Microsoft Build, June 2-3, 2026 / San Francisco and online",
                            "url": "https://build.microsoft.com/en-US/home",
                            "snippet": "Go deep on real code and real systems with the teams building and scaling AI at Microsoft Build, June 2-3, 2026, in San Francisco and online."
                        },
                        {
                            "title": "Google Cloud Next 2026 - Las Vegas Conference",
                            "url": "https://www.googlecloudevents.com/next-vegas",
                            "snippet": "Join us for Google Cloud Next 2026 in Las Vegas. Explore the latest in generative AI, infrastructure, and security. Register now for the year's premier cloud event."
                        },
                        {
                            "title": "re:Inforce Home - aws.amazon.com",
                            "url": "https://aws.amazon.com/events/reinforce/",
                            "snippet": "AWS re:Inforce has concluded for 2025, but you can still watch on-demand content from this year's event."
                        }
                    ]
                }
            }),
        )];

        assert!(current_research_output_needs_additional_evidence(
            prompt, &messages
        ));
    }

    #[test]
    fn prompt_requests_gitignore_file_detects_ignore_request() {
        let prompt = "Create a .gitignore ignoring pycache and temp files.";
        assert!(prompt_requests_gitignore_file(prompt));
    }

    #[test]
    fn invalid_file_management_targets_flags_literal_markdown_pycache() {
        let dir = tempdir().unwrap();
        let output = dir.path().join(".gitignore");
        std::fs::write(&output, "**pycache**\n").unwrap();

        let invalid = invalid_file_management_targets(
            "Create a project structure with .gitignore ignoring **pycache**.",
            dir.path(),
            &[LlmMessage::tool_result(
                "call_1",
                "file_write",
                json!({"success": true, "path": output.to_string_lossy()}),
            )],
        );

        assert_eq!(invalid, vec![".gitignore".to_string()]);
        assert!(output_lacks_expected_ignore_patterns(
            "Create a .gitignore ignoring **pycache**.",
            "**pycache**"
        ));
    }

    #[test]
    fn invalid_file_management_targets_flags_fake_png_outputs() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("robot.png");
        std::fs::write(&output, "this is not a png").unwrap();

        let invalid = invalid_file_management_targets(
            "Generate an image and save it to robot.png.",
            dir.path(),
            &[LlmMessage::tool_result(
                "call_1",
                "file_write",
                json!({"success": true, "path": output.to_string_lossy()}),
            )],
        );

        assert_eq!(invalid, vec!["robot.png".to_string()]);
    }

    #[test]
    fn utf8_safe_preview_handles_zero_length() {
        assert_eq!(utf8_safe_preview("hello", 0), "");
    }

    #[test]
    fn normalize_conversation_log_text_preserves_meaningful_line_breaks() {
        let text = "  First line.\n\n   Return only the result.  ";

        let normalized = normalize_conversation_log_text(text);

        assert_eq!(
            normalized.as_deref(),
            Some("First line.\n\nReturn only the result.")
        );
    }

    #[test]
    fn normalize_conversation_log_text_skips_empty_content() {
        assert_eq!(normalize_conversation_log_text(" \n\t "), None);
    }

    #[test]
    fn select_relevant_skills_prefers_matching_entries() {
        let skills = vec![
            TextualSkill {
                file_name: "battery_monitor".into(),
                absolute_path: "/tmp/battery/SKILL.md".into(),
                description: "Inspect battery and power telemetry".into(),
                tags: vec!["battery".into(), "power".into()],
                triggers: vec!["check battery".into()],
                examples: vec!["check battery status".into()],
                openclaw_requires: Vec::new(),
                openclaw_install: Vec::new(),
                prelude_excerpt: String::new(),
                code_fence_languages: Vec::new(),
                shell_prelude: false,
                searchable_text:
                    "battery_monitor inspect battery and power telemetry battery power check battery check battery status inspect device power".into(),
            },
            TextualSkill {
                file_name: "calendar_sync".into(),
                absolute_path: "/tmp/calendar/SKILL.md".into(),
                description: "Handle schedule sync tasks".into(),
                tags: Vec::new(),
                triggers: Vec::new(),
                examples: Vec::new(),
                openclaw_requires: Vec::new(),
                openclaw_install: Vec::new(),
                prelude_excerpt: String::new(),
                code_fence_languages: Vec::new(),
                shell_prelude: false,
                searchable_text: "calendar_sync handle schedule sync tasks".into(),
            },
        ];

        let selected = select_relevant_skills("please check battery status battery", &skills, 2);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].file_name, "battery_monitor");
    }

    #[test]
    fn backend_has_preferred_auth_accepts_codex_auth_file() {
        let dir = tempdir().unwrap();
        let auth_path = dir.path().join("auth.json");
        std::fs::write(
            &auth_path,
            r#"{"tokens":{"access_token":"token-a","refresh_token":"token-b"}}"#,
        )
        .unwrap();

        let cfg = json!({
            "oauth": {
                "auth_path": auth_path
            }
        });

        assert!(backend_has_preferred_auth("openai-codex", &cfg));
    }

    #[test]
    fn backend_has_preferred_auth_rejects_empty_codex_auth_file() {
        let dir = tempdir().unwrap();
        let auth_path = dir.path().join("auth.json");
        std::fs::write(
            &auth_path,
            r#"{"tokens":{"access_token":"","refresh_token":""}}"#,
        )
        .unwrap();

        let cfg = json!({
            "oauth": {
                "auth_path": auth_path
            }
        });

        assert!(!backend_has_preferred_auth("openai-codex", &cfg));
    }

    #[test]
    fn authenticated_codex_backend_is_promoted_above_defaults() {
        let dir = tempdir().unwrap();
        let auth_path = dir.path().join("auth.json");
        std::fs::write(
            &auth_path,
            r#"{"tokens":{"access_token":"token-a","refresh_token":"token-b"}}"#,
        )
        .unwrap();

        let config = LlmConfig {
            active_backend: "gemini".into(),
            fallback_backends: vec!["openai".into()],
            backends: json!({
                "gemini": {
                    "model": "gemini-2.5-flash"
                },
                "openai": {
                    "api_key": ""
                },
                "openai-codex": {
                    "oauth": {
                        "auth_path": auth_path
                    }
                }
            }),
        };
        let dir = tempdir().unwrap();
        let key_store = KeyStore::new(dir.path());
        let candidates = build_backend_candidates(&config, &PluginManager::new(), &key_store);

        assert_eq!(
            candidates.first().map(|candidate| candidate.name.as_str()),
            Some("openai-codex")
        );
        assert!(candidates
            .iter()
            .find(|candidate| candidate.name == "openai-codex")
            .map(|candidate| candidate.priority >= AUTHENTICATED_BACKEND_PRIORITY_BOOST)
            .unwrap_or(false));
    }

    #[test]
    fn llm_config_default_includes_expected_fallbacks() {
        let config = LlmConfig::default();

        assert_eq!(config.active_backend, "gemini");
        assert_eq!(
            config.fallback_backends,
            vec!["openai".to_string(), "ollama".to_string()]
        );
    }

    #[test]
    fn llm_config_load_uses_default_fallbacks_when_file_missing() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("missing-llm-config.json");

        let config = LlmConfig::load(&config_path.to_string_lossy());

        assert_eq!(config.active_backend, "gemini");
        assert_eq!(
            config.fallback_backends,
            vec!["openai".to_string(), "ollama".to_string()]
        );
    }

    #[test]
    fn build_skill_prefetch_message_returns_snapshot_block() {
        let skills = vec![TextualSkill {
            file_name: "battery_monitor".into(),
            absolute_path: "/tmp/battery/SKILL.md".into(),
            description: "Inspect battery and power telemetry".into(),
            tags: vec!["battery".into()],
            triggers: vec!["check battery".into()],
            examples: Vec::new(),
            openclaw_requires: vec!["upower".into()],
            openclaw_install: vec!["apt install upower".into()],
            prelude_excerpt: String::new(),
            code_fence_languages: Vec::new(),
            shell_prelude: false,
            searchable_text: "battery_monitor inspect battery and power telemetry".into(),
        }];

        let message = build_skill_prefetch_message(&skills).unwrap_or_default();

        assert!(message.contains("Prefetched Skill Snapshot"));
        assert!(message.contains("battery_monitor"));
        assert!(message.contains("tags: battery"));
        assert!(message.contains("requires: upower"));
    }

    #[test]
    fn build_role_supervisor_hint_includes_role_scope() {
        let role = AgentRole {
            name: "device_monitor".into(),
            system_prompt: "monitor device".into(),
            allowed_tools: Vec::new(),
            max_iterations: 4,
            description: "Inspect device health metrics".into(),
            role_type: "worker".into(),
            auto_start: false,
            can_delegate_to: Vec::new(),
            prompt_mode: Some(PromptMode::Minimal),
            reasoning_policy: Some(ReasoningPolicy::Native),
        };

        let hint = build_role_supervisor_hint("sess-1", "check battery health", &role);

        assert!(hint.contains("sess-1"));
        assert!(hint.contains("device_monitor"));
        assert!(hint.contains("Inspect device health metrics"));
    }

    #[test]
    fn build_progress_marker_uses_tool_calls_for_tool_only_rounds() {
        let marker = build_progress_marker(
            "",
            "",
            &[LlmToolCall {
                id: "call_1".into(),
                name: "search_tools".into(),
                args: json!({"query": "ALL"}),
            }],
        );

        assert!(marker.contains("search_tools"));
        assert!(marker.contains("\"ALL\""));
    }

    #[test]
    fn score_tool_search_match_finds_document_extractor_from_pdf_query() {
        let tool = crate::llm::backend::LlmToolDecl {
            name: "extract_document_text".into(),
            description:
                "Extract readable text from a local document such as TXT, Markdown, JSON, CSV, or PDF."
                    .into(),
            parameters: json!({"type": "object"}),
        };
        let unrelated = crate::llm::backend::LlmToolDecl {
            name: "web_search".into(),
            description: "Search the web using the configured search provider stack.".into(),
            parameters: json!({"type": "object"}),
        };

        let extractor_score = score_tool_search_match(&tool, "PDF extract document text tool");
        let unrelated_score = score_tool_search_match(&unrelated, "PDF extract document text tool");

        assert!(extractor_score > unrelated_score);
        assert!(extractor_score >= 400);
    }

    #[test]
    fn score_tool_search_match_prefers_exact_tool_name() {
        let exact = crate::llm::backend::LlmToolDecl {
            name: "extract_document_text".into(),
            description: "Document extractor".into(),
            parameters: json!({"type": "object"}),
        };
        let partial = crate::llm::backend::LlmToolDecl {
            name: "extract_document_summary".into(),
            description: "Summarize extracted documents".into(),
            parameters: json!({"type": "object"}),
        };

        assert!(score_tool_search_match(&exact, "extract_document_text")
            > score_tool_search_match(&partial, "extract_document_text"));
    }

    #[test]
    fn parse_tool_uri_decodes_query_parameters() {
        let (tool_name, params) = parse_tool_uri(
            "tool://extract_document_text?path=docs%2Freport.pdf&output_path=tmp%2Freport.txt&max_chars=1200",
        )
        .expect("tool uri");

        assert_eq!(tool_name, "extract_document_text");
        assert_eq!(params.get("path").map(String::as_str), Some("docs/report.pdf"));
        assert_eq!(
            params.get("output_path").map(String::as_str),
            Some("tmp/report.txt")
        );
        assert_eq!(params.get("max_chars").map(String::as_str), Some("1200"));
    }

    #[test]
    fn parse_shell_like_args_preserves_quoted_groups() {
        let parsed = parse_shell_like_args("--name \"hello world\" 'alpha beta'");

        assert_eq!(
            parsed,
            vec![
                "--name".to_string(),
                "hello world".to_string(),
                "alpha beta".to_string(),
            ]
        );
    }

    #[test]
    fn extract_explicit_file_paths_finds_absolute_files() {
        let prompt = "Read /tmp/ds_olympiad/problem.md and '/tmp/ds_olympiad/answer.md'.";

        let paths = extract_explicit_file_paths(prompt);

        assert_eq!(
            paths,
            vec![
                "/tmp/ds_olympiad/problem.md".to_string(),
                "/tmp/ds_olympiad/answer.md".to_string(),
            ]
        );
    }

    #[test]
    fn extract_explicit_paths_includes_output_directories() {
        let prompt = "Write files into /tmp/ds_olympiad/result/library-used and /tmp/ds_olympiad/result/library-not-used.";

        let paths = extract_explicit_paths(prompt);
        let directories = extract_explicit_directory_paths(prompt);

        assert!(paths.contains(&"/tmp/ds_olympiad/result/library-used".to_string()));
        assert!(paths.contains(&"/tmp/ds_olympiad/result/library-not-used".to_string()));
        assert_eq!(directories.len(), 2);
    }

    #[test]
    fn collect_grounded_paths_reads_paths_from_tool_stdout_json() {
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "read_file",
            json!({
                "stdout": "{\"path\":\"/tmp/ds_olympiad/problem.md\",\"content\":\"demo\"}",
                "success": true
            }),
        )];

        let grounded = collect_grounded_paths(&messages);

        assert!(grounded.contains("/tmp/ds_olympiad/problem.md"));
    }

    #[test]
    fn build_prefetched_prompt_file_messages_prefetches_markdown_specs() {
        let dir = tempdir().unwrap();
        let spec_path = dir.path().join("problem.md");
        std::fs::write(&spec_path, "# Problem\nUse the real data.\n").unwrap();
        let csv_path = dir.path().join("data.csv");
        std::fs::write(&csv_path, "x,y\n1,2\n").unwrap();

        let messages = build_prefetched_prompt_file_messages(&[
            spec_path.to_string_lossy().to_string(),
            csv_path.to_string_lossy().to_string(),
        ]);

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].tool_name, "read_file");
        assert_eq!(
            messages[0]
                .tool_result
                .get("path")
                .and_then(|value| value.as_str()),
            Some(spec_path.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn build_prefetched_prompt_file_context_includes_authoritative_excerpt() {
        let dir = tempdir().unwrap();
        let spec_path = dir.path().join("problem.md");
        std::fs::write(&spec_path, "# Problem\nUse the exact CSV files.\n").unwrap();

        let context =
            build_prefetched_prompt_file_context(&[spec_path.to_string_lossy().to_string()])
                .unwrap_or_default();

        assert!(context.contains("Prompt File Excerpts"));
        assert!(context.contains(spec_path.to_string_lossy().as_ref()));
        assert!(context.contains("Use the exact CSV files."));
    }

    #[test]
    fn build_authoritative_problem_requirements_context_summarizes_levels() {
        let dir = tempdir().unwrap();
        let spec_path = dir.path().join("problem.md");
        std::fs::write(
            &spec_path,
            "## [1단계] 초등학생\n**문제:** `level1_fruit_sales.csv` 파일에서 가장 많이 팔린 과일을 찾으세요.\n## [2단계] 초등학생\n**문제:** `level2_study_time.csv` 파일에서 하루 평균 공부 시간을 구하세요.\n",
        )
        .unwrap();

        let context = build_authoritative_problem_requirements_context(&[spec_path
            .to_string_lossy()
            .to_string()])
        .unwrap_or_default();

        assert!(context.contains("Level 1 uses"));
        assert!(context.contains("level1_fruit_sales.csv"));
        assert!(context.contains("Level 2 uses"));
        assert!(context.contains("exactly one final answer value"));
    }

    #[test]
    fn extract_level_number_from_output_path_reads_level_suffix() {
        assert_eq!(
            extract_level_number_from_output_path(
                "/tmp/ds_olympiad/result/library-used/level-4-solution.py"
            )
            .as_deref(),
            Some("4")
        );
    }

    #[test]
    fn extract_explicit_file_paths_finds_paths_inside_function_calls() {
        let script = "with open('/tmp/ds_olympiad/data.csv') as fh:\n    print(fh.read())";

        let paths = extract_explicit_file_paths(script);

        assert_eq!(paths, vec!["/tmp/ds_olympiad/data.csv".to_string()]);
    }

    #[test]
    fn validate_generated_code_grounding_requires_prompt_files_to_be_read_first() {
        let dir = tempdir().unwrap();
        let input_path = dir.path().join("problem.md");
        std::fs::write(&input_path, "demo").unwrap();
        let prompt = format!("Read {} and solve it.", input_path.display());
        let grounded_paths = HashSet::new();
        let grounded_csv_headers = HashSet::new();

        let result = validate_generated_code_grounding(
            &prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "print('hello')",
            "",
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains(input_path.to_string_lossy().as_ref()));
    }

    #[test]
    fn validate_generated_code_grounding_allows_nonexistent_prompt_output_paths() {
        let dir = tempdir().unwrap();
        let output_path = dir.path().join("new_result.txt");
        let prompt = format!("Write the answer to {}.", output_path.display());
        let grounded_paths = HashSet::new();
        let grounded_csv_headers = HashSet::new();

        let result = validate_generated_code_grounding(
            &prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "print('hello')",
            "",
        );

        assert!(result.is_ok());
    }

    #[test]
    fn validate_generated_code_grounding_rejects_unverified_paths() {
        let prompt = "Read /tmp/ds_olympiad/problem.md and solve it.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "read_file",
            json!({
                "stdout": "{\"path\":\"/tmp/ds_olympiad/problem.md\",\"content\":\"demo\"}",
                "success": true
            }),
        )];
        let grounded_paths = collect_grounded_paths(&messages);
        let grounded_csv_headers = collect_grounded_csv_headers(&messages);

        let result = validate_generated_code_grounding(
            prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "with open('/tmp/ds_olympiad/data.csv') as fh:\n    print(fh.read())",
            "",
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unverified file paths"));
    }

    #[test]
    fn validate_generated_code_grounding_accepts_declared_output_paths() {
        let prompt = "Read /tmp/ds_olympiad/problem.md and save the script to /tmp/ds_olympiad/result/library-used.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "read_file",
            json!({
                "stdout": "{\"path\":\"/tmp/ds_olympiad/problem.md\",\"content\":\"demo\"}",
                "success": true
            }),
        )];
        let grounded_paths = collect_grounded_paths(&messages);
        let grounded_csv_headers = collect_grounded_csv_headers(&messages);

        let result = validate_generated_code_grounding(
            prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "# /tmp/ds_olympiad/result/library-used/level-1-solution.py\nwith open('/tmp/ds_olympiad/problem.md') as fh:\n    print(fh.read())",
            "",
        )
        .unwrap();

        assert_eq!(
            result.declared_output_path.as_deref(),
            Some("/tmp/ds_olympiad/result/library-used/level-1-solution.py")
        );
    }

    #[test]
    fn validate_generated_code_grounding_requires_leading_comment_output_path() {
        let prompt = "Read /tmp/ds_olympiad/problem.md and save the script to /tmp/ds_olympiad/result/library-used using the filename level-{problem level}-solution.py.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "read_file",
            json!({
                "stdout": "{\"path\":\"/tmp/ds_olympiad/problem.md\",\"content\":\"demo\"}",
                "success": true
            }),
        )];
        let grounded_paths = collect_grounded_paths(&messages);
        let grounded_csv_headers = collect_grounded_csv_headers(&messages);

        let result = validate_generated_code_grounding(
            prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "with open('/tmp/ds_olympiad/problem.md') as source:\n    print(source.read())\nwith open('/tmp/ds_olympiad/result/library-used/level-1-solution.py', 'w') as fh:\n    fh.write('demo')\nprint('[Level 1] Answer: demo')",
            "",
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("leading comment line"));
    }

    #[test]
    fn validate_generated_code_grounding_requires_declared_output_path_when_prompt_demands_files() {
        let prompt = "Write each result to /tmp/ds_olympiad/result/library-used using the filename level-{problem level}-solution.py after reading /tmp/ds_olympiad/problem.md.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "read_file",
            json!({
                "stdout": "{\"path\":\"/tmp/ds_olympiad/problem.md\",\"content\":\"demo\"}",
                "success": true
            }),
        )];
        let grounded_paths = collect_grounded_paths(&messages);
        let grounded_csv_headers = collect_grounded_csv_headers(&messages);

        let result = validate_generated_code_grounding(
            prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "with open('/tmp/ds_olympiad/problem.md') as fh:\n    print(fh.read())",
            "",
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("leading comment line"));
    }

    #[test]
    fn validate_generated_code_grounding_rejects_multi_level_script_when_one_file_is_required() {
        let prompt = "Write each result to /tmp/ds_olympiad/result/library-used using the filename level-{problem level}-solution.py after reading /tmp/ds_olympiad/problem.md.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "read_file",
            json!({
                "stdout": "{\"path\":\"/tmp/ds_olympiad/problem.md\",\"content\":\"demo\"}",
                "success": true
            }),
        )];
        let grounded_paths = collect_grounded_paths(&messages);
        let grounded_csv_headers = collect_grounded_csv_headers(&messages);

        let result = validate_generated_code_grounding(
            prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "# /tmp/ds_olympiad/result/library-used/level-1-solution.py\nwith open('/tmp/ds_olympiad/problem.md') as source:\n    print(source.read())\nprint('[Level 1] Answer: 1')\nprint('[Level 2] Answer: 2')",
            "",
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exactly one level per script"));
    }

    #[test]
    fn validate_generated_code_grounding_accepts_known_inputs() {
        let prompt = "Read /tmp/ds_olympiad/problem.md and solve it.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "read_file",
            json!({
                "stdout": "{\"path\":\"/tmp/ds_olympiad/problem.md\",\"content\":\"demo\"}",
                "success": true
            }),
        )];
        let grounded_paths = collect_grounded_paths(&messages);
        let grounded_csv_headers = collect_grounded_csv_headers(&messages);

        let result = validate_generated_code_grounding(
            prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "with open('/tmp/ds_olympiad/problem.md') as fh:\n    print(fh.read())",
            "",
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap().declared_output_path, None);
    }

    #[test]
    fn validate_generated_code_grounding_rejects_unverified_csv_columns() {
        let prompt = "Read /tmp/ds_olympiad/level1_fruit_sales.csv and solve it.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "extract_document_text",
            json!({
                "path": "/tmp/ds_olympiad/level1_fruit_sales.csv",
                "text_preview": "Fruit\nApple\nBanana\n",
                "status": "success"
            }),
        )];
        let grounded_paths = collect_grounded_paths(&messages);
        let grounded_csv_headers = collect_grounded_csv_headers(&messages);

        let result = validate_generated_code_grounding(
            prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "import csv\nwith open('/tmp/ds_olympiad/level1_fruit_sales.csv') as fh:\n    rows = [row['Sales'] for row in csv.DictReader(fh)]\nprint(rows)",
            "",
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unverified CSV columns"));
    }

    #[test]
    fn validate_generated_code_grounding_requires_runtime_json_config_reads() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        std::fs::write(
            &config_path,
            r#"{"api":{"endpoint":"https://api.example.com/v2/data"}}"#,
        )
        .unwrap();

        let prompt = format!(
            "Read {} and create a Python script that uses the config to call the API.",
            config_path.to_string_lossy()
        );
        let grounded_paths = HashSet::from([config_path.to_string_lossy().to_string()]);
        let grounded_csv_headers = HashSet::new();

        let result = validate_generated_code_grounding(
            &prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "import requests\nENDPOINT='https://api.example.com/v2/data'\nprint(requests.get(ENDPOINT).status_code)",
            "",
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("must read the provided JSON input at runtime"));
    }

    #[test]
    fn validate_generated_code_grounding_rejects_speculative_placeholders() {
        let prompt = "Read /tmp/ds_olympiad/level5_multiple_regression.csv and solve it.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "extract_document_text",
            json!({
                "path": "/tmp/ds_olympiad/level5_multiple_regression.csv",
                "text_preview": "X1,X2,Y\n1,2,3\n",
                "status": "success"
            }),
        )];
        let grounded_paths = collect_grounded_paths(&messages);
        let grounded_csv_headers = collect_grounded_csv_headers(&messages);

        let result = validate_generated_code_grounding(
            prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "with open('/tmp/ds_olympiad/level5_multiple_regression.csv') as fh:\n    print('placeholder value')",
            "",
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("speculative assumptions"));
    }

    #[test]
    fn validate_generated_code_grounding_rejects_cross_level_inputs_for_saved_scripts() {
        let prompt = "Write each result to /tmp/ds_olympiad/result/library-used and /tmp/ds_olympiad/result/library-not-used using the filename level-{problem level}-solution.py after reading /tmp/ds_olympiad/problem.md, /tmp/ds_olympiad/level1_fruit_sales.csv, /tmp/ds_olympiad/level2_study_time.csv.";
        let messages = vec![
            LlmMessage::tool_result(
                "call_1",
                "read_file",
                json!({
                    "stdout": "{\"path\":\"/tmp/ds_olympiad/problem.md\",\"content\":\"demo\"}",
                    "success": true
                }),
            ),
            LlmMessage::tool_result(
                "call_2",
                "extract_document_text",
                json!({
                    "path": "/tmp/ds_olympiad/level1_fruit_sales.csv",
                    "text_preview": "Fruit\nApple\nBanana\n",
                    "status": "success"
                }),
            ),
            LlmMessage::tool_result(
                "call_3",
                "extract_document_text",
                json!({
                    "path": "/tmp/ds_olympiad/level2_study_time.csv",
                    "text_preview": "Day,Hours\nMon,2.0\n",
                    "status": "success"
                }),
            ),
        ];
        let grounded_paths = collect_grounded_paths(&messages);
        let grounded_csv_headers = collect_grounded_csv_headers(&messages);

        let result = validate_generated_code_grounding(
            prompt,
            &grounded_paths,
            &grounded_csv_headers,
            "# /tmp/ds_olympiad/result/library-used/level-1-solution.py\nimport csv\nwith open('/tmp/ds_olympiad/level2_study_time.csv') as fh:\n    print(next(csv.DictReader(fh))['Hours'])",
            "",
        );

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("matching inspected input file"));
    }

    #[test]
    fn validate_generated_code_execution_output_rejects_multi_token_answers() {
        let result = validate_generated_code_execution_output(
            "[Level 4] Answer: 81.2 8 12.03\n",
            Some("4"),
            true,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("multiple answer tokens"));
    }

    #[test]
    fn validate_generated_code_execution_output_accepts_single_value_answers() {
        let result = validate_generated_code_execution_output(
            "[Level 2] Answer: 2.2시간\n",
            Some("2"),
            true,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn expected_persisted_level_script_paths_expands_all_levels_and_dirs() {
        let prompt = "Write each result to /tmp/ds_olympiad/result/library-used and /tmp/ds_olympiad/result/library-not-used using the filename level-{problem level}-solution.py after reading /tmp/ds_olympiad/problem.md, /tmp/ds_olympiad/level1_fruit_sales.csv, /tmp/ds_olympiad/level2_study_time.csv, /tmp/ds_olympiad/level3_student_physical.csv.";

        let expected = expected_persisted_level_script_paths(prompt);

        assert_eq!(
            expected,
            vec![
                "/tmp/ds_olympiad/result/library-not-used/level-1-solution.py",
                "/tmp/ds_olympiad/result/library-not-used/level-2-solution.py",
                "/tmp/ds_olympiad/result/library-not-used/level-3-solution.py",
                "/tmp/ds_olympiad/result/library-used/level-1-solution.py",
                "/tmp/ds_olympiad/result/library-used/level-2-solution.py",
                "/tmp/ds_olympiad/result/library-used/level-3-solution.py",
            ]
        );
    }

    #[test]
    fn persist_generated_code_copy_writes_requested_output_file() {
        let dir = tempdir().unwrap();
        let output_path = dir.path().join("result/library-used/level-1-solution.py");

        persist_generated_code_copy("print('ok')\n", &output_path).unwrap();

        assert_eq!(
            std::fs::read_to_string(&output_path).unwrap(),
            "print('ok')\n"
        );
    }

    #[test]
    fn generated_code_runtime_spec_maps_supported_runtimes() {
        assert_eq!(
            generated_code_runtime_spec("python"),
            Some(("python3", ".py"))
        );
        assert_eq!(
            generated_code_runtime_spec("python3"),
            Some(("python3", ".py"))
        );
        assert_eq!(generated_code_runtime_spec("node"), Some(("node", ".js")));
        assert_eq!(generated_code_runtime_spec("bash"), Some(("bash", ".sh")));
        assert_eq!(generated_code_runtime_spec("ruby"), None);
    }

    #[test]
    fn sanitize_generated_code_name_normalizes_user_input() {
        assert_eq!(
            sanitize_generated_code_name("Hello World.py"),
            "hello-world-py"
        );
        assert_eq!(sanitize_generated_code_name("   "), "script");
        assert_eq!(
            sanitize_generated_code_name("battery_probe"),
            "battery-probe"
        );
    }

    #[test]
    fn generated_code_script_path_uses_codes_directory() {
        let base_dir = std::path::Path::new("/opt/usr/share/tizenclaw");
        let script_path = generated_code_script_path(base_dir, "python", "Battery Probe").unwrap();

        assert!(script_path.starts_with(base_dir.join("codes")));
        let file_name = script_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap();
        assert!(file_name.contains("-generated-battery-probe"));
        assert_eq!(
            script_path.extension().and_then(|ext| ext.to_str()),
            Some("py")
        );
    }

    #[test]
    fn manage_generated_code_tool_deletes_only_named_file() {
        let unique = format!(
            "tizenclaw-generated-code-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let base_dir = std::env::temp_dir().join(unique);
        let codes_dir = base_dir.join("codes");
        std::fs::create_dir_all(&codes_dir).unwrap();
        let keep_path = codes_dir.join("keep.py");
        let delete_path = codes_dir.join("delete.py");
        std::fs::write(&keep_path, "print('keep')").unwrap();
        std::fs::write(&delete_path, "print('delete')").unwrap();

        let result = manage_generated_code_tool("delete", Some("delete.py"), &base_dir);

        assert_eq!(
            result.get("status").and_then(|v| v.as_str()),
            Some("success")
        );
        assert!(keep_path.exists());
        assert!(!delete_path.exists());

        let _ = std::fs::remove_dir_all(base_dir);
    }

    #[test]
    fn append_dashboard_outbound_message_keeps_recent_entries() {
        let unique = format!(
            "tizenclaw-outbound-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let base_dir = std::env::temp_dir().join(unique);

        for idx in 0..205 {
            append_dashboard_outbound_message(
                &base_dir,
                Some("Alert"),
                &format!("message-{}", idx),
                Some("sess-1"),
            )
            .unwrap();
        }

        let queue_path = dashboard_outbound_queue_path(&base_dir);
        let content = std::fs::read_to_string(&queue_path).unwrap();
        let lines = content.lines().collect::<Vec<_>>();

        assert_eq!(lines.len(), MAX_OUTBOUND_DASHBOARD_MESSAGES);
        assert!(content.contains("message-204"));
        assert!(!content.contains("message-0"));

        let _ = std::fs::remove_dir_all(base_dir);
    }

    #[test]
    fn extract_final_text_prefers_final_block() {
        let text = "<think>plan</think>\n<final>Visible answer</final>";
        assert_eq!(extract_final_text(text), "Visible answer");
    }

    #[test]
    fn extract_final_text_strips_think_block_without_final_tag() {
        let text = "<think>private plan</think>\nVisible answer";
        assert_eq!(extract_final_text(text), "Visible answer");
    }

    #[test]
    fn simple_file_management_requests_are_detected() {
        assert!(is_simple_file_management_request(
            "Create a project structure with README.md and src/app.py in the current working directory."
        ));
        assert!(should_skip_memory_for_prompt(
            "Create a project structure with README.md and src/app.py in the current working directory."
        ));
        assert!(!is_simple_file_management_request(
            "Write and execute a Python script that prints the fibonacci sequence."
        ));
        assert!(is_simple_file_management_request(
            "config/app.toml 파일을 열어 내용을 보여줘."
        ));
    }

    #[tokio::test]
    async fn file_manager_tool_can_force_rust_fallback_for_reads() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("note.txt");
        std::fs::write(&file_path, "alpha\n").unwrap();

        let result = file_manager_tool(
            &json!({
                "operation": "read",
                "path": "note.txt",
                "backend_preference": "rust_fallback"
            }),
            dir.path(),
        )
        .await;

        assert_eq!(result["success"], json!(true));
        assert_eq!(result["backend"], json!("rust_fallback"));
        assert_eq!(result["content"], json!("alpha\n"));
    }

    #[tokio::test]
    async fn file_manager_tool_read_truncates_large_text_files() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("large.txt");
        let content = "alpha ".repeat(5000);
        std::fs::write(&file_path, &content).unwrap();

        let result = file_manager_tool(
            &json!({
                "operation": "read",
                "path": "large.txt",
                "backend_preference": "rust_fallback",
                "max_chars": 200
            }),
            dir.path(),
        )
        .await;

        assert_eq!(result["success"], json!(true));
        assert_eq!(result["truncated"], json!(true));
        assert_eq!(result["total_chars"], json!(content.chars().count()));
        assert_eq!(result["paragraph_count"], json!(1));
        assert!(result["content"].as_str().unwrap_or("").chars().count() <= 240);
    }

    #[tokio::test]
    async fn file_manager_tool_read_reports_paragraph_preview_for_multiline_text() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("summary.txt");
        let content = "First paragraph about context.\n\nSecond paragraph with more detail.\n\nThird paragraph with conclusion.";
        std::fs::write(&file_path, content).unwrap();

        let result = file_manager_tool(
            &json!({
                "operation": "read",
                "path": "summary.txt",
                "backend_preference": "rust_fallback",
                "max_chars": 400
            }),
            dir.path(),
        )
        .await;

        assert_eq!(result["success"], json!(true));
        assert_eq!(result["paragraph_count"], json!(3));
        let preview = result["paragraph_preview"].as_str().unwrap_or("");
        assert!(preview.contains("First paragraph"));
        assert!(preview.contains("Second paragraph"));
        assert!(preview.contains("Third paragraph"));
    }

    #[tokio::test]
    async fn file_manager_tool_read_directory_returns_listing_payload() {
        let dir = tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("emails")).unwrap();
        std::fs::write(dir.path().join("emails/alpha.txt"), "alpha").unwrap();
        std::fs::write(dir.path().join("emails/beta.txt"), "beta").unwrap();

        let result = file_manager_tool(
            &json!({
                "operation": "read",
                "path": "emails",
                "backend_preference": "rust_fallback"
            }),
            dir.path(),
        )
        .await;

        assert_eq!(result["success"], json!(true));
        assert_eq!(result["is_dir"], json!(true));
        assert_eq!(result["backend"], json!("rust_fallback"));
        let content = result["content"].as_str().unwrap_or("");
        assert!(content.contains("Directory listing"));
        assert!(content.contains("alpha.txt"));
        assert!(content.contains("beta.txt"));
        assert_eq!(result["entries"].as_array().map(|v| v.len()), Some(2));
    }

    #[tokio::test]
    async fn file_manager_tool_read_can_return_pattern_snippets() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("quote.html");
        std::fs::write(
            &file_path,
            "<html><body>noise<data-symbol=\"AAPL\" data-field=\"regularMarketPrice\" data-value=\"260.48\">AAPL 260.48</data></body></html>",
        )
        .unwrap();

        let result = file_manager_tool(
            &json!({
                "operation": "read",
                "path": "quote.html",
                "backend_preference": "rust_fallback",
                "pattern": "regularMarketPrice",
                "max_chars": 200
            }),
            dir.path(),
        )
        .await;

        assert_eq!(result["success"], json!(true));
        assert_eq!(result["match_count"], json!(1));
        assert!(result["content"]
            .as_str()
            .unwrap_or("")
            .contains("260.48"));
    }

    #[test]
    fn build_text_read_payload_returns_full_small_file_even_with_pattern() {
        let content = "# Upcoming Tech Conferences\n\n| Name | Date |\n|---|---|\n| ExampleConf 2026 | June 2-3, 2026 |\n";
        let payload = build_text_read_payload(content, Some("Upcoming|ExampleConf"), 400);

        assert_eq!(payload["truncated"], json!(false));
        assert_eq!(payload["content"], json!(content));
    }

    #[tokio::test]
    async fn file_manager_tool_read_supports_alternative_patterns() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("quote.txt");
        std::fs::write(
            &file_path,
            "Ticker: AAPL\nExchange: NASDAQ\nCurrent price: 260.48 USD\n",
        )
        .unwrap();

        let result = file_manager_tool(
            &json!({
                "operation": "read",
                "path": "quote.txt",
                "backend_preference": "rust_fallback",
                "pattern": "MSFT|NASDAQ|price",
                "max_chars": 200
            }),
            dir.path(),
        )
        .await;

        assert_eq!(result["success"], json!(true));
        assert!(result["match_count"].as_u64().unwrap_or(0) >= 1);
        let content = result["content"].as_str().unwrap_or("");
        assert!(content.contains("NASDAQ"));
        assert!(content.contains("Current price"));
    }

    #[tokio::test]
    async fn file_manager_tool_read_supports_regex_patterns() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("quote.json");
        std::fs::write(
            &file_path,
            r#"{"symbol":"AAPL","price":{"regularMarketPrice":{"raw":260.48}}}"#,
        )
        .unwrap();

        let result = file_manager_tool(
            &json!({
                "operation": "read",
                "path": "quote.json",
                "backend_preference": "rust_fallback",
                "pattern": "\"symbol\":\"AAPL\".{0,80}regularMarketPrice\"?:\\{\"raw\":?[0-9.]+",
                "max_chars": 200
            }),
            dir.path(),
        )
        .await;

        assert_eq!(result["success"], json!(true));
        assert!(result["match_count"].as_u64().unwrap_or(0) >= 1);
        assert!(result["content"].as_str().unwrap_or("").contains("260.48"));
    }

    #[tokio::test]
    async fn file_manager_tool_redirects_pdf_reads_to_document_extractor() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("paper.pdf");
        std::fs::write(
            &file_path,
            br#"%PDF-1.4
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Count 1 /Kids [3 0 R] >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>
endobj
4 0 obj
<< /Length 44 >>
stream
BT /F1 12 Tf 72 120 Td (typed WebSocket) Tj ET
endstream
endobj
5 0 obj
<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>
endobj
xref
0 6
0000000000 65535 f 
0000000010 00000 n 
0000000063 00000 n 
0000000122 00000 n 
0000000248 00000 n 
0000000342 00000 n 
trailer
<< /Root 1 0 R /Size 6 >>
startxref
412
%%EOF
"#,
        )
        .unwrap();

        let result = file_manager_tool(
            &json!({
                "operation": "read",
                "path": "paper.pdf"
            }),
            dir.path(),
        )
        .await;

        assert_eq!(result["status"], json!("success"));
        assert_eq!(result["redirected_from"], json!("file_manager.read"));
        assert_eq!(result["recommended_tool"], json!("extract_document_text"));
        assert!(result["text_preview"]
            .as_str()
            .unwrap_or("")
            .contains("typed WebSocket"));
    }

    #[tokio::test]
    async fn file_manager_tool_redirects_csv_reads_to_tabular_inspector() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("sales.csv");
        std::fs::write(&file_path, "region,revenue\nNorth,10\nSouth,20\n").unwrap();

        let result = file_manager_tool(
            &json!({
                "operation": "read",
                "path": "sales.csv"
            }),
            dir.path(),
        )
        .await;

        assert_eq!(result["status"], json!("success"));
        assert_eq!(result["redirected_from"], json!("file_manager.read"));
        assert_eq!(result["recommended_tool"], json!("inspect_tabular_data"));
        assert_eq!(result["inspection"]["sheets"][0]["row_count"], json!(2));
    }

    #[tokio::test]
    async fn file_manager_tool_rejects_binary_reads() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("blob.bin");
        std::fs::write(&file_path, [0_u8, 159, 146, 150, 0, 1, 2, 3]).unwrap();

        let result = file_manager_tool(
            &json!({
                "operation": "read",
                "path": "blob.bin"
            }),
            dir.path(),
        )
        .await;

        assert!(result["error"]
            .as_str()
            .unwrap_or("")
            .contains("Binary file detected"));
        assert_eq!(result["recommended_tool"], json!("extract_document_text"));
    }

    #[tokio::test]
    async fn file_manager_tool_can_force_rust_fallback_for_moves() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("from.txt");
        std::fs::write(&src, "alpha\n").unwrap();

        let result = file_manager_tool(
            &json!({
                "operation": "move",
                "src": "from.txt",
                "dst": "to.txt",
                "backend_preference": "rust_fallback"
            }),
            dir.path(),
        )
        .await;

        assert_eq!(result["success"], json!(true));
        assert_eq!(result["backend"], json!("rust_fallback"));
        assert!(!src.exists());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("to.txt")).unwrap(),
            "alpha\n"
        );
    }

    #[test]
    fn wrapped_telegram_prompt_uses_user_request_for_file_detection() {
        let wrapped = "You are handling a Telegram request through TizenClaw.\n\nTelegram development preferences:\n- Coding backend: codex\n- Coding model: gpt-5.4\n- Project directory: /home/hjhun/samba/github/tizenclaw\n- Coding execution mode: plan\n- Coding auto approve: on\n\nTelegram user request:\n지금 상태를 요약해서 알려줘.";

        assert!(!is_simple_file_management_request(wrapped));

        let wrapped_file_request = "You are handling a Telegram request through TizenClaw.\n\nTelegram development preferences:\n- Coding backend: codex\n- Coding model: gpt-5.4\n- Project directory: /home/hjhun/samba/github/tizenclaw\n- Coding execution mode: plan\n- Coding auto approve: on\n\nTelegram user request:\nconfig/app.toml 파일을 열어 내용을 보여줘.";

        assert!(is_simple_file_management_request(wrapped_file_request));
    }

    #[test]
    fn canonical_tool_trace_maps_file_manager_writes() {
        let trace = canonical_tool_trace(&LlmToolCall {
            id: "call_1".to_string(),
            name: "file_manager".to_string(),
            args: json!({
                "operation": "write",
                "path": "README.md",
                "content": "# Demo"
            }),
        });

        assert_eq!(trace["name"], json!("write_file"));
        assert_eq!(trace["arguments"]["path"], json!("README.md"));
    }

    #[test]
    fn expected_file_management_targets_tracks_relative_files_and_readme() {
        let targets = expected_file_management_targets(
            "Create a project structure with README and src/app.py in the current working directory.",
        );

        assert!(targets
            .iter()
            .any(|group| group.iter().any(|path| path == "src/app.py")));
        assert!(targets
            .iter()
            .any(|group| group.iter().any(|path| path == "README.md")));
    }

    #[test]
    fn expected_file_management_targets_tracks_dot_gitignore() {
        let targets =
            expected_file_management_targets("Create .gitignore and src/main.py for this project.");

        assert!(targets
            .iter()
            .any(|group| group.iter().any(|path| path == ".gitignore")));
    }

    #[test]
    fn expected_file_management_targets_track_report_outputs_for_research_prompts() {
        let targets = expected_file_management_targets(
            "Research the current stock price of Apple (AAPL) and save it to stock_report.txt.",
        );

        assert!(targets
            .iter()
            .any(|group| group.iter().any(|path| path == "stock_report.txt")));
    }

    #[test]
    fn expected_file_management_targets_ignore_email_domains_and_hosts() {
        let calendar_targets = expected_file_management_targets(
            "Schedule a meeting for next Tuesday at 3pm with john@example.com. Title it \"Project Sync\" and save it as project_sync_next_tuesday.ics.",
        );
        let weather_targets = expected_file_management_targets(
            "Create a Python script called weather.py that fetches weather data for San Francisco using the wttr.in API and prints a summary.",
        );

        assert!(calendar_targets
            .iter()
            .any(|group| group.iter().any(|path| path == "project_sync_next_tuesday.ics")));
        assert!(!calendar_targets
            .iter()
            .any(|group| group.iter().any(|path| path == "example.com")));
        assert!(weather_targets
            .iter()
            .any(|group| group.iter().any(|path| path == "weather.py")));
        assert!(!weather_targets
            .iter()
            .any(|group| group.iter().any(|path| path == "wttr.in")));
    }

    #[test]
    fn prompt_prefers_direct_specialized_tools_for_tabular_tasks() {
        let prompt =
            "Review quarterly_sales.csv and company_expenses.xlsx, then save the findings to data_summary.md.";

        assert!(prompt_requests_tabular_inspection(prompt));
        assert!(prompt_prefers_direct_specialized_tools(prompt));
        assert!(!prompt_requests_code_generation(prompt));
    }

    #[test]
    fn prompt_requests_prediction_market_briefing_detects_polymarket_format() {
        let prompt = "Fetch the top 3 trending prediction markets from Polymarket right now. For each market, include **Current odds:** Yes 55% / No 45% and **Related news:** in polymarket_briefing.md.";
        assert!(prompt_requests_prediction_market_briefing(prompt));
    }

    #[test]
    fn prompt_requests_prediction_market_briefing_detects_task_shape_without_literal_labels() {
        let prompt = "Fetch the top 3 trending prediction markets from Polymarket right now and save the result as polymarket_briefing.md.";
        assert!(prompt_requests_prediction_market_briefing(prompt));
    }

    #[test]
    fn prompt_supplies_prediction_market_briefing_evidence_detects_bounded_prompt() {
        let prompt = "Using only the evidence below, write polymarket_briefing.md in markdown.\n\n1. Will the Fed decrease interest rates by 50+ bps after the April 2026 meeting?\nCurrent odds: Yes 1% / No 99%\nRelated news: April 12, 2026 Reuters reported traders raised bets on a bigger Fed cut.\n\n2. Will the Iranian regime fall by April 30?\nCurrent odds: Yes 3% / No 97%\nRelated news: April 12, 2026 AP reported new protests and succession concerns.\n\n3. Will Bitcoin reach $150,000 in April?\nCurrent odds: Yes 47% / No 53%\nRelated news: April 13, 2026 Bloomberg reported ETF inflows stayed strong.";
        assert!(prompt_supplies_prediction_market_briefing_evidence(prompt));
    }

    #[test]
    fn render_prediction_market_briefing_from_prompt_evidence_preserves_requested_shape() {
        let prompt = "Using only the evidence below, write polymarket_briefing.md in markdown.\n\n1. Will the Fed decrease interest rates by 50+ bps after the April 2026 meeting?\nCurrent odds: Yes 1% / No 99%\nRelated news: April 12, 2026 Reuters reported traders raised bets on a bigger Fed cut.\n\n2. Will the Iranian regime fall by April 30?\nCurrent odds: Yes 3% / No 97%\nRelated news: April 12, 2026 AP reported new protests and succession concerns.\n\n3. Will Bitcoin reach $150,000 in April?\nCurrent odds: Yes 47% / No 53%\nRelated news: April 13, 2026 Bloomberg reported ETF inflows stayed strong.";
        let rendered = render_prediction_market_briefing_from_prompt_evidence(prompt).unwrap();
        assert!(rendered.starts_with("# Polymarket Briefing"));
        assert!(rendered.contains("## 1. Will the Fed decrease interest rates"));
        assert!(rendered.contains("**Current odds:** Yes 47% / No 53%"));
        assert!(rendered.contains("**Related news:** April 12, 2026 AP"));
    }

    #[test]
    fn completed_file_management_targets_accepts_valid_saved_output() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("daily_briefing.md"),
            "# Daily Briefing\n\nRevenue improved this week after stronger renewals.\n",
        )
        .unwrap();

        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "file_write",
            json!({
                "path": dir.path().join("daily_briefing.md").to_string_lossy().to_string(),
                "success": true,
                "bytes_written": 66
            }),
        )];

        let completed = completed_file_management_targets(
            "Create daily_briefing.md with a daily briefing.",
            dir.path(),
            &messages,
            false,
        );

        assert_eq!(completed, vec!["daily_briefing.md".to_string()]);
        assert_eq!(
            completion_text_for_file_targets(&completed),
            "Completed. Saved `daily_briefing.md` in the working directory."
        );
        let completion_message = completion_message_for_file_targets(dir.path(), &completed);
        assert!(completion_message.contains("Preview of `daily_briefing.md`"));
        assert!(completion_message.contains("Revenue improved this week"));
    }

    #[test]
    fn completion_message_for_small_markdown_targets_includes_full_content() {
        let dir = tempdir().unwrap();
        let body = "# Polymarket Briefing — 2026-04-13\n\n## 1. Will the Fed cut rates?\n**Current odds:** Yes 49% / No 51%\n**Related news:** Reuters reported softer inflation data on April 12, 2026.\n\n## 2. Will Bitcoin reach $150,000 in April?\n**Current odds:** Yes 47% / No 53%\n**Related news:** Bloomberg said ETF inflows stayed strong on April 13, 2026.\n\n## 3. Will the Iranian regime fall by April 30?\n**Current odds:** Yes 3% / No 97%\n**Related news:** AP reported new protests and succession concerns on April 12, 2026.\n";
        std::fs::write(dir.path().join("polymarket_briefing.md"), body).unwrap();

        let message = completion_message_for_file_targets(
            dir.path(),
            &["polymarket_briefing.md".to_string()],
        );

        assert!(message.contains("## 1. Will the Fed cut rates?"));
        assert!(message.contains("## 2. Will Bitcoin reach $150,000 in April?"));
        assert!(message.contains("## 3. Will the Iranian regime fall by April 30?"));
    }

    #[test]
    fn completion_message_for_large_markdown_targets_surfaces_multiple_sections() {
        let dir = tempdir().unwrap();
        let body = [
            "# Project Alpha Summary",
            "",
            "## 1. Project Overview",
            "Project Alpha is an analytics dashboard.",
            "",
            "## 2. Timeline",
            "Beta moved to May 6 and GA moved to May 27.",
            "",
            "## 3. Key Risks and Issues",
            "Security remediation and budget pressure remain active.",
            "",
            "## 4. Client and Business Impact",
            "Pipeline remains above the earlier ARR projection.",
            "",
            "## 5. Current Status",
            "Frontend work is progressing in parallel.",
            "",
            &"Extra detail ".repeat(260),
        ]
        .join("\n");
        std::fs::write(dir.path().join("alpha_summary.md"), body).unwrap();

        let message = completion_message_for_file_targets(
            dir.path(),
            &["alpha_summary.md".to_string()],
        );

        assert!(message.contains("## 1. Project Overview"));
        assert!(message.contains("## 3. Key Risks and Issues"));
        assert!(message.contains("## 5. Current Status"));
    }

    #[test]
    fn completion_text_for_image_targets_confirms_generation() {
        assert_eq!(
            completion_text_for_file_targets(&["robot_cafe.png".to_string()]),
            "Completed. Generated and saved `robot_cafe.png` in the working directory."
        );
    }

    #[test]
    fn invalid_file_management_targets_flag_prediction_market_briefing_without_odds() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("polymarket_briefing.md"),
            "# Polymarket Briefing\n\n## 1) Trending Markets\n**Related news:** Something happened.\n",
        )
        .unwrap();

        let invalid = invalid_file_management_targets(
            "Fetch the top 3 trending prediction markets from Polymarket right now and save polymarket_briefing.md with **Current odds:** Yes {X}% / No {Y}% and **Related news:** for each market question.",
            dir.path(),
            &[LlmMessage::tool_result(
                "call_1",
                "file_write",
                json!({
                    "path": dir.path().join("polymarket_briefing.md").to_string_lossy().to_string(),
                    "success": true,
                    "bytes_written": 84
                }),
            )],
        );

        assert_eq!(invalid, vec!["polymarket_briefing.md".to_string()]);
    }

    #[test]
    fn invalid_file_management_targets_flag_prediction_market_briefing_without_grounded_links() {
        let dir = tempdir().unwrap();
        let content = "# Polymarket Briefing — 2026-04-13\n\n## 1. Military action against Iran ends by April 17, 2026?\n**Current odds:** Yes 99% / No 1%\n**Related news:** Reuters reported within the last 48 hours on de-escalation signals. Article URL: current search results referenced.\n\n## 2. US x Iran permanent peace deal by April 22, 2026?\n**Current odds:** Yes 15% / No 85%\n**Related news:** AP reported within the last 48 hours that talks remained fragile. Article URL: current search results referenced.\n\n## 3. Will the Iranian regime fall by April 30?\n**Current odds:** Yes 2% / No 98%\n**Related news:** Reuters/AP-style current coverage suggested pressure without a confirmed article URL.\n";
        std::fs::write(dir.path().join("polymarket_briefing.md"), content).unwrap();

        let prompt = "Fetch the top 3 trending prediction markets from Polymarket (polymarket.com) right now. For each market, find a related recent news story (from the last 48 hours) that explains why people are betting on it. Save the result as polymarket_briefing.md with **Current odds:** Yes {X}% / No {Y}% and **Related news:** for each market question.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "file_write",
            json!({
                "path": dir.path().join("polymarket_briefing.md").to_string_lossy().to_string(),
                "success": true,
                "bytes_written": content.len()
            }),
        )];

        let invalid = invalid_file_management_targets(prompt, dir.path(), &messages);
        assert_eq!(invalid, vec!["polymarket_briefing.md".to_string()]);
    }

    #[test]
    fn invalid_file_management_targets_flag_prediction_market_briefing_with_stale_news_dates() {
        let dir = tempdir().unwrap();
        let content = "# Polymarket Briefing — 2026-04-13\n\n## 1. Military action against Iran ends by April 17, 2026?\n**Current odds:** Yes 99% / No 1%\n**Related news:** Reuters reported on 2026-04-07 that ceasefire talks were being explored. https://www.reuters.com/world/iran-war-live-tehran-rejects-ceasefire-deal-trumps-deadline-reopen-strait-hormuz-2026-04-07/\n\n## 2. US x Iran permanent peace deal by April 22, 2026?\n**Current odds:** Yes 15% / No 85%\n**Related news:** Reuters reported on 2026-04-10 that negotiations remained fragile. https://www.reuters.com/world/asia-pacific/us-iran-ceasefire-deal-shows-strain-ahead-talks-with-oil-flows-squeezed-2026-04-10/\n\n## 3. Will the Iranian regime fall by April 30?\n**Current odds:** Yes 2% / No 98%\n**Related news:** Reuters reported on 2026-04-08 that Iran's economy was under strain. https://www.reuters.com/world/middle-east/irans-shattered-economy-means-any-success-war-may-be-fleeting-2026-04-08/\n";
        std::fs::write(dir.path().join("polymarket_briefing.md"), content).unwrap();

        let prompt = "Fetch the top 3 trending prediction markets from Polymarket (polymarket.com) right now. For each market, find a related recent news story (from the last 48 hours) that explains why people are betting on it. Save the result as polymarket_briefing.md with **Current odds:** Yes {X}% / No {Y}% and **Related news:** for each market question.";
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "file_write",
            json!({
                "path": dir.path().join("polymarket_briefing.md").to_string_lossy().to_string(),
                "success": true,
                "bytes_written": content.len()
            }),
        )];

        let invalid = invalid_file_management_targets(prompt, dir.path(), &messages);
        assert_eq!(invalid, vec!["polymarket_briefing.md".to_string()]);
    }

    #[test]
    fn extract_specific_calendar_dates_accepts_named_month_without_comma() {
        let dates = extract_specific_calendar_dates(
            "Apr 13 2026 reported US military says it will blockade Iranian ports after talks ended without agreement.",
        );
        assert!(dates.contains(&(2026, 4, 13)));
    }

    #[test]
    fn completed_file_management_targets_accept_prediction_market_decimal_odds() {
        let dir = tempdir().unwrap();
        let content = "# Polymarket Briefing — 2026-04-13\n\n## 1. Will the next Prime Minister of Hungary be Viktor Orbán?\n**Current odds:** Yes 0.35% / No 99.65%\n**Related news:** Reuters reported on 2026-04-13 that the Hungarian forint jumped after Orbán's election defeat. https://www.reuters.com/business/hungarian-forint-jumps-after-orbans-election-defeat-2026-04-13/\n\n## 2. Will WTI Crude Oil (WTI) hit (HIGH) $110 in April?\n**Current odds:** Yes 72% / No 28%\n**Related news:** Reuters reported on 2026-04-12 that oil rose more than 7% to above $100 ahead of a U.S. blockade on Iran. https://www.reuters.com/business/energy/oil-bounces-back-above-100-after-us-iran-talks-end-stalemate-2026-04-12/\n\n## 3. Strait of Hormuz traffic returns to normal by end of April?\n**Current odds:** Yes 13% / No 88%\n**Related news:** Reuters reported on 2026-04-13 that Trump said the U.S. would begin a naval blockade of Iran's ports in the Strait of Hormuz. https://www.reuters.com/world/iran-war-live-trump-says-us-begin-naval-blockade-irans-ports-strait-hormuz-2026-04-13/\n";
        std::fs::write(dir.path().join("polymarket_briefing.md"), content).unwrap();

        let prompt = "Fetch the top 3 trending prediction markets from Polymarket (polymarket.com) right now. For each market, find a related recent news story (from the last 48 hours) that explains why people are betting on it. Save the result as polymarket_briefing.md with **Current odds:** Yes {X}% / No {Y}% and **Related news:** for each market question.";
        let messages = vec![
            LlmMessage::tool_result(
                "call_1",
                "web_search",
                json!({
                    "status": "success",
                    "result": {"results": [
                        {
                            "title": "Hungarian forint jumps after Orban's election defeat | Reuters",
                            "url": "https://www.reuters.com/business/hungarian-forint-jumps-after-orbans-election-defeat-2026-04-13/",
                            "snippet": ""
                        },
                        {
                            "title": "Oil rises over 7% to above $100 ahead of US blockade on Iran - Reuters",
                            "url": "https://www.reuters.com/business/energy/oil-bounces-back-above-100-after-us-iran-talks-end-stalemate-2026-04-12/",
                            "snippet": ""
                        },
                        {
                            "title": "Iran war live: Trump says US to begin naval blockade of Iran's ports in Strait of Hormuz",
                            "url": "https://www.reuters.com/world/iran-war-live-trump-says-us-begin-naval-blockade-irans-ports-strait-hormuz-2026-04-13/",
                            "snippet": ""
                        }
                    ]}
                }),
            ),
            LlmMessage::tool_result(
                "call_2",
                "file_write",
                json!({
                    "path": dir.path().join("polymarket_briefing.md").to_string_lossy().to_string(),
                    "success": true,
                    "bytes_written": 1449
                }),
            ),
        ];

        let invalid = invalid_file_management_targets(prompt, dir.path(), &messages);
        assert!(
            invalid.is_empty(),
            "invalid targets: {:?}; details: {:?}",
            invalid,
            describe_invalid_file_management_targets(prompt, dir.path(), &messages, &invalid)
        );

        let completed = completed_file_management_targets(prompt, dir.path(), &messages, false);
        assert_eq!(completed, vec!["polymarket_briefing.md".to_string()]);
    }

    #[test]
    fn completed_file_management_targets_accept_prediction_market_briefing_after_rate_limited_search() {
        let dir = tempdir().unwrap();
        let content = "# Polymarket Briefing — 2026-04-13\n\n## 1. Will WTI Crude Oil (WTI) hit (HIGH) $110 in April?\n**Current odds:** Yes 74% / No 27%\n**Related news:** Reuters reported on 2026-04-13 that oil prices stayed elevated as Iran-related shipping risks threatened supply through the Gulf. https://www.reuters.com/business/energy/oil-tankers-steer-clear-hormuz-ahead-us-blockade-2026-04-13/\n\n## 2. Strait of Hormuz traffic returns to normal by end of April?\n**Current odds:** Yes 13% / No 88%\n**Related news:** Reuters reported on 2026-04-13 that tanker traffic near the Strait of Hormuz remained disrupted while blockade fears grew. https://www.reuters.com/business/energy/oil-tankers-steer-clear-hormuz-ahead-us-blockade-2026-04-13/\n\n## 3. US x Iran permanent peace deal by April 22, 2026?\n**Current odds:** Yes 9% / No 92%\n**Related news:** Reuters reported on 2026-04-10 that ceasefire talks were still fragile and far from a durable settlement. https://www.reuters.com/world/middle-east/irans-guards-will-view-military-vessels-approaching-strait-ceasefire-breach-2026-04-12/\n";
        std::fs::write(dir.path().join("polymarket_briefing.md"), content).unwrap();

        let prompt = "Fetch the top 3 trending prediction markets from Polymarket (polymarket.com) right now. For each market, find a related recent news story (from the last 48 hours) that explains why people are betting on it. Save the result as polymarket_briefing.md with **Current odds:** Yes {X}% / No {Y}% and **Related news:** for each market question.";
        let messages = vec![
            LlmMessage::tool_result(
                "call_1",
                "web_search",
                json!({
                    "error": "DuckDuckGo mirror returned HTTP 429"
                }),
            ),
            LlmMessage::tool_result(
                "call_2",
                "file_write",
                json!({
                    "path": dir.path().join("polymarket_briefing.md").to_string_lossy().to_string(),
                    "success": true,
                    "bytes_written": 1234
                }),
            ),
        ];

        let invalid = invalid_file_management_targets(prompt, dir.path(), &messages);
        assert!(
            invalid.is_empty(),
            "invalid targets: {:?}; details: {:?}",
            invalid,
            describe_invalid_file_management_targets(prompt, dir.path(), &messages, &invalid)
        );
    }

    #[test]
    fn expected_file_management_targets_ignore_input_filenames_when_output_is_explicit() {
        let targets = expected_file_management_targets(
            "The emails have been provided to you in the inbox/ folder in your workspace (files named `email_01.txt` through `email_13.txt`). Read all 13 emails and create a triage report saved to `triage_report.md`.",
        );

        assert!(targets
            .iter()
            .any(|group| group.iter().any(|path| path == "triage_report.md")));
        assert!(!targets
            .iter()
            .any(|group| group.iter().any(|path| path == "email_01.txt")));
        assert!(!targets
            .iter()
            .any(|group| group.iter().any(|path| path == "email_13.txt")));
    }

    #[test]
    fn expected_file_management_targets_ignore_named_source_file_when_output_is_explicit() {
        let targets = expected_file_management_targets(
            "Read the document in summary_source.txt and write a concise 3-paragraph summary to summary_output.txt.",
        );

        assert!(targets
            .iter()
            .any(|group| group.iter().any(|path| path == "summary_output.txt")));
        assert!(!targets
            .iter()
            .any(|group| group.iter().any(|path| path == "summary_source.txt")));
    }

    #[test]
    fn missing_file_management_targets_reports_uncreated_relative_files() {
        let dir = tempdir().unwrap();
        let messages = vec![LlmMessage::tool_result(
            "call_1",
            "file_write",
            json!({
                "success": true,
                "path": dir.path().join("README.md").to_string_lossy(),
            }),
        )];

        let missing = missing_file_management_targets(
            "Create README and src/app.py in the current working directory.",
            dir.path(),
            &messages,
        );

        assert_eq!(missing, vec!["src/app.py".to_string()]);
    }

    #[test]
    fn expected_file_management_targets_ignore_telegram_wrapper_metadata() {
        let wrapped = "You are handling a Telegram request through TizenClaw.\n\nTelegram development preferences:\n- Coding backend: codex\n- Coding model: gpt-5.4\n- Project directory: /home/hjhun/samba/github/tizenclaw\n- Coding execution mode: plan\n- Coding auto approve: on\n\nTelegram user request:\ndata/sample/llm_config.json.sample 파일을 열어 active_backend 값이 무엇인지 보여줘.";

        let targets = expected_file_management_targets(wrapped);

        assert!(targets.is_empty());
        assert!(!targets
            .iter()
            .any(|group| group.iter().any(|path| path == "gpt-5.4")));
    }

    #[test]
    fn prompt_policy_auto_selects_ollama_defaults() {
        let doc = json!({"prompt": {"mode": "auto", "reasoning_policy": "auto"}});

        assert_eq!(prompt_mode_from_doc(&doc, "ollama"), PromptMode::Minimal);
        assert_eq!(
            reasoning_policy_from_doc(&doc, "ollama"),
            ReasoningPolicy::Tagged
        );
    }

    #[test]
    fn prompt_policy_auto_selects_hosted_backend_defaults() {
        let doc = json!({"prompt": {"mode": "auto", "reasoning_policy": "auto"}});

        assert_eq!(prompt_mode_from_doc(&doc, "anthropic"), PromptMode::Full);
        assert_eq!(
            reasoning_policy_from_doc(&doc, "anthropic"),
            ReasoningPolicy::Native
        );
    }

    fn sample_agent_role(name: &str, desc: &str) -> AgentRole {
        AgentRole {
            name: name.into(),
            system_prompt: format!("You are {}.", name),
            allowed_tools: Vec::new(),
            max_iterations: 4,
            description: desc.into(),
            role_type: "worker".into(),
            auto_start: false,
            can_delegate_to: Vec::new(),
            prompt_mode: Some(PromptMode::Minimal),
            reasoning_policy: Some(ReasoningPolicy::Native),
        }
    }

    #[test]
    fn role_relevance_score_prefers_matching_roles() {
        let role = sample_agent_role(
            "device_monitor",
            "Monitor battery temperature cpu memory and storage",
        );

        assert!(role_relevance_score("battery temperature status", &role) > 0);
        assert_eq!(role_relevance_score("calendar sync", &role), 0);
    }

    #[test]
    fn select_delegate_roles_falls_back_when_no_keyword_matches() {
        let roles = vec![
            sample_agent_role("device_monitor", "Monitor device health"),
            sample_agent_role("knowledge_retriever", "Search documents"),
        ];

        let selected = select_delegate_roles("completely unrelated request", &roles, 1);

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].name, "device_monitor");
    }
}
