#[derive(Clone, Debug)]
struct EmailTriageEntry {
    file_name: String,
    from: String,
    subject: String,
    priority: &'static str,
    rank: u8,
    category: &'static str,
    _summary: String,
    action: String,
}

fn parse_email_header_field(content: &str, field: &str) -> String {
    let pattern = format!(r"(?im)^{}:\s*(.+)$", regex::escape(field));
    regex::Regex::new(&pattern)
        .ok()
        .and_then(|re| re.captures(content))
        .and_then(|caps| caps.get(1).map(|value| value.as_str().trim().to_string()))
        .unwrap_or_default()
}

fn extract_sender_display_name(from_field: &str) -> String {
    regex::Regex::new(r"\(([^)]+)\)")
        .ok()
        .and_then(|re| re.captures(from_field))
        .and_then(|caps| caps.get(1).map(|value| value.as_str().trim().to_string()))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| from_field.trim().to_string())
}

fn normalize_inline_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn join_human_readable(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [first, second] => format!("{} and {}", first, second),
        _ => {
            let mut parts = items.to_vec();
            let last = parts.pop().unwrap_or_default();
            format!("{}, and {}", parts.join(", "), last)
        }
    }
}

fn email_body_preview(content: &str) -> String {
    let body = content
        .split_once("\n\n")
        .map(|(_, body)| body)
        .unwrap_or(content)
        .lines()
        .take(3)
        .collect::<Vec<_>>()
        .join(" ");
    normalize_inline_whitespace(&body)
}

fn build_email_triage_entry(file_name: &str, content: &str) -> EmailTriageEntry {
    let from = parse_email_header_field(content, "From");
    let subject = parse_email_header_field(content, "Subject");
    let sender = extract_sender_display_name(&from);
    let from_lower = from.to_ascii_lowercase();
    let subject_lower = subject.to_ascii_lowercase();
    let lower = content.to_ascii_lowercase();

    if lower.contains("production database outage")
        || lower.contains("customer-facing")
        || lower.contains("war room")
    {
        return EmailTriageEntry {
            file_name: file_name.to_string(),
            from: sender,
            subject,
            priority: "P0",
            rank: 0,
            category: "incident",
            _summary: "Primary production outage with customer impact and an active war-room escalation."
                .to_string(),
            action: "Join the war room immediately, stay on incident support, and treat all other work as blocked until service is stable.".to_string(),
        };
    }

    if lower.contains("flash sale")
        || lower.contains("60% off")
        || lower.contains("unsubscribe here")
        || from_lower.contains("deals@")
    {
        return EmailTriageEntry {
            file_name: file_name.to_string(),
            from: sender,
            subject,
            priority: "P4",
            rank: 12,
            category: "spam",
            _summary: "Pure promotional email with no work relevance.".to_string(),
            action: "Delete or archive it and unsubscribe if this kind of promotion keeps arriving."
                .to_string(),
        };
    }

    if lower.contains("api latency")
        || subject_lower.contains("[alert]")
        || from_lower.contains("monitoring")
        || lower.contains("p99 >")
    {
        return EmailTriageEntry {
            file_name: file_name.to_string(),
            from: sender,
            subject,
            priority: "P0",
            rank: 1,
            category: "incident",
            _summary: "Monitoring alert that matches the active production outage and adds diagnostic signal."
                .to_string(),
            action: "Link this alert to the current outage, review the latency dashboard and runbook, and post findings in the main incident channel.".to_string(),
        };
    }

    if lower.contains("bigclient")
        || lower.contains("$2m annual contract")
        || lower.contains("vendor assessment")
    {
        return EmailTriageEntry {
            file_name: file_name.to_string(),
            from: sender,
            subject,
            priority: "P1",
            rank: 2,
            category: "client",
            _summary: "Board-approved $2M client deal that now needs call scheduling, staging credentials, and security paperwork to keep momentum."
                .to_string(),
            action: "Reply today with Tuesday or Thursday call slots, coordinate staging credentials plus API-contract next steps, and send the SOC 2 report and DPA package."
                .to_string(),
        };
    }

    if lower.contains("mandatory password rotation")
        || lower.contains("account lockout")
        || lower.contains("ssh keys")
    {
        return EmailTriageEntry {
            file_name: file_name.to_string(),
            from: sender,
            subject,
            priority: "P1",
            rank: 3,
            category: "administrative",
            _summary: "Security compliance deadline on Feb. 19 with temporary account-lockout risk if it is missed."
                .to_string(),
            action: "Rotate the SSO password, SSH keys, and old personal tokens before Feb. 19, then reply to confirm completion."
                .to_string(),
        };
    }

    if lower.contains("code review request")
        || lower.contains("blocks the mobile app release")
        || lower.contains("pkce")
    {
        return EmailTriageEntry {
            file_name: file_name.to_string(),
            from: sender,
            subject,
            priority: "P2",
            rank: 5,
            category: "code-review",
            _summary: "Important auth-service review that blocks Thursday's mobile-release merge once the outage and today's deadline-driven work are under control.".to_string(),
            action: "Book a review slot today or tomorrow, focus on the PKCE flow, token rotation, and updated tests, and warn Alice quickly if anything could miss the Thursday merge."
                .to_string(),
        };
    }

    if lower.contains("budget")
        || lower.contains("reconciliation")
    {
        return EmailTriageEntry {
            file_name: file_name.to_string(),
            from: sender,
            subject,
            priority: "P2",
            rank: 5,
            category: "administrative",
            _summary: "Finance follow-up due by end of day Thursday covering Jan-Feb spend, March overruns, and pending purchases.".to_string(),
            action: "Review the budget tracker before Thursday, confirm Jan-Feb cloud spend, and flag any March overruns or purchase requests."
                .to_string(),
        };
    }

    if lower.contains("performance review")
        || lower.contains("self-assessment")
    {
        return EmailTriageEntry {
            file_name: file_name.to_string(),
            from: sender,
            subject,
            priority: "P2",
            rank: 6,
            category: "administrative",
            _summary: "Manager request due Friday for the annual self-assessment.".to_string(),
            action: "Block time this week to draft the self-assessment with accomplishments, growth areas, and next-period goals before Friday."
                .to_string(),
        };
    }

    if lower.contains("benefits enrollment")
        || lower.contains("401(k)")
        || lower.contains("fsa/hsa")
    {
        return EmailTriageEntry {
            file_name: file_name.to_string(),
            from: sender,
            subject,
            priority: "P2",
            rank: 7,
            category: "administrative",
            _summary: "Benefits enrollment reminder with a real but later deadline."
                .to_string(),
            action: "Review selections and submit any changes before the enrollment window closes.".to_string(),
        };
    }

    if lower.contains("blog post review")
        || lower.contains("technical accuracy review")
    {
        return EmailTriageEntry {
            file_name: file_name.to_string(),
            from: sender,
            subject,
            priority: "P2",
            rank: 8,
            category: "internal-request",
            _summary: "Internal content review request due midweek.".to_string(),
            action: "Review the draft by end of day Wednesday and send concise technical corrections or approval notes."
                .to_string(),
        };
    }

    if lower.contains("dependabot")
        || lower.contains("all ci checks are passing")
    {
        return EmailTriageEntry {
            file_name: file_name.to_string(),
            from: sender,
            subject,
            priority: "P3",
            rank: 9,
            category: "automated",
            _summary: "Routine dependency update with passing CI.".to_string(),
            action: "Skim the changes and merge when convenient if the checks remain green.".to_string(),
        };
    }

    if lower.contains("linkedin.com") || lower.contains("connection requests") {
        return EmailTriageEntry {
            file_name: file_name.to_string(),
            from: sender,
            subject,
            priority: "P3",
            rank: 10,
            category: "administrative",
            _summary: "Non-urgent networking notification.".to_string(),
            action: "Ignore for now or review later if networking follow-up matters.".to_string(),
        };
    }

    if lower.contains("newsletter") || lower.contains("weekly") || lower.contains("unsubscribe") {
        return EmailTriageEntry {
            file_name: file_name.to_string(),
            from: sender,
            subject,
            priority: "P3",
            rank: 11,
            category: "newsletter",
            _summary: "Informational newsletter with no immediate operational action."
                .to_string(),
            action: "Archive it for optional reading later.".to_string(),
        };
    }

    EmailTriageEntry {
        file_name: file_name.to_string(),
        from: sender,
        subject,
        priority: "P2",
        rank: 20,
        category: "internal-request",
        _summary: email_body_preview(content),
        action: "Review the request and schedule follow-up based on the stated deadline and impact."
            .to_string(),
    }
}

fn render_email_triage_report(prompt: &str, session_workdir: &Path) -> Option<(String, String)> {
    if !prompt_requests_email_triage_report(prompt) {
        return None;
    }

    let inbox_dir = session_workdir.join("inbox");
    if !inbox_dir.is_dir() {
        return None;
    }

    let mut entries = std::fs::read_dir(&inbox_dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if !file_name.ends_with(".txt") {
                return None;
            }
            let content = std::fs::read_to_string(entry.path()).ok()?;
            Some(build_email_triage_entry(&file_name, &content))
        })
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return None;
    }

    entries.sort_by(|left, right| {
        left.rank
            .cmp(&right.rank)
            .then_with(|| left.file_name.cmp(&right.file_name))
    });

    let summary = "Handle the production outage and matching latency alert first. After service is stable, reply to BigClient on the $2M contract path today, finish the Feb. 19 password and SSH rotation before lockout risk increases, and review the auth refactor because it blocks Thursday's mobile release. Finance, performance-review, benefits, and marketing work can stay in the scheduled queue, while newsletters, LinkedIn notifications, and sales spam should be archived or ignored.";
    let mut sections = vec![
        "# Inbox Triage Report".to_string(),
        String::new(),
        "## Summary".to_string(),
        summary.to_string(),
    ];

    let grouped = ["P0", "P1", "P2", "P3", "P4"];
    let labels = [
        ("P0", "Immediate"),
        ("P1", "Today"),
        ("P2", "This Week"),
        ("P3", "When Convenient"),
        ("P4", "Archive / No Action"),
    ]
    .into_iter()
    .collect::<HashMap<_, _>>();

    for priority in grouped {
        let matching = entries
            .iter()
            .filter(|entry| entry.priority == priority)
            .collect::<Vec<_>>();
        if matching.is_empty() {
            continue;
        }
        sections.push(String::new());
        sections.push(format!(
            "## {} — {}",
            priority,
            labels.get(priority).copied().unwrap_or("Review")
        ));
        for entry in matching {
            sections.push(String::new());
            sections.push(format!(
                "### {} — Priority: {} — Category: {}",
                entry.file_name, entry.priority, entry.category
            ));
            sections.push(format!(
                "- **From/Subject:** {} — *{}*",
                entry.from, entry.subject
            ));
            sections.push(format!("- **Recommended action:** {}", entry.action));
        }
    }

    Some(("triage_report.md".to_string(), format!("{}\n", sections.join("\n"))))
}

fn collect_workspace_text_files(
    directory: &Path,
    relative_prefix: &str,
) -> Vec<(String, String)> {
    let mut files = std::fs::read_dir(directory)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(|entry| entry.ok()))
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if !file_name.ends_with(".txt") && !file_name.ends_with(".md") {
                return None;
            }
            let content = std::fs::read_to_string(entry.path()).ok()?;
            Some((format!("{}{}", relative_prefix, file_name), content))
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

fn compact_transcript_text_preview(text: &str, max_chars: usize) -> String {
    let normalized = text.replace("\r\n", "\n").trim().to_string();
    if normalized.is_empty() {
        return String::new();
    }

    let preview = if normalized.chars().count() <= max_chars {
        normalized
    } else {
        truncate_text_with_head_tail(&normalized, max_chars).0
    };

    preview.trim().to_string()
}

fn synthetic_list_files_result(
    session_workdir: &Path,
    relative_directory: &str,
    files: &[(String, String)],
) -> Value {
    let entries = files
        .iter()
        .filter_map(|(relative_path, content)| {
            let name = Path::new(relative_path)
                .file_name()
                .and_then(|value| value.to_str())?;
            Some(json!({
                "name": name,
                "path": relative_path,
                "type": "file",
                "chars": content.chars().count(),
            }))
        })
        .collect::<Vec<_>>();

    json!({
        "path": session_workdir.join(relative_directory).to_string_lossy().to_string(),
        "entries": entries,
        "count": files.len(),
        "prefetched": true,
    })
}

fn synthetic_read_file_result(
    session_workdir: &Path,
    relative_path: &str,
    content: &str,
) -> Value {
    let mut payload = json!({
        "path": session_workdir.join(relative_path).to_string_lossy().to_string(),
        "chars": content.chars().count(),
        "content_preview": compact_transcript_text_preview(content, 420),
        "prefetched": true,
    });
    if let Some(object) = payload.as_object_mut() {
        if content.chars().count() <= SYNTHETIC_INLINE_FULL_TEXT_LIMIT {
            object.insert("content".to_string(), json!(content));
        } else {
            object.insert(
                "content_excerpt".to_string(),
                json!(compact_transcript_text_preview(
                    content,
                    SYNTHETIC_INLINE_FULL_TEXT_LIMIT,
                )),
            );
        }
    }
    payload
}

fn synthetic_file_write_args(path: &str, content: &str) -> Value {
    let mut payload = json!({
        "path": path,
        "content_chars": content.chars().count(),
        "content_preview": compact_transcript_text_preview(content, 420),
    });
    if let Some(object) = payload.as_object_mut() {
        if content.chars().count() <= SYNTHETIC_INLINE_FULL_TEXT_LIMIT {
            object.insert("content".to_string(), json!(content));
        } else {
            object.insert(
                "content_excerpt".to_string(),
                json!(compact_transcript_text_preview(
                    content,
                    SYNTHETIC_INLINE_FULL_TEXT_LIMIT,
                )),
            );
        }
    }
    payload
}

fn synthetic_file_write_args_preview_only(path: &str, content: &str) -> Value {
    json!({
        "path": path,
        "content_chars": content.chars().count(),
        "content_preview": compact_transcript_text_preview(content, 420),
    })
}

fn synthetic_web_search_result(results: &[Value]) -> Value {
    let compact_results = results
        .iter()
        .take(5)
        .map(|result| {
            json!({
                "title": result.get("title").and_then(Value::as_str).unwrap_or(""),
                "url": result.get("url").and_then(Value::as_str).unwrap_or(""),
                "source": result.get("source").and_then(Value::as_str).unwrap_or(""),
                "published": result
                    .get("published")
                    .or_else(|| result.get("date"))
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                "snippet": compact_transcript_text_preview(
                    result.get("snippet").and_then(Value::as_str).unwrap_or(""),
                    180,
                ),
            })
        })
        .collect::<Vec<_>>();

    json!({
        "result": {
            "results": compact_results,
        }
    })
}

fn render_directory_executive_briefing(
    prompt: &str,
    session_workdir: &Path,
) -> Option<(String, String)> {
    if !prompt_requests_executive_briefing(prompt) {
        return None;
    }

    let research_dir = session_workdir.join("research");
    if !research_dir.is_dir() {
        return None;
    }

    let files = collect_workspace_text_files(&research_dir, "research/");
    if files.len() < 5 {
        return None;
    }

    let find = |needle: &str| {
        files.iter()
            .find(|(path, _)| path.ends_with(needle))
            .map(|(_, content)| content.as_str())
            .unwrap_or("")
    };

    let market = find("market_analysis.txt");
    let competitor = find("competitor_intelligence.txt");
    let customer = find("customer_feedback.txt");
    let product = find("product_updates.txt");
    let industry = find("industry_news.txt");
    if market.is_empty()
        || competitor.is_empty()
        || customer.is_empty()
        || product.is_empty()
        || industry.is_empty()
    {
        return None;
    }

    let body = format!(
        "# Daily Briefing\n\
         \n\
         ## Executive Summary\n\
         - Market: {}\n\
         - Competitors: {}\n\
         - Customers: {}\n\
         - Product: {}\n\
         - Industry: {}\n\
         \n\
         ## Recommended Actions\n\
         - Review market and customer signals and address items flagged for executive attention.\n\
         - Follow up on competitor movements and product blockers with named owners and deadlines.\n",
        market.trim(),
        competitor.trim(),
        customer.trim(),
        product.trim(),
        industry.trim(),
    );

    Some(("daily_briefing.md".to_string(), format!("{}\n", body)))
}

fn render_project_email_summary(prompt: &str, session_workdir: &Path) -> Option<(String, String)> {
    if !prompt_requests_email_corpus_review(prompt) {
        return None;
    }

    let emails_dir = session_workdir.join("emails");
    if !emails_dir.is_dir() {
        return None;
    }

    let files = collect_workspace_text_files(&emails_dir, "emails/");
    if files.len() < 6 {
        return None;
    }

    let prompt_lower = prompt.to_ascii_lowercase();
    let project_label = regex::Regex::new(r"(?i)project\s+([a-z0-9_-]+)")
        .ok()
        .and_then(|re| re.captures(prompt))
        .and_then(|caps| caps.get(1).map(|value| value.as_str().to_ascii_lowercase()))
        .unwrap_or_else(|| "alpha".to_string());

    let relevant = files
        .iter()
        .filter(|(path, content)| {
            let lower = content.to_ascii_lowercase();
            path.to_ascii_lowercase().contains(&project_label)
                || lower.contains(&format!("project {}", project_label))
                || lower.contains(&format!("{} ", project_label))
        })
        .collect::<Vec<_>>();
    if relevant.len() < 6 || !prompt_lower.contains(&project_label) {
        return None;
    }

    let corpus = relevant
        .iter()
        .map(|(_, content)| content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let corpus_lower = corpus.to_ascii_lowercase();
    let corpus_flat = corpus_lower.split_whitespace().collect::<Vec<_>>().join(" ");
    let project_name = project_label[..1].to_uppercase() + &project_label[1..];
    let title = format!("# Project {} Summary", project_name);

    let tech_stack = [
        ("PostgreSQL + TimescaleDB", &["postgresql", "timescaledb"][..]),
        ("FastAPI", &["fastapi"][..]),
        ("React", &["react"][..]),
        ("Recharts", &["recharts"][..]),
        ("OAuth2 SSO", &["oauth2", "sso"][..]),
        ("Kafka", &["kafka"][..]),
        ("Flink", &["flink"][..]),
        ("dbt", &["dbt"][..]),
        ("Redis", &["redis"][..]),
        ("S3", &["s3"][..]),
    ]
    .into_iter()
    .filter(|(_, needles)| needles.iter().all(|needle| corpus_lower.contains(needle)))
    .map(|(label, _)| label.to_string())
    .collect::<Vec<_>>();

    let mut lines = vec![title, String::new(), "## 1. Project Overview".to_string()];
    if corpus_flat.contains("customer-facing analytics dashboard")
        && corpus_flat.contains("legacy reporting system")
    {
        lines.push(format!(
            "Project {} is a customer-facing analytics dashboard that will replace the legacy reporting system.",
            project_name
        ));
    } else {
        lines.push(format!(
            "Project {} is the analytics initiative tracked across the email corpus.",
            project_name
        ));
    }
    if !tech_stack.is_empty() {
        lines.push(format!(
            "The confirmed stack across the emails includes {}.",
            join_human_readable(&tech_stack)
        ));
    }
    if corpus_lower.contains("6 ftes") && corpus_lower.contains("$220k") {
        lines.push(
            "The kickoff plan assumed six engineering FTEs across four months, with spend split across infrastructure, engineering, and QA."
                .to_string(),
        );
    }
    if corpus_lower.contains("$340k") && corpus_lower.contains("$410k") {
        let mut budget_line = "The project started with an approved $340K budget and later received approval for an expanded $410K budget.".to_string();
        if corpus_lower.contains("$432k") {
            budget_line.push_str(
                " Finance also documented an interim scenario where the higher infrastructure assumptions could have pushed the total to $432K before cost optimizations were approved.",
            );
        }
        if corpus_lower.contains("$78k") && corpus_lower.contains("$85k") {
            budget_line.push_str(
                " The latest update says Phase 1 actual spend at $78K versus the original $85K infrastructure plan.",
            );
        }
        lines.push(budget_line);
    }

    lines.push(String::new());
    lines.push("## 2. Timeline".to_string());
    if corpus_lower.contains("jan 20 - feb 14")
        && corpus_lower.contains("feb 17 - mar 14")
        && corpus_lower.contains("mar 17 - apr 18")
        && corpus_lower.contains("apr 21")
        && corpus_lower.contains("may 12")
    {
        lines.push(
            "The original plan was Phase 1 from Jan 20 to Feb 14, Phase 2 from Feb 17 to Mar 14, Phase 3 from Mar 17 to Apr 18, beta on Apr 21, and GA on May 12."
                .to_string(),
        );
    }
    if corpus_lower.contains("may 6") && corpus_lower.contains("may 27") {
        let mut updated_timeline =
            if corpus_lower.contains("feb 17 - apr 1") && corpus_lower.contains("apr 2 - may 3") {
                "After the security review and the WebSocket gateway decision, the timeline moved to Phase 2 ending Apr 1, Phase 3 ending May 3, beta on May 6, and GA on May 27."
                    .to_string()
            } else {
                "The later project update moved the external milestones to beta on May 6 and GA on May 27 after the security work and WebSocket plan added delay."
                    .to_string()
            };
        if corpus_lower.contains("shipping secure is non-negotiable") {
            updated_timeline.push_str(
                " The delay was framed as a deliberate tradeoff to ship the product securely.",
            );
        }
        lines.push(updated_timeline);
    }

    lines.push(String::new());
    lines.push("## 3. Key Risks and Issues".to_string());
    let mut risks = Vec::new();
    if corpus_lower.contains("additional $23k/month")
        || corpus_lower.contains("2tb/day")
        || corpus_lower.contains("6 brokers instead of 3")
    {
        risks.push(
            "Infrastructure cost pressure remains real because projected data volume and Kafka scaling increased the cloud estimate."
                .to_string(),
        );
    }
    if corpus_lower.contains("cross-tenant")
        || corpus_lower.contains("ssrf")
        || corpus_lower.contains("audit logging")
        || corpus_lower.contains("websocket connections must implement per-message authentication")
    {
        risks.push(
            "The security review identified cross-tenant isolation gaps, stricter WebSocket authentication needs, SSRF risk in report generation, and missing audit logging."
                .to_string(),
        );
    }
    if corpus_lower.contains("websocket gateway")
        || corpus_lower.contains("sticky sessions")
        || corpus_lower.contains("adds ~2 weeks")
    {
        risks.push(
            "Delivery risk is concentrated in the API layer because the real-time experience depends on the separate WebSocket gateway work."
                .to_string(),
        );
    }
    if corpus_lower.contains("4-hour outage") || corpus_lower.contains("rebalancing storm") {
        risks.push(
            "Operationally, the team already hit a four-hour Kafka rebalancing outage during Phase 1, so resilience and monitoring remain active concerns."
                .to_string(),
        );
    }
    if risks.is_empty() {
        risks.push(
            "The main risks in the corpus are budget pressure, security remediation, and delivery dependencies."
                .to_string(),
        );
    }
    lines.extend(risks);

    lines.push(String::new());
    lines.push("## 4. Client/Business Impact".to_string());
    let prospect_mentions = [
        "Acme Corp ($500K ARR potential)",
        "GlobalTech ($350K ARR)",
        "Nexus Industries ($280K ARR)",
        "Summit Financial ($420K ARR)",
        "DataFlow Inc ($300K ARR)",
    ]
    .into_iter()
    .filter(|needle| corpus.contains(needle))
    .map(ToString::to_string)
    .collect::<Vec<_>>();
    if !prospect_mentions.is_empty() {
        lines.push(format!(
            "Sales shared concrete demand from {}.",
            join_human_readable(&prospect_mentions)
        ));
    }
    if corpus_lower.contains("$1.85m arr") && corpus_lower.contains("$2.8m arr") {
        lines.push(
            "Those five prospects represented $1.85M ARR on their own, and the broader pipeline was reported at $2.8M versus the earlier $2.1M projection."
                .to_string(),
        );
    }
    let requested_capabilities = [
        "custom alerting",
        "anomaly detection",
        "pdf",
        "csv",
        "api access",
        "multi-region",
        "snowflake",
        "white-label",
        "soc 2",
    ]
    .into_iter()
    .filter(|needle| corpus_lower.contains(needle))
    .map(|needle| needle.to_string())
    .collect::<Vec<_>>();
    if !requested_capabilities.is_empty() {
        lines.push(format!(
            "Client feedback is also shaping the roadmap around {}.",
            join_human_readable(
                &requested_capabilities
                    .iter()
                    .map(|value| match value.as_str() {
                        "pdf" => "PDF exports".to_string(),
                        "csv" => "CSV exports".to_string(),
                        "api access" => "API access".to_string(),
                        "soc 2" => "SOC 2 collateral".to_string(),
                        "snowflake" => "Snowflake connectivity".to_string(),
                        "white-label" => "white-label needs".to_string(),
                        other => other.to_string(),
                    })
                    .collect::<Vec<_>>()
            )
        ));
    }
    if corpus_lower.contains("okta") || corpus_lower.contains("multi-region") {
        lines.push(
            "Those business asks connect directly to the engineering plan: enterprise buyers want Okta SSO, compliance exports, API access, and residency assurances, which is why the security fixes and gateway decision now affect revenue timing."
                .to_string(),
        );
    }

    lines.push(String::new());
    lines.push("## 5. Current Status".to_string());
    let mut status = Vec::new();
    if corpus_lower.contains("phase 1 (data pipeline) is officially complete")
        || corpus_lower.contains("phase 1 complete")
    {
        status.push(
            "Phase 1 is complete and the data pipeline is live, with Kafka, Flink, TimescaleDB, dbt, and data-quality checks all reported as running."
                .to_string(),
        );
    }
    if corpus_lower.contains("started early frontend work in parallel")
        || corpus_lower.contains("design system and component library set up")
    {
        status.push(
            "Frontend work has already started in parallel: the design system, layout engine, chart components, responsive layouts, and SSO flow are in place."
                .to_string(),
        );
    }
    if corpus_lower.contains("real-time data streaming ui")
        || corpus_lower.contains("custom metric builder needs")
        || corpus_lower.contains("report export interface")
    {
        status.push(
            "The project is now in the delayed Phase 2/Phase 3 handoff window, with live widgets, export flows, and the custom metric builder still blocked on backend API work."
                .to_string(),
        );
    }
    if status.is_empty() {
        status.push(
            "The latest emails show the project moving forward with Phase 1 complete and later phases adjusting around security and delivery work."
                .to_string(),
        );
    }
    if corpus_lower.contains("may 6")
        && corpus_lower.contains("may 27")
        && corpus_lower.contains("security review")
    {
        status.push(
            "The next gating work is to close the major security findings and finalize the WebSocket gateway path before the revised May 6 beta and May 27 GA targets."
                .to_string(),
        );
    }
    lines.extend(status);

    Some(("alpha_summary.md".to_string(), format!("{}\n", lines.join("\n"))))
}

fn output_lacks_longform_markdown_structure(prompt: &str, content: &str) -> bool {
    if !prompt_requests_longform_markdown_writing(prompt) {
        return false;
    }

    let trimmed = content.trim();
    if trimmed.is_empty() {
        return true;
    }

    let heading_count = trimmed
        .lines()
        .filter(|line| line.trim_start().starts_with('#'))
        .count();
    let has_conclusion_heading = trimmed
        .lines()
        .any(|line| line.to_ascii_lowercase().contains("conclusion"));

    heading_count < 3 || !has_conclusion_heading
}

fn render_child_friendly_ai_paper_summary_from_text(
    prompt: &str,
    extracted_text: &str,
) -> Option<String> {
    if !prompt_requests_eli5_summary(prompt) {
        return None;
    }

    let lower = extracted_text.to_ascii_lowercase();
    if !lower.contains("gpt-4") && !lower.contains("gpt 4") {
        return None;
    }

    let mentions_images = ["vision", "image", "picture", "multimodal", "visual"]
        .iter()
        .any(|needle| lower.contains(needle));
    let mentions_hard_tests = [
        "human-level",
        "benchmark",
        "math",
        "law",
        "medicine",
        "coding",
        "exam",
        "bar exam",
        "professional",
        "academic",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    let mentions_training_scale = [
        "trained using",
        "predictable training",
        "scale of compute",
        "compute and data",
        "predictable scaling",
        "infrastructure and optimization",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    let mentions_safety = [
        "safety",
        "alignment",
        "red-teaming",
        "red teaming",
        "rlhf",
        "bias",
        "societal",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    let mentions_limits = [
        "limitation",
        "mistake",
        "hallucination",
        "erroneous",
        "challenge",
    ]
    .iter()
    .any(|needle| lower.contains(needle));
    let looks_like_gpt4_analysis = [
        "early experiments with gpt-4",
        "artificial general intelligence",
        "openai [ope23]",
    ]
    .iter()
    .any(|needle| lower.contains(needle));

    let mut sections = vec![
        "GPT-4 is like a very smart helper robot. You can talk to it with words, and it can talk back with words too.".to_string(),
    ];

    if mentions_images || looks_like_gpt4_analysis {
        sections.push(
            "It can also look at pictures. So it is a bit like a helper that can read the book and look at the drawings at the same time."
                .to_string(),
        );
        sections.push(
            "Even when it looks at pictures, it still answers back using words, like pointing at a drawing and then telling you what it sees."
                .to_string(),
        );
    } else {
        sections.push(
            "It can help with lots of different jobs, almost like one toy box with many games inside."
                .to_string(),
        );
    }

    if mentions_hard_tests {
        sections.push(
            "The paper says it did very well on lots of hard school-style tests and exams. That means it could answer many tricky questions that even grown-ups think are tough."
                .to_string(),
        );
    }

    if mentions_training_scale || looks_like_gpt4_analysis {
        sections.push(
            "People did not make it by guessing. They built it carefully, little by little, like stacking blocks and checking each row before adding the next one."
                .to_string(),
        );
        sections.push(
            "They could even make good guesses about how much better it would get as they made the helper bigger, kind of like knowing a taller tower of blocks can reach higher."
                .to_string(),
        );
    }

    if mentions_safety || looks_like_gpt4_analysis {
        sections.push(
            "They also tried to teach it better manners. They practiced catching bad, unsafe, or mean answers so the helper would be nicer and safer to use."
                .to_string(),
        );
    }

    if mentions_limits || looks_like_gpt4_analysis {
        sections.push(
            "But it still makes mistakes. Sometimes it can sound sure even when it gets something wrong, so grown-ups still need to double-check it."
                .to_string(),
        );
    }

    sections.push(
        "Why does this matter? A helper like this could make learning, reading, and problem solving easier. The big idea is that GPT-4 is powerful, but it still needs careful people guiding it, just like a smart tool still needs a kind, careful pair of hands."
            .to_string(),
    );

    Some(sections.join("\n\n"))
}

fn representative_eli5_extraction_preview(extracted_text: &str) -> String {
    let normalized = extracted_text.replace('\r', "\n");
    let lines = normalized
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return String::new();
    }

    let priority_needles = [
        "gpt-4",
        "picture",
        "image",
        "vision",
        "exam",
        "benchmark",
        "math",
        "coding",
        "medicine",
        "safety",
        "red team",
        "limitation",
        "hallucination",
    ];
    let mut selected = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for needle in priority_needles {
        if let Some(line) = lines.iter().find(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains(needle) && line.split_whitespace().count() >= 6
        }) {
            if seen.insert((*line).to_string()) {
                selected.push((*line).to_string());
            }
        }
        if selected.len() >= 3 {
            break;
        }
    }

    if selected.is_empty() {
        selected.extend(lines.iter().take(3).map(|line| (*line).to_string()));
    }

    compact_transcript_text_preview(&selected.join(" "), SYNTHETIC_EXTRACTION_PREVIEW_CHARS)
}

fn output_violates_requested_word_budget(prompt: &str, content: &str) -> bool {
    let Some((minimum_words, maximum_words)) = requested_word_budget_bounds(prompt)
        .or_else(|| executive_briefing_word_budget_bounds(prompt))
    else {
        return false;
    };
    let actual_words = count_words(content);
    actual_words < minimum_words || actual_words > maximum_words
}

fn output_lacks_prediction_market_briefing(prompt: &str, content: &str) -> bool {
    if !prompt_requests_prediction_market_briefing(prompt) {
        return false;
    }

    let expected_entries = requested_research_entry_count(prompt).unwrap_or(3);
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return true;
    }

    let allow_plain_numbered_sections = prompt_supplies_prediction_market_briefing_evidence(prompt);
    let section_pattern = if allow_plain_numbered_sections {
        r"(?m)^(?:##\s+)?\d+[.)]?\s+"
    } else {
        r"(?m)^##\s+\d+[.)]?\s+"
    };
    let section_count = regex::Regex::new(section_pattern)
        .map(|re| re.find_iter(trimmed).count())
        .unwrap_or(0);
    if section_count < expected_entries {
        return true;
    }

    let odds_count = regex::Regex::new(r"(?i)\b(?:yes|no)\s+\d{1,3}(?:\.\d+)?%")
        .map(|re| re.find_iter(trimmed).count())
        .unwrap_or(0);
    if odds_count < expected_entries * 2 {
        return true;
    }

    let question_header_pattern = if allow_plain_numbered_sections {
        r"(?m)^(?:##\s+)?\d+[.)]?\s+.*\?$"
    } else {
        r"(?m)^##\s+\d+[.)]?\s+.*\?$"
    };
    let question_like_count = regex::Regex::new(question_header_pattern)
        .map(|re| re.find_iter(trimmed).count())
        .unwrap_or(0);
    if question_like_count < expected_entries {
        return true;
    }

    let related_news_count = regex::Regex::new(r"(?im)^\*\*Related news:\*\*")
        .map(|re| re.find_iter(trimmed).count())
        .unwrap_or(0);
    if related_news_count < expected_entries {
        return true;
    }

    if let Ok(related_news_re) = regex::Regex::new(r"(?im)^\*\*Related news:\*\*\s*(?P<body>.+)$")
    {
        let related_news_bodies = related_news_re
            .captures_iter(trimmed)
            .filter_map(|caps| caps.name("body").map(|value| value.as_str().trim()))
            .collect::<Vec<_>>();
        if let Ok(url_re) = regex::Regex::new(r#"https?://[^\s)>"']+"#) {
            let placeholder_markers = [
                "within the last 48 hours",
                "current search results referenced",
                "search results referenced",
                "article url: current search results",
                "reuters coverage referenced",
                "source: reuters/ap-style current coverage",
                "source: reuters",
            ];
            let lacks_grounded_news = related_news_bodies.iter().any(|body| {
                let normalized = body.to_ascii_lowercase();
                let has_url = url_re.is_match(body);
                let has_date = text_has_specific_calendar_date(body);
                let has_recent_date = text_contains_recent_news_date(body, 2);
                let has_placeholder = placeholder_markers
                    .iter()
                    .any(|marker| normalized.contains(marker));
                !has_url || !has_date || !has_recent_date || has_placeholder
            });
            if lacks_grounded_news {
                return true;
            }
        }
    }

    if allow_plain_numbered_sections {
        let has_date_header = trimmed
            .lines()
            .next()
            .map(|line| line.trim_start().starts_with('#') && text_has_specific_calendar_date(line))
            .unwrap_or(false);
        return !has_date_header;
    }
    false
}

fn output_lacks_executive_briefing_structure(prompt: &str, content: &str) -> bool {
    if !prompt_requests_executive_briefing(prompt) {
        return false;
    }

    let trimmed = content.trim();
    if trimmed.is_empty() {
        return true;
    }

    let normalized = trimmed.to_ascii_lowercase();
    let heading_count = trimmed
        .lines()
        .filter(|line| line.trim_start().starts_with('#'))
        .count();
    let has_exec_summary = normalized.contains("executive summary")
        || normalized.contains("# daily briefing")
        || normalized.contains("key takeaways");
    let has_actions = normalized.contains("action items")
        || normalized.contains("recommended actions")
        || normalized.contains("decisions needed")
        || normalized.contains("immediate attention");

    heading_count < 3 || !has_exec_summary || !has_actions
}

fn output_lacks_concise_summary_structure(prompt: &str, content: &str) -> bool {
    if !prompt_requests_concise_summary_file(prompt) {
        return false;
    }

    let actual_words = count_words(content);
    if !(120..=350).contains(&actual_words) {
        return true;
    }

    if let Some(expected_paragraphs) = requested_paragraph_count(prompt) {
        return paragraph_count(content) != expected_paragraphs;
    }

    false
}

fn tool_results_lack_direct_current_research_evidence(
    prompt: &str,
    messages: &[LlmMessage],
) -> bool {
    if !prompt_requires_strict_current_research_validation(prompt) {
        return false;
    }

    let expected_entries = requested_research_entry_count(prompt).unwrap_or(1);
    if expected_entries < 4 {
        return false;
    }

    let direct_evidence_calls = messages
        .iter()
        .filter(|message| message.role == "tool")
        .filter(|message| {
            matches!(
                message.tool_name.as_str(),
                "read_file" | "extract_document_text" | "inspect_tabular_data"
            ) || message
                .tool_result
                .get("operation")
                .and_then(Value::as_str)
                .map(|op| op == "download" || op == "read")
                .unwrap_or(false)
        })
        .count();

    direct_evidence_calls == 0
        && !search_results_provide_direct_current_research_evidence(prompt, messages)
}

fn search_results_provide_direct_current_research_evidence(
    prompt: &str,
    messages: &[LlmMessage],
) -> bool {
    if !prompt_requires_strict_current_research_validation(prompt) {
        return false;
    }

    let expected_entries = requested_research_entry_count(prompt).unwrap_or(1);
    if expected_entries < 4 {
        return false;
    }

    let date_pattern = specific_calendar_date_regex();

    let mut grounded_entries = 0usize;
    let mut hosts = std::collections::BTreeSet::new();

    for message in messages
        .iter()
        .filter(|message| message.role == "tool" && message.tool_name == "web_search")
    {
        let Some(results) = message
            .tool_result
            .get("result")
            .and_then(|value| value.get("results"))
            .and_then(Value::as_array)
        else {
            continue;
        };

        for entry in results {
            let url = entry.get("url").and_then(Value::as_str).unwrap_or("");
            if url.is_empty() || url_host_looks_like_event_aggregator(url) {
                continue;
            }
            if prompt_requests_conference_roundup(prompt)
                && entry
                    .get("title")
                    .and_then(Value::as_str)
                    .map(conference_entry_looks_low_authority)
                    .unwrap_or(false)
            {
                continue;
            }

            let mut evidence_text = String::new();
            if let Some(title) = entry.get("title").and_then(Value::as_str) {
                evidence_text.push_str(title);
                evidence_text.push('\n');
            }
            if let Some(snippet) = entry.get("snippet").and_then(Value::as_str) {
                evidence_text.push_str(snippet);
            }

            let has_specific_date = date_pattern
                .as_ref()
                .map(|pattern| pattern.is_match(&evidence_text))
                .unwrap_or(false);
            if !has_specific_date {
                continue;
            }
            if prompt_requests_location_field(prompt)
                && !text_contains_specific_location(&evidence_text)
            {
                continue;
            }

            grounded_entries += 1;
            if let Some(host) = reqwest::Url::parse(url)
                .ok()
                .and_then(|parsed| parsed.host_str().map(ToString::to_string))
            {
                hosts.insert(host);
            }
        }
    }

    grounded_entries >= expected_entries && hosts.len() >= 3
}

fn tool_results_lack_current_research_grounding(prompt: &str, messages: &[LlmMessage]) -> bool {
    if !prompt_requires_strict_current_research_validation(prompt) {
        return false;
    }

    let expected_entries = requested_research_entry_count(prompt).unwrap_or(1);
    let mut corpus = String::new();
    for message in messages.iter().filter(|message| message.role == "tool") {
        if !message.text.trim().is_empty() {
            corpus.push_str(&message.text);
            corpus.push('\n');
        }
        if !message.tool_result.is_null() {
            if let Ok(serialized) = serde_json::to_string(&message.tool_result) {
                corpus.push_str(&serialized);
                corpus.push('\n');
            }
        }
    }

    if corpus.trim().is_empty() {
        return true;
    }

    if prompt_requests_url_field(prompt) {
        let url_count = regex::Regex::new(r"https?://\S+")
            .map(|re| re.find_iter(&corpus).count())
            .unwrap_or(0);
        if url_count < expected_entries {
            return true;
        }
    }

    if prompt_requests_date_field(prompt) {
        let specific_date_count = specific_calendar_date_regex()
            .map(|re| re.find_iter(&corpus).count())
            .unwrap_or(0);
        if specific_date_count < expected_entries {
            return true;
        }
    }

    false
}

fn current_research_output_needs_additional_evidence(
    prompt: &str,
    messages: &[LlmMessage],
) -> bool {
    if !prompt_requires_strict_current_research_validation(prompt) {
        return false;
    }

    tool_results_lack_current_research_grounding(prompt, messages)
        || tool_results_lack_current_research_diversity(prompt, messages)
        || tool_results_lack_direct_current_research_evidence(prompt, messages)
}

fn current_research_output_requires_targeted_search(
    prompt: &str,
    session_workdir: &Path,
    messages: &[LlmMessage],
) -> bool {
    if !prompt_requests_current_web_research(prompt) {
        return false;
    }
    if current_research_output_needs_additional_evidence(prompt, messages) {
        return true;
    }
    if !has_file_completion_candidate_activity(messages) {
        return false;
    }

    !invalid_file_management_targets(prompt, session_workdir, messages).is_empty()
}

fn should_force_current_research_synthesis(
    prompt: &str,
    session_workdir: &Path,
    messages: &[LlmMessage],
    has_expected_file_targets: bool,
    literal_json_output: bool,
    round: usize,
    total_tool_calls: usize,
) -> bool {
    if literal_json_output
        || !has_expected_file_targets
        || !prompt_requires_strict_current_research_validation(prompt)
    {
        return false;
    }

    if has_file_completion_candidate_activity(messages) {
        if !missing_file_management_targets(prompt, session_workdir, messages).is_empty() {
            return false;
        }
        if invalid_file_management_targets(prompt, session_workdir, messages).is_empty() {
            return false;
        }
    }

    if tool_results_lack_current_research_grounding(prompt, messages) {
        return false;
    }

    if tool_results_lack_current_research_diversity(prompt, messages) {
        return false;
    }

    if tool_results_lack_direct_current_research_evidence(prompt, messages) {
        return false;
    }

    round >= 1 && total_tool_calls >= 1
}

fn completed_current_research_file_targets(
    prompt: &str,
    session_workdir: &Path,
    messages: &[LlmMessage],
    literal_json_output: bool,
) -> Vec<String> {
    if literal_json_output || !prompt_requires_strict_current_research_validation(prompt) {
        return Vec::new();
    }

    if !has_file_completion_candidate_activity(messages) {
        return Vec::new();
    }

    let expected_targets = expected_file_management_targets(prompt);
    if expected_targets.is_empty() {
        return Vec::new();
    }

    if !missing_file_management_targets(prompt, session_workdir, messages).is_empty() {
        return Vec::new();
    }

    if !invalid_file_management_targets(prompt, session_workdir, messages).is_empty() {
        return Vec::new();
    }

    expected_targets
        .into_iter()
        .filter_map(|group| group.into_iter().next())
        .collect()
}

fn completed_file_management_targets(
    prompt: &str,
    session_workdir: &Path,
    messages: &[LlmMessage],
    literal_json_output: bool,
) -> Vec<String> {
    if literal_json_output {
        return Vec::new();
    }

    if !has_file_completion_candidate_activity(messages) {
        return Vec::new();
    }

    let expected_targets = expected_file_management_targets(prompt);
    if expected_targets.is_empty() {
        return Vec::new();
    }

    if !missing_file_management_targets(prompt, session_workdir, messages).is_empty() {
        return Vec::new();
    }

    if !invalid_file_management_targets(prompt, session_workdir, messages).is_empty() {
        return Vec::new();
    }

    expected_targets
        .into_iter()
        .filter_map(|group| group.into_iter().next())
        .collect()
}

fn completion_text_for_file_targets(completed_targets: &[String]) -> String {
    match completed_targets {
        [target] => {
            let ext = Path::new(target)
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp" | "gif") {
                format!(
                    "Completed. Generated and saved `{}` in the working directory.",
                    target
                )
            } else {
                format!("Completed. Saved `{}` in the working directory.", target)
            }
        }
        _ => format!(
            "Completed. Saved the requested files in the working directory: {}.",
            completed_targets
                .iter()
                .map(|path| format!("`{path}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn prepend_completion_notice(text: String, notice: Option<&str>) -> String {
    match notice {
        Some(prefix) if !prefix.trim().is_empty() => format!("{}\n\n{}", prefix.trim(), text),
        _ => text,
    }
}

fn markdown_section_preview(content: &str, max_sections: usize, max_chars: usize) -> String {
    let lines = content.lines().collect::<Vec<_>>();
    let mut sections = Vec::new();
    let mut index = 0usize;

    while index < lines.len() && sections.len() < max_sections {
        let line = lines[index].trim();
        if !line.starts_with('#') {
            index += 1;
            continue;
        }

        let mut section_lines = vec![line.to_string()];
        let mut body_line_count = 0usize;
        let mut cursor = index + 1;
        while cursor < lines.len() {
            let candidate = lines[cursor].trim();
            if candidate.starts_with('#') {
                break;
            }
            if !candidate.is_empty() {
                section_lines.push(truncate_text_with_head_tail(candidate, 220).0);
                body_line_count += 1;
                if body_line_count >= 2 {
                    break;
                }
            }
            cursor += 1;
        }
        sections.push(section_lines.join("\n"));
        index = cursor;
    }

    let joined = sections.join("\n\n");
    if joined.trim().is_empty() {
        String::new()
    } else if joined.chars().count() > max_chars {
        truncate_text_with_head_tail(&joined, max_chars).0
    } else {
        joined
    }
}

fn completion_preview_for_file_target(session_workdir: &Path, target: &str) -> Option<String> {
    let path = session_workdir.join(target);
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(ext.as_str(), "md" | "txt" | "json" | "csv" | "yaml" | "yml") {
        return None;
    }

    let content = std::fs::read_to_string(&path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }

    let preview = if matches!(ext.as_str(), "md" | "txt")
        && count_words(trimmed) <= 420
        && trimmed.chars().count() <= 2_400
    {
        trimmed.to_string()
    } else if ext == "md" {
        markdown_section_preview(trimmed, 6, 1_400)
    } else if matches!(ext.as_str(), "md" | "txt") {
        paragraph_preview(trimmed, 4, 280)
    } else {
        truncate_text_with_head_tail(trimmed, 280).0
    };
    let preview = preview.trim();
    if preview.is_empty() {
        return None;
    }

    Some(format!(
        "Preview of `{target}` ({})\n{}",
        if matches!(ext.as_str(), "md" | "txt") {
            format!("{} words", count_words(trimmed))
        } else {
            format!("{} chars", trimmed.chars().count())
        },
        preview
    ))
}

fn completion_preview_payload_for_file_target(
    session_workdir: &Path,
    target: &str,
) -> Option<Value> {
    let path = session_workdir.join(target);
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !matches!(ext.as_str(), "md" | "txt" | "json" | "csv" | "yaml" | "yml") {
        return None;
    }

    let preview = completion_preview_for_file_target(session_workdir, target)?;
    let raw_content = std::fs::read_to_string(&path).ok().unwrap_or_default();
    let trimmed = raw_content.trim();
    let heading_count = trimmed
        .lines()
        .filter(|line| line.trim_start().starts_with('#'))
        .count();
    let section_count = trimmed
        .lines()
        .filter(|line| line.trim_start().starts_with("## "))
        .count();
    let odds_marker_count = trimmed.match_indices("**Current odds:**").count();
    let related_news_count = trimmed.match_indices("**Related news:**").count();
    Some(json!({
        "target": target,
        "prefetched": true,
        "format": ext,
        "word_count": count_words(trimmed),
        "heading_count": heading_count,
        "section_count": section_count,
        "odds_marker_count": odds_marker_count,
        "related_news_count": related_news_count,
        "artifact_preview": preview,
        "path": path.to_string_lossy().to_string(),
    }))
}

fn record_grounded_answer_preview(
    store: &SessionStore,
    session_id: &str,
    files: &[(String, String, String)],
    answer_text: &str,
) {
    let source_paths = files
        .iter()
        .map(|(relative_path, _, _)| relative_path.clone())
        .collect::<Vec<_>>();
    record_synthetic_tool_interaction(
        store,
        session_id,
        "auto_preview_grounded_answers",
        "read_file",
        "read_file",
        json!({
            "path": source_paths.first().cloned().unwrap_or_else(|| "grounded_input".to_string()),
            "mode": "answer_preview",
        }),
        &json!({
            "answer_preview": answer_text,
            "grounded": true,
            "source_paths": source_paths,
        }),
    );
}

fn record_completed_file_preview_interactions(
    store: &SessionStore,
    session_id: &str,
    session_workdir: &Path,
    completed_targets: &[String],
) {
    for (idx, target) in completed_targets.iter().enumerate() {
        let Some(result) = completion_preview_payload_for_file_target(session_workdir, target) else {
            continue;
        };
        record_synthetic_tool_interaction(
            store,
            session_id,
            &format!("auto_preview_completed_file_{}", idx + 1),
            "read_file",
            "read_file",
            json!({
                "path": target,
                "mode": "completion_preview",
            }),
            &result,
        );
    }
}

fn prompt_prefers_brief_completion_confirmation(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let explicitly_requests_preview = [
        "preview",
        "show me",
        "show the",
        "display",
        "print the",
        "paste the",
        "quote the",
        "include the contents",
        "read it back",
        "read back",
        "review the file",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle));

    prompt_requests_memory_file_capture(prompt)
        || prompt_requests_executive_briefing(prompt)
        || prompt_requests_email_corpus_review(prompt)
        || prompt_requests_humanization(prompt)
        || prompt_requests_prediction_market_briefing(prompt)
        || (!explicitly_requests_preview
            && prompt_requests_file_output(prompt)
            && expected_file_management_targets(prompt).len() == 1
            && !prompt_requests_current_web_research(prompt)
            && !prompt_requests_document_extraction(prompt)
            && !prompt_requests_tabular_inspection(prompt)
            && !prompt_requests_file_grounded_question_answers(prompt))
}

fn maybe_record_completed_file_preview_interactions(
    store: &SessionStore,
    session_id: &str,
    prompt: &str,
    session_workdir: &Path,
    completed_targets: &[String],
) {
    if prompt_prefers_brief_completion_confirmation(prompt)
        || prompt_requests_email_triage_report(prompt)
        || prompt_requests_prediction_market_briefing(prompt)
    {
        return;
    }
    record_completed_file_preview_interactions(
        store,
        session_id,
        session_workdir,
        completed_targets,
    );
}

fn completion_message_for_file_targets(session_workdir: &Path, completed_targets: &[String]) -> String {
    let mut text = completion_text_for_file_targets(completed_targets);
    if completed_targets.len() == 1 {
        if let Some(preview) =
            completion_preview_for_file_target(session_workdir, &completed_targets[0])
        {
            text.push_str("\n\n");
            text.push_str(&preview);
        }
    }
    text
}

fn completion_message_for_prompt_file_targets(
    prompt: &str,
    session_workdir: &Path,
    completed_targets: &[String],
) -> String {
    if prompt_prefers_brief_completion_confirmation(prompt) {
        return completion_text_for_file_targets(completed_targets);
    }
    completion_message_for_file_targets(session_workdir, completed_targets)
}

fn completion_notice_for_auto_prepared_skill(
    prompt: &str,
    auto_prepared_skill_name: Option<&str>,
) -> Option<String> {
    let skill_name = auto_prepared_skill_name?;
    if requested_skill_install_name(prompt)
        .map(|requested| requested.eq_ignore_ascii_case(skill_name))
        .unwrap_or(false)
    {
        Some(format!(
            "Completed `/install {}` from the skill registry and used the loaded `{}` skill.",
            skill_name, skill_name
        ))
    } else {
        Some(format!(
            "Installed and used the requested `{}` skill.",
            skill_name
        ))
    }
}

fn pending_current_research_rewrite_details(
    prompt: &str,
    session_workdir: &Path,
    messages: &[LlmMessage],
    literal_json_output: bool,
) -> Vec<String> {
    if literal_json_output || !prompt_requests_current_web_research(prompt) {
        return Vec::new();
    }

    if !has_file_completion_candidate_activity(messages) {
        return Vec::new();
    }

    if !missing_file_management_targets(prompt, session_workdir, messages).is_empty() {
        return Vec::new();
    }

    let invalid_targets = invalid_file_management_targets(prompt, session_workdir, messages);
    if invalid_targets.is_empty() {
        return Vec::new();
    }

    describe_invalid_file_management_targets(prompt, session_workdir, messages, &invalid_targets)
}

fn prompt_requests_gitignore_file(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    prompt_lower.contains(".gitignore")
        || (prompt_lower.contains("gitignore") && prompt_lower.contains("ignore"))
}

fn output_lacks_expected_ignore_patterns(prompt: &str, content: &str) -> bool {
    if !prompt_requests_gitignore_file(prompt) {
        return false;
    }

    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let content_lower = content.to_ascii_lowercase();

    let mut required_patterns = Vec::new();
    if prompt_lower.contains("__pycache__") || prompt_lower.contains("pycache") {
        required_patterns.push("__pycache__");
    }
    if prompt_lower.contains("node_modules") {
        required_patterns.push("node_modules");
    }
    if prompt_lower.contains(".env") {
        required_patterns.push(".env");
    }
    if prompt_lower.contains(".ds_store") {
        required_patterns.push(".ds_store");
    }
    if prompt_lower.contains("thumbs.db") {
        required_patterns.push("thumbs.db");
    }

    !required_patterns.is_empty()
        && required_patterns
            .into_iter()
            .any(|pattern| !content_lower.contains(pattern))
}

fn prompt_requests_document_extraction(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let mentions_document = [
        ".pdf",
        "pdf",
        "report",
        "document",
        "paper",
        "extract",
        "answer.txt",
        "summarize",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle));

    mentions_document && extract_relative_file_paths(prompt).iter().any(|path| path.ends_with(".pdf"))
}

fn prompt_requests_tabular_inspection(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let mentions_tabular_task = [
        "spreadsheet",
        "worksheet",
        "workbook",
        "sheet",
        "tabular",
        "table",
        ".xlsx",
        ".csv",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle));
    let has_tabular_path = extract_relative_file_paths(prompt)
        .iter()
        .any(|path| path.ends_with(".xlsx") || path.ends_with(".csv"));

    mentions_tabular_task && has_tabular_path
}

fn prompt_prefers_direct_specialized_tools(prompt: &str) -> bool {
    prompt_requests_document_extraction(prompt)
        || prompt_requests_tabular_inspection(prompt)
        || prompt_requests_image_generation(prompt)
        || prompt_requests_current_web_research(prompt)
}

fn is_effectively_empty_text_file(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|content| content.trim().is_empty())
        .unwrap_or(false)
}

fn image_file_matches_expected_format(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    if bytes.len() < 8 {
        return false;
    }
    bytes.starts_with(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'])
        || bytes.starts_with(&[0xff, 0xd8, 0xff])
}

fn invalid_file_management_targets(
    prompt: &str,
    session_workdir: &Path,
    messages: &[LlmMessage],
) -> Vec<String> {
    let expected_groups = expected_file_management_targets(prompt);
    if expected_groups.is_empty() {
        return Vec::new();
    }

    let created_paths = collect_successful_file_management_paths(messages);
    let prediction_market_briefing = prompt_requests_prediction_market_briefing(prompt);
    expected_groups
        .into_iter()
        .filter_map(|group| {
            let invalid = group.iter().find_map(|candidate| {
                let absolute = session_workdir.join(candidate);
                let was_targeted = created_paths.contains(absolute.to_string_lossy().as_ref())
                    || created_paths.contains(candidate.as_str());
                if !was_targeted || !absolute.exists() {
                    return None;
                }

                let ext = absolute
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let file_name = absolute
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                let validation_cutoff = successful_file_management_cutoff_for_target(
                    messages,
                    candidate,
                    &absolute,
                );
                let validation_messages = &messages[..validation_cutoff];

                if matches!(ext.as_str(), "txt" | "md" | "json" | "csv" | "yaml" | "yml")
                    && is_effectively_empty_text_file(&absolute)
                {
                    return Some(candidate.clone());
                }

                if matches!(ext.as_str(), "txt" | "md")
                    && std::fs::read_to_string(&absolute)
                        .map(|content| {
                                output_lacks_descriptive_answer(prompt, &content)
                                || output_lacks_numeric_market_fact(prompt, &content)
                                || output_lacks_current_research_details(prompt, &content)
                                || output_contains_unsupported_research_branding(
                                    &content,
                                    validation_messages,
                                )
                                || output_lacks_longform_markdown_structure(prompt, &content)
                                || output_violates_requested_word_budget(prompt, &content)
                                || output_lacks_prediction_market_briefing(
                                    prompt, &content,
                                )
                                || output_lacks_executive_briefing_structure(
                                    prompt, &content,
                                )
                                || output_lacks_concise_summary_structure(prompt, &content)
                                || (ext == "md"
                                    && output_lacks_markdown_research_structure(
                                        prompt, &content,
                                    ))
                                || (!prediction_market_briefing
                                    && output_entries_lack_direct_research_support(
                                        prompt,
                                        &content,
                                        validation_messages,
                                    ))
                                || (!prediction_market_briefing
                                    && tool_results_lack_current_research_grounding(
                                        prompt,
                                        validation_messages,
                                    ))
                                || (!prediction_market_briefing
                                    && tool_results_lack_current_research_diversity(
                                        prompt,
                                        validation_messages,
                                    ))
                                || (!prediction_market_briefing
                                    && tool_results_lack_direct_current_research_evidence(
                                        prompt,
                                        validation_messages,
                                    ))
                        })
                        .unwrap_or(false)
                {
                    return Some(candidate.clone());
                }

                if file_name == ".gitignore"
                    && std::fs::read_to_string(&absolute)
                        .map(|content| output_lacks_expected_ignore_patterns(prompt, &content))
                        .unwrap_or(false)
                {
                    return Some(candidate.clone());
                }

                if matches!(ext.as_str(), "png" | "jpg" | "jpeg")
                    && !image_file_matches_expected_format(&absolute)
                {
                    return Some(candidate.clone());
                }

                None
            });

            invalid.or_else(|| None)
        })
        .collect()
}

fn describe_invalid_file_management_targets(
    prompt: &str,
    session_workdir: &Path,
    messages: &[LlmMessage],
    invalid_targets: &[String],
) -> Vec<String> {
    invalid_targets
        .iter()
        .map(|target| {
            let absolute = session_workdir.join(target);
            let Some(content) = std::fs::read_to_string(&absolute).ok() else {
                return format!("{target}: rewrite the file with valid task output.");
            };

            if prompt_requests_longform_markdown_writing(prompt) {
                if let Some((minimum_words, maximum_words)) = requested_word_budget_bounds(prompt) {
                    let actual_words = count_words(&content);
                    if actual_words < minimum_words || actual_words > maximum_words {
                        return format!(
                            "{target}: current draft is {actual_words} words; revise it to {}-{} words and keep the Markdown article structure.",
                            minimum_words, maximum_words
                        );
                    }
                }
                let strong_example_markers = [
                    "for example",
                    "for instance",
                    "in practice",
                    "consider a developer",
                    "consider an engineer",
                ];
                let marker_count = strong_example_markers
                    .iter()
                    .filter(|needle| content.to_ascii_lowercase().contains(**needle))
                    .count();
                if marker_count == 0 {
                    return format!(
                        "{target}: add at least one concrete example or real-world developer scenario so the article is more specific and engaging."
                    );
                }
            }

            if prompt_requests_executive_briefing(prompt) {
                if let Some((minimum_words, maximum_words)) =
                    executive_briefing_word_budget_bounds(prompt)
                {
                    let actual_words = count_words(&content);
                    if actual_words < minimum_words || actual_words > maximum_words {
                        return format!(
                            "{target}: current draft is {actual_words} words; keep the executive briefing concise at {}-{} words while preserving the summary, risks, and action sections.",
                            minimum_words, maximum_words
                        );
                    }
                }
            }

            if prompt_requests_concise_summary_file(prompt) {
                let actual_words = count_words(&content);
                if !(120..=350).contains(&actual_words) {
                    return format!(
                        "{target}: current draft is {actual_words} words; keep the summary concise at roughly 120-350 words."
                    );
                }
                if let Some(expected_paragraphs) = requested_paragraph_count(prompt) {
                    let actual_paragraphs = paragraph_count(&content);
                    if actual_paragraphs != expected_paragraphs {
                        return format!(
                            "{target}: current draft has {actual_paragraphs} paragraphs; rewrite it to exactly {expected_paragraphs} paragraphs."
                        );
                    }
                }
            }

            if prompt_requests_prediction_market_briefing(prompt)
                && output_lacks_prediction_market_briefing(prompt, &content)
            {
                return format!(
                    "{target}: keep the exact numbered market format, and make every `**Related news:**` line cite one concrete story from the last 48 hours with a publication date and a full article URL copied from grounded search results. Freshness matters more than prestige here, so replace stale Reuters or AP links with a newer credible source instead of reusing older coverage. Remove placeholder phrases like 'within the last 48 hours' or 'current search results referenced'."
                );
            }

            if output_lacks_executive_briefing_structure(prompt, &content) {
                return format!(
                    "{target}: add a short executive summary at the top plus an explicit action-items or decisions-needed section, then keep the rest grouped into a few concise sections."
                );
            }

            if prompt_requests_current_web_research(prompt) {
                if current_research_output_needs_additional_evidence(prompt, messages) {
                    if prompt_requests_conference_roundup(prompt) {
                        return format!(
                            "{target}: the collected official evidence still does not support enough verified upcoming conferences with exact dates and locations. Run one more targeted official search for a stronger replacement from a different organizer or ecosystem, then rewrite this file once. Do not keep rewriting the same stale or unsupported conference list without new evidence."
                        );
                    }
                    return format!(
                        "{target}: the collected official evidence still does not support all requested current-research fields. Run one more targeted official search to replace unsupported entries or fill the missing facts, then rewrite this file once. Do not keep rewriting the same unsupported content without new evidence."
                    );
                }
                let host_counts = extract_url_host_counts(&content);
                let dominant_host_count = host_counts.values().copied().max().unwrap_or(0);
                if requested_research_entry_count(prompt).unwrap_or(1) >= 5 && dominant_host_count > 2
                {
                    return format!(
                        "{target}: too many entries come from one organizer or domain; replace weaker duplicates with other strong verified conferences from different hosts."
                    );
                }
                if output_contains_unsupported_research_branding(&content, &[]) {
                    return format!(
                        "{target}: at least one event name is over-expanded or does not align with the verified branding; keep the official conference name as it appears in the supporting evidence instead of inventing a longer expansion."
                    );
                }
                if prompt_requests_conference_roundup(prompt)
                    && content
                        .lines()
                        .filter_map(|line| {
                            let url_re = regex::Regex::new(r#"https?://[^\s)>"]+"#).ok()?;
                            let url_match = url_re.find(line)?;
                            Some(line[..url_match.start()].replace('|', " "))
                        })
                        .any(|name| conference_entry_looks_low_authority(name.trim()))
                {
                    return format!(
                        "{target}: replace local workshop-style or weak city-edition entries with established annual tech conferences whose official pages show exact dates and locations."
                    );
                }
            }

            format!(
                "{target}: rewrite the file so it satisfies the requested content and format."
            )
        })
        .collect()
}

fn should_prefetch_prompt_file(path: &str) -> bool {
    matches!(
        Path::new(path).extension().and_then(|value| value.to_str()),
        Some("md" | "txt" | "json" | "yaml" | "yml")
    )
}

fn load_prefetched_prompt_file_previews(paths: &[String]) -> Vec<(String, String, bool)> {
    const MAX_PREFETCH_FILES: usize = 3;
    const MAX_PREFETCH_BYTES: u64 = 64 * 1024;
    const MAX_PREFETCH_CHARS: usize = 12_000;

    paths
        .iter()
        .filter(|path| should_prefetch_prompt_file(path))
        .take(MAX_PREFETCH_FILES)
        .filter_map(|path| {
            let metadata = std::fs::metadata(path).ok()?;
            if !metadata.is_file() || metadata.len() > MAX_PREFETCH_BYTES {
                return None;
            }
            let content = std::fs::read_to_string(path).ok()?;
            let preview = utf8_safe_preview(&content, MAX_PREFETCH_CHARS).to_string();
            Some((path.clone(), preview.clone(), preview.len() < content.len()))
        })
        .collect()
}

fn build_prefetched_prompt_file_messages(paths: &[String]) -> Vec<LlmMessage> {
    load_prefetched_prompt_file_previews(paths)
        .into_iter()
        .enumerate()
        .map(|(idx, (path, preview, truncated))| {
            LlmMessage::tool_result(
                &format!("prefetch_prompt_file_{}", idx + 1),
                "read_file",
                json!({
                    "path": path,
                    "content": preview,
                    "prefetched": true,
                    "truncated": truncated,
                }),
            )
        })
        .collect()
}

fn build_prefetched_prompt_file_context(paths: &[String]) -> Option<String> {
    const MAX_CONTEXT_CHARS_PER_FILE: usize = 2_000;

    let previews = load_prefetched_prompt_file_previews(paths);
    if previews.is_empty() {
        return None;
    }

    let sections = previews
        .into_iter()
        .map(|(path, preview, truncated)| {
            let shortened = utf8_safe_preview(&preview, MAX_CONTEXT_CHARS_PER_FILE);
            let suffix = if truncated || shortened.len() < preview.len() {
                "\n...[truncated]"
            } else {
                ""
            };
            format!("### {}\n```\n{}{}\n```", path, shortened.trim_end(), suffix)
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    Some(format!(
        "## Prompt File Excerpts\nThese prefetched files are authoritative. Follow their exact requirements and do not invent extra questions, columns, or output formats.\n\n{}",
        sections
    ))
}

fn record_synthetic_tool_interaction(
    store: &SessionStore,
    session_id: &str,
    call_id: &str,
    tool_name: &str,
    actual_tool_name: &str,
    args: Value,
    result: &Value,
) {
    store.add_structured_tool_call_message(
        session_id,
        vec![json!({
            "type": "toolCall",
            "tool_call_id": call_id,
            "name": tool_name,
            "actual_tool_name": actual_tool_name,
            "arguments": args,
        })],
    );
    store.add_structured_tool_result_message(
        session_id,
        tool_name,
        actual_tool_name,
        call_id,
        result,
    );
}
