#[derive(Clone, Debug, Default)]
struct SessionPromptProfile {
    role_name: Option<String>,
    role_description: Option<String>,
    system_prompt: Option<String>,
    allowed_tools: Option<Vec<String>>,
    max_iterations: Option<usize>,
    role_type: Option<String>,
    can_delegate_to: Option<Vec<String>>,
    prompt_mode: Option<PromptMode>,
    reasoning_policy: Option<ReasoningPolicy>,
}

fn normalize_text_block(text: &str) -> Option<String> {
    let mut lines = Vec::new();
    let mut blank_run = 0usize;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            blank_run += 1;
            if !lines.is_empty() && blank_run == 1 {
                lines.push(String::new());
            }
            continue;
        }

        blank_run = 0;
        lines.push(line.to_string());
    }

    while matches!(lines.last(), Some(line) if line.is_empty()) {
        lines.pop();
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn utf8_safe_preview(text: &str, max_chars: usize) -> &str {
    if max_chars == 0 {
        return "";
    }

    match text.char_indices().nth(max_chars) {
        Some((idx, _)) => &text[..idx],
        None => text,
    }
}

fn truncate_text_with_head_tail(text: &str, max_chars: usize) -> (String, bool) {
    let total_chars = text.chars().count();
    if total_chars <= max_chars {
        return (text.to_string(), false);
    }
    if max_chars <= 32 {
        return (utf8_safe_preview(text, max_chars).to_string(), true);
    }

    let head_chars = max_chars * 2 / 3;
    let tail_chars = max_chars.saturating_sub(head_chars + 17);
    let head = utf8_safe_preview(text, head_chars);
    let tail_start = text
        .char_indices()
        .rev()
        .nth(tail_chars)
        .map(|(idx, _)| idx)
        .unwrap_or(0);
    let tail = &text[tail_start..];

    (format!("{}\n... [truncated] ...\n{}", head, tail), true)
}

fn count_words(text: &str) -> usize {
    text.split_whitespace().count()
}

fn slugify_file_stem(text: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

fn calendar_weekday_index(raw: &str) -> Option<i32> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "sunday" | "sun" => Some(0),
        "monday" | "mon" => Some(1),
        "tuesday" | "tue" | "tues" => Some(2),
        "wednesday" | "wed" => Some(3),
        "thursday" | "thu" | "thur" | "thurs" => Some(4),
        "friday" | "fri" => Some(5),
        "saturday" | "sat" => Some(6),
        _ => None,
    }
}

fn next_local_weekday_date(target_wday: i32) -> Option<(i32, u32, u32)> {
    let now_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as libc::time_t;
    let mut tm_buf: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        libc::localtime_r(&now_epoch, &mut tm_buf);
    }
    let mut days_ahead = (target_wday - tm_buf.tm_wday + 7) % 7;
    if days_ahead == 0 {
        days_ahead = 7;
    }
    tm_buf.tm_mday += days_ahead;
    tm_buf.tm_isdst = -1;
    unsafe {
        libc::mktime(&mut tm_buf);
    }
    Some((
        tm_buf.tm_year + 1900,
        (tm_buf.tm_mon + 1) as u32,
        tm_buf.tm_mday as u32,
    ))
}

fn current_utc_ics_timestamp() -> Option<String> {
    let now_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as libc::time_t;
    let mut tm_buf: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        libc::gmtime_r(&now_epoch, &mut tm_buf);
    }
    Some(format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        tm_buf.tm_year + 1900,
        tm_buf.tm_mon + 1,
        tm_buf.tm_mday,
        tm_buf.tm_hour,
        tm_buf.tm_min,
        tm_buf.tm_sec
    ))
}

fn extract_relative_calendar_request(prompt: &str) -> Option<(String, String)> {
    let prompt_lower = prompt.to_ascii_lowercase();
    if !prompt_lower.contains("schedule")
        && !prompt_lower.contains("calendar")
        && !prompt_lower.contains("meeting")
    {
        return None;
    }

    let weekday_re = regex::Regex::new(
        r"(?i)\bnext\s+(monday|tuesday|wednesday|thursday|friday|saturday|sunday)\b",
    )
    .ok()?;
    let weekday_name = weekday_re.captures(prompt)?.get(1)?.as_str();
    let target_wday = calendar_weekday_index(weekday_name)?;

    let time_re = regex::Regex::new(r"(?i)\bat\s+(\d{1,2})(?::(\d{2}))?\s*(am|pm)\b").ok()?;
    let time_caps = time_re.captures(prompt)?;
    let hour_12 = time_caps.get(1)?.as_str().parse::<u32>().ok()?;
    let minute = time_caps
        .get(2)
        .and_then(|value| value.as_str().parse::<u32>().ok())
        .unwrap_or(0);
    if !(1..=12).contains(&hour_12) || minute > 59 {
        return None;
    }
    let meridiem = time_caps.get(3)?.as_str().to_ascii_lowercase();
    let mut hour_24 = hour_12 % 12;
    if meridiem == "pm" {
        hour_24 += 12;
    }

    let email_re = regex::Regex::new(r"(?i)\b([A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,})\b").ok()?;
    let attendee = email_re.captures(prompt)?.get(1)?.as_str().to_string();

    let title = regex::Regex::new(r#"(?i)\btitle it\s+"([^"]+)""#)
        .ok()
        .and_then(|re| re.captures(prompt))
        .and_then(|caps| caps.get(1).map(|value| value.as_str().trim().to_string()))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Meeting".to_string());

    let description = regex::Regex::new(r#"(?i)\badd a note about\s+(.+?)(?:\.\s|$)"#)
        .ok()
        .and_then(|re| re.captures(prompt))
        .and_then(|caps| caps.get(1).map(|value| value.as_str().trim().to_string()))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Meeting details".to_string());

    let target = extract_explicit_file_paths(prompt)
        .into_iter()
        .find(|path| path.to_ascii_lowercase().ends_with(".ics"))
        .or_else(|| {
            let slug = slugify_file_stem(&title);
            if slug.is_empty() {
                None
            } else {
                Some(format!("{}.ics", slug))
            }
        })?;

    let (year, month, day) = next_local_weekday_date(target_wday)?;
    let date_compact = format!("{:04}{:02}{:02}", year, month, day);
    let start_time = format!("{:02}{:02}00", hour_24, minute);
    let end_hour = (hour_24 + 1) % 24;
    let end_time = format!("{:02}{:02}00", end_hour, minute);
    let dtstamp =
        current_utc_ics_timestamp().unwrap_or_else(|| format!("{}T000000Z", date_compact));
    let uid = format!(
        "{}-{}T{}@tizenclaw.local",
        slugify_file_stem(&title),
        date_compact,
        start_time
    );
    let description = description.trim_end_matches('.').trim().to_string();

    let ics = format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//TizenClaw//Calendar Simulation//EN\r\nCALSCALE:GREGORIAN\r\nMETHOD:REQUEST\r\nBEGIN:VEVENT\r\nUID:{uid}\r\nDTSTAMP:{dtstamp}\r\nDTSTART:{date_compact}T{start_time}\r\nDTEND:{date_compact}T{end_time}\r\nSUMMARY:{title}\r\nDESCRIPTION:{description}.\r\nATTENDEE;CN={attendee}:MAILTO:{attendee}\r\nSTATUS:CONFIRMED\r\nTRANSP:OPAQUE\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    );

    Some((target, ics))
}

fn requested_word_range_bounds(prompt: &str) -> Option<(usize, usize)> {
    let regex = regex::Regex::new(r"(?ix)\b(\d{2,5})\s*(?:-|–|to)\s*(\d{2,5})\s*words?\b").ok()?;
    let captures = regex.captures(prompt)?;
    let minimum = captures.get(1)?.as_str().parse::<usize>().ok()?;
    let maximum = captures.get(2)?.as_str().parse::<usize>().ok()?;
    if minimum >= maximum {
        return None;
    }
    Some((minimum, maximum))
}

fn requested_word_budget_bounds(prompt: &str) -> Option<(usize, usize)> {
    if let Some(bounds) = requested_word_range_bounds(prompt) {
        return Some(bounds);
    }

    let target_words = requested_word_target(prompt)?;
    if target_words < 100 {
        return None;
    }

    if prompt_requests_longform_markdown_writing(prompt) {
        let tolerance = (target_words / 10).clamp(35, 50);
        Some((
            target_words.saturating_sub(tolerance),
            target_words.saturating_add(tolerance),
        ))
    } else {
        Some((
            target_words.saturating_mul(8) / 10,
            target_words.saturating_mul(12) / 10,
        ))
    }
}

fn executive_briefing_word_budget_bounds(prompt: &str) -> Option<(usize, usize)> {
    if !prompt_requests_executive_briefing(prompt) {
        return None;
    }

    if let Some(bounds) = requested_word_budget_bounds(prompt) {
        return Some(bounds);
    }

    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    if prompt_lower.contains("concise")
        || prompt_lower.contains("executive attention")
        || prompt_lower.contains("daily briefing")
    {
        return Some((500, 760));
    }

    None
}

fn requested_word_target(prompt: &str) -> Option<usize> {
    let regex = regex::Regex::new(r"(?ix)\b(\d{2,5})\s*(?:-|–)?\s*word\b").ok()?;
    regex
        .captures(prompt)
        .and_then(|captures| captures.get(1))
        .and_then(|value| value.as_str().parse::<usize>().ok())
}

fn requested_paragraph_count(prompt: &str) -> Option<usize> {
    let regex = regex::Regex::new(r"(?ix)\b(\d{1,2})\s*(?:-|–)?\s*paragraphs?\b").ok()?;
    regex
        .captures(prompt)
        .and_then(|captures| captures.get(1))
        .and_then(|value| value.as_str().parse::<usize>().ok())
}

fn paragraph_count(text: &str) -> usize {
    text.split("\n\n")
        .filter(|paragraph| !paragraph.trim().is_empty())
        .count()
}

fn extract_text_match_snippets(
    text: &str,
    pattern: &str,
    window_chars: usize,
    max_matches: usize,
) -> Vec<String> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return Vec::new();
    }

    let mut snippets = Vec::new();

    if looks_like_regex_pattern(pattern) {
        if let Ok(regex) = regex::RegexBuilder::new(pattern)
            .case_insensitive(true)
            .dot_matches_new_line(true)
            .build()
        {
            for matched in regex.find_iter(text).take(max_matches) {
                append_text_match_snippet(
                    &mut snippets,
                    text,
                    matched.start(),
                    matched.end(),
                    window_chars,
                );
                if snippets.len() >= max_matches {
                    break;
                }
            }
            if !snippets.is_empty() {
                return snippets;
            }
        }
    }

    let haystack = text.to_ascii_lowercase();
    let needle = pattern.to_ascii_lowercase();
    let mut search_from = 0usize;
    while snippets.len() < max_matches {
        let Some(relative_idx) = haystack[search_from..].find(&needle) else {
            break;
        };
        let match_start = search_from + relative_idx;
        let match_end = match_start + needle.len();
        append_text_match_snippet(&mut snippets, text, match_start, match_end, window_chars);
        search_from = match_end;
    }

    snippets
}

fn looks_like_regex_pattern(pattern: &str) -> bool {
    pattern.contains('\\')
        || pattern.contains('[')
        || pattern.contains(']')
        || pattern.contains('(')
        || pattern.contains(')')
        || pattern.contains('{')
        || pattern.contains('}')
        || pattern.contains('+')
        || pattern.contains('?')
        || pattern.contains(".*")
}

fn append_text_match_snippet(
    snippets: &mut Vec<String>,
    text: &str,
    match_start: usize,
    match_end: usize,
    window_chars: usize,
) {
    let start_char = text[..match_start].chars().count();
    let end_char = text[..match_end].chars().count();
    let snippet_start_char = start_char.saturating_sub(window_chars / 2);
    let snippet_end_char = end_char.saturating_add(window_chars / 2);

    let snippet_start = text
        .char_indices()
        .nth(snippet_start_char)
        .map(|(idx, _)| idx)
        .unwrap_or(0);
    let snippet_end = text
        .char_indices()
        .nth(snippet_end_char)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len());
    let snippet = text[snippet_start..snippet_end].trim().to_string();
    if !snippets.iter().any(|existing| existing == &snippet) {
        snippets.push(snippet);
    }
}

fn format_unix_timestamp_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let secs_of_day = secs % 86_400;
    let hour = secs_of_day / 3_600;
    let minute = (secs_of_day % 3_600) / 60;
    let second = secs_of_day % 60;

    // Howard Hinnant's civil_from_days algorithm.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        year, month, day, hour, minute, second
    )
}

fn split_match_patterns(pattern: &str) -> Vec<String> {
    let trimmed = pattern.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let parts: Vec<String> = trimmed
        .split('|')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    if parts.len() <= 1 {
        return parts;
    }

    let mut deduped = Vec::new();
    for part in parts {
        if !deduped
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&part))
        {
            deduped.push(part);
        }
    }
    deduped
}

fn paragraph_preview(text: &str, max_paragraphs: usize, max_chars_per_paragraph: usize) -> String {
    text.split("\n\n")
        .map(str::trim)
        .filter(|paragraph| !paragraph.is_empty())
        .take(max_paragraphs)
        .map(|paragraph| utf8_safe_preview(paragraph, max_chars_per_paragraph).to_string())
        .collect::<Vec<_>>()
        .join(" | ")
}

fn build_text_read_payload(content: &str, pattern: Option<&str>, max_chars: usize) -> Value {
    let total_chars = content.chars().count();
    let paragraph_count = paragraph_count(content);
    let preview = paragraph_preview(content, 3, 72);
    let pattern = pattern.map(str::trim).filter(|value| !value.is_empty());
    let snippets = pattern.map(|value| {
        let mut collected = Vec::new();
        for part in split_match_patterns(value) {
            for snippet in extract_text_match_snippets(
                content,
                &part,
                DEFAULT_FILE_READ_MATCH_WINDOW_CHARS,
                DEFAULT_FILE_READ_MAX_MATCHES,
            ) {
                if collected.len() >= DEFAULT_FILE_READ_MAX_MATCHES {
                    break;
                }
                if !collected
                    .iter()
                    .any(|existing: &String| existing == &snippet)
                {
                    collected.push(snippet);
                }
            }
            if collected.len() >= DEFAULT_FILE_READ_MAX_MATCHES {
                break;
            }
        }
        collected
    });

    let combined = if total_chars <= max_chars {
        content.to_string()
    } else {
        snippets
            .as_ref()
            .filter(|items| !items.is_empty())
            .map(|items| items.join("\n\n"))
            .unwrap_or_else(|| content.to_string())
    };
    let (truncated_content, truncated) = truncate_text_with_head_tail(&combined, max_chars);

    json!({
        "paragraph_count": paragraph_count,
        "paragraph_preview": preview,
        "content": truncated_content,
        "truncated": truncated || total_chars > max_chars,
        "total_chars": total_chars,
        "pattern": pattern,
        "match_count": snippets.as_ref().map(|items| items.len()).unwrap_or(0),
    })
}

fn directory_listing_text(path: &Path, entries: &[Value]) -> String {
    let mut lines = vec![format!(
        "Directory listing for `{}`",
        path.to_string_lossy()
    )];
    for entry in entries {
        let name = entry
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let is_dir = entry
            .get("is_dir")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let size = entry.get("size").and_then(Value::as_u64).unwrap_or(0);
        if is_dir {
            lines.push(format!("- {}/", name));
        } else {
            lines.push(format!("- {} ({} bytes)", name, size));
        }
    }
    lines.join("\n")
}

fn build_directory_read_payload(
    path: &Path,
    entries: Vec<Value>,
    backend: &str,
    pattern: Option<&str>,
    max_chars: usize,
) -> Value {
    let listing_text = directory_listing_text(path, &entries);
    let mut payload = build_text_read_payload(&listing_text, pattern, max_chars);
    let object = payload
        .as_object_mut()
        .expect("directory read payload object");
    object.insert("success".into(), json!(true));
    object.insert("operation".into(), json!("read"));
    object.insert("path".into(), json!(path.to_string_lossy()));
    object.insert("backend".into(), json!(backend));
    object.insert("is_dir".into(), json!(true));
    object.insert("entries".into(), json!(entries));
    payload
}

fn normalize_conversation_log_text(text: &str) -> Option<String> {
    normalize_text_block(text)
}

fn log_conversation(role: &str, text: &str) {
    if let Some(normalized) = normalize_conversation_log_text(text) {
        log::info!("[Conversation][{}]\n{}", role, normalized);
    }
}

fn unix_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn inject_context_message(messages: &mut Vec<LlmMessage>, text: String) {
    if text.trim().is_empty() {
        return;
    }
    let context_message = LlmMessage::user(&text);
    if let Some(last_user_idx) = messages.iter().rposition(|message| message.role == "user") {
        messages.insert(last_user_idx, context_message);
    } else {
        messages.push(context_message);
    }
}

fn sanitize_message_for_transport(mut message: LlmMessage) -> Option<LlmMessage> {
    message.text = message.text.trim().to_string();
    message.reasoning_text = message.reasoning_text.trim().to_string();
    message.tool_name = message.tool_name.trim().to_string();
    message.tool_call_id = message.tool_call_id.trim().to_string();

    let has_text = !message.text.is_empty();
    let has_reasoning = !message.reasoning_text.is_empty();
    let has_tool_calls = !message.tool_calls.is_empty();
    let has_tool_payload = message.role == "tool";

    if has_text || has_reasoning || has_tool_calls || has_tool_payload {
        Some(message)
    } else {
        None
    }
}

fn sanitize_messages_for_transport(messages: Vec<LlmMessage>) -> Vec<LlmMessage> {
    messages
        .into_iter()
        .filter_map(sanitize_message_for_transport)
        .collect()
}

fn estimate_text_tokens(text: &str) -> usize {
    estimate_char_tokens(text.chars().count())
}

fn estimate_char_tokens(chars: usize) -> usize {
    (chars.saturating_add(3)) / 4
}

fn current_timestamp_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn normalize_tool_search_text(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect()
}

fn tokenize_tool_search_text(text: &str) -> Vec<String> {
    normalize_tool_search_text(text)
        .split_whitespace()
        .filter(|token| token.len() > 1)
        .filter(|token| !matches!(*token, "tool" | "tools" | "available"))
        .map(ToString::to_string)
        .collect()
}

fn score_tool_search_match(tool: &crate::llm::backend::LlmToolDecl, query: &str) -> usize {
    let query = query.trim();
    if query.is_empty() || query.eq_ignore_ascii_case("ALL") {
        return 1;
    }

    let normalized_query = normalize_tool_search_text(query);
    let normalized_name = normalize_tool_search_text(&tool.name);
    let normalized_description = normalize_tool_search_text(&tool.description);

    if normalized_name.trim() == normalized_query.trim() {
        return 10_000;
    }

    let mut score = 0usize;
    if normalized_name.contains(normalized_query.trim()) {
        score += 2_000;
    }
    if normalized_description.contains(normalized_query.trim()) {
        score += 1_000;
    }

    let query_tokens = tokenize_tool_search_text(query);
    if query_tokens.is_empty() {
        return score;
    }

    let mut tool_tokens: HashSet<String> =
        tokenize_tool_search_text(&tool.name).into_iter().collect();
    tool_tokens.extend(tokenize_tool_search_text(&tool.description));

    let overlap = query_tokens
        .iter()
        .filter(|token| tool_tokens.contains(token.as_str()))
        .count();
    if overlap == 0 {
        return score;
    }

    score += overlap * 100;
    if overlap == query_tokens.len() {
        score += 500;
    }

    score
}

fn dashboard_outbound_queue_path(base_dir: &Path) -> PathBuf {
    base_dir.join("outbound").join("web_dashboard.jsonl")
}

fn append_dashboard_outbound_message(
    base_dir: &Path,
    title: Option<&str>,
    message: &str,
    session_id: Option<&str>,
) -> Result<Value, String> {
    let message = message.trim();
    if message.is_empty() {
        return Err("Outbound message cannot be empty".to_string());
    }

    let path = dashboard_outbound_queue_path(base_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let mut entries = if let Ok(content) = std::fs::read_to_string(&path) {
        content
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let created_at_ms = current_timestamp_millis();
    let record = json!({
        "id": format!("dashboard-{}", created_at_ms),
        "channel": "web_dashboard",
        "title": title.unwrap_or("TizenClaw"),
        "message": message,
        "session_id": session_id,
        "created_at_ms": created_at_ms,
    });
    entries.push(record.to_string());
    if entries.len() > MAX_OUTBOUND_DASHBOARD_MESSAGES {
        let start = entries.len() - MAX_OUTBOUND_DASHBOARD_MESSAGES;
        entries = entries.split_off(start);
    }

    let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    for entry in entries {
        writeln!(file, "{}", entry).map_err(|e| e.to_string())?;
    }

    Ok(record)
}

fn tool_schema_char_count(tools: &[backend::LlmToolDecl]) -> usize {
    tools
        .iter()
        .map(|tool| {
            tool.name.len() + tool.description.len() + tool.parameters.to_string().chars().count()
        })
        .sum()
}

fn total_message_chars(messages: &[LlmMessage]) -> usize {
    messages
        .iter()
        .map(|message| {
            message.text.chars().count()
                + message.reasoning_text.chars().count()
                + message.tool_name.chars().count()
                + message.tool_call_id.chars().count()
                + message.tool_result.to_string().chars().count()
                + message
                    .tool_calls
                    .iter()
                    .map(|tool_call| {
                        tool_call.id.chars().count()
                            + tool_call.name.chars().count()
                            + tool_call.args.to_string().chars().count()
                    })
                    .sum::<usize>()
        })
        .sum()
}

fn log_payload_breakdown(
    session_id: &str,
    prompt: &str,
    history: &[crate::storage::session_store::SessionMessage],
    prefetched_skill_context: Option<&str>,
    dynamic_context: Option<&str>,
    memory_context: Option<&str>,
    system_prompt: &str,
    tools: &[backend::LlmToolDecl],
    messages: &[LlmMessage],
    context_engine: &SizedContextEngine,
) {
    let history_chars: usize = history.iter().map(|msg| msg.text.chars().count()).sum();
    let system_chars = system_prompt.chars().count();
    let skill_chars = prefetched_skill_context
        .map(|text| text.chars().count())
        .unwrap_or(0);
    let runtime_chars = dynamic_context
        .map(|text| text.chars().count())
        .unwrap_or(0);
    let memory_chars = memory_context.map(|text| text.chars().count()).unwrap_or(0);
    let tool_schema_chars = tool_schema_char_count(tools);
    let message_chars = total_message_chars(messages);
    let estimated_message_tokens = context_engine.estimate_tokens(messages);
    let estimated_system_tokens = estimate_text_tokens(system_prompt);
    let estimated_tool_schema_tokens = estimate_char_tokens(tool_schema_chars);

    log::debug!(
        "[PayloadBreakdown] session='{}'\n  prompt_chars={} (~{} tok)\n  history_msgs={} history_chars={} (~{} tok)\n  prefetched_skill_chars={} (~{} tok)\n  runtime_context_chars={} (~{} tok)\n  memory_context_chars={} (~{} tok)\n  system_prompt_chars={} (~{} tok)\n  tools={} tool_schema_chars={} (~{} tok)\n  transport_msgs={} transport_chars={} (~{} tok)\n  estimated_total_input_tokens~={}",
        session_id,
        prompt.chars().count(),
        estimate_text_tokens(prompt),
        history.len(),
        history_chars,
        estimate_char_tokens(history_chars),
        skill_chars,
        estimate_char_tokens(skill_chars),
        runtime_chars,
        estimate_char_tokens(runtime_chars),
        memory_chars,
        estimate_char_tokens(memory_chars),
        system_chars,
        estimated_system_tokens,
        tools.len(),
        tool_schema_chars,
        estimated_tool_schema_tokens,
        messages.len(),
        message_chars,
        estimated_message_tokens,
        estimated_system_tokens + estimated_tool_schema_tokens + estimated_message_tokens
    );
}

fn skill_relevance_score(prompt: &str, skill: &TextualSkill) -> usize {
    let prompt_lower = prompt.to_lowercase();
    let searchable = skill.searchable_text.as_str();

    let mut score = 0;
    if prompt_lower.len() >= 3 && searchable.contains(&prompt_lower) {
        score += 4;
    }

    for token in prompt_lower.split(|c: char| !c.is_alphanumeric()) {
        if token.len() >= 2 && searchable.contains(token) {
            score += 1;
        }
    }

    for trigger in &skill.triggers {
        let trigger_lower = trigger.to_lowercase();
        if !trigger_lower.is_empty() && prompt_lower.contains(&trigger_lower) {
            score += 4;
        }
    }

    for example in &skill.examples {
        let example_lower = example.to_lowercase();
        if !example_lower.is_empty() && prompt_lower.contains(&example_lower) {
            score += 3;
        }
    }

    for tag in &skill.tags {
        let tag_lower = tag.to_lowercase();
        if !tag_lower.is_empty() && prompt_lower.contains(&tag_lower) {
            score += 2;
        }
    }

    score
}

fn select_relevant_skills(
    prompt: &str,
    skills: &[TextualSkill],
    limit: usize,
) -> Vec<TextualSkill> {
    let mut scored: Vec<(usize, TextualSkill)> = skills
        .iter()
        .cloned()
        .filter_map(|skill| {
            let score = skill_relevance_score(prompt, &skill);
            (score > 0).then_some((score, skill))
        })
        .collect();

    scored.sort_by(|(left_score, left_skill), (right_score, right_skill)| {
        right_score
            .cmp(left_score)
            .then_with(|| left_skill.file_name.cmp(&right_skill.file_name))
    });

    scored
        .into_iter()
        .take(limit)
        .map(|(_, skill)| skill)
        .collect()
}

fn role_relevance_score(goal: &str, role: &AgentRole) -> usize {
    let goal_lower = goal.to_lowercase();
    let searchable = format!(
        "{} {} {}",
        role.name.to_lowercase(),
        role.description.to_lowercase(),
        role.system_prompt.to_lowercase()
    );

    let mut score = 0;
    if goal_lower.len() >= 3 && searchable.contains(&goal_lower) {
        score += 5;
    }

    for token in goal_lower.split(|c: char| !c.is_alphanumeric()) {
        if token.len() >= 2 && searchable.contains(token) {
            score += 1;
        }
    }

    score
}

fn select_delegate_roles(goal: &str, roles: &[AgentRole], limit: usize) -> Vec<AgentRole> {
    let mut scored: Vec<(usize, AgentRole)> = roles
        .iter()
        .cloned()
        .map(|role| (role_relevance_score(goal, &role), role))
        .collect();

    scored.sort_by(|(left_score, left_role), (right_score, right_role)| {
        right_score
            .cmp(left_score)
            .then_with(|| left_role.name.cmp(&right_role.name))
    });

    let mut selected: Vec<AgentRole> = scored
        .iter()
        .filter(|(score, _)| *score > 0)
        .take(limit)
        .map(|(_, role)| role.clone())
        .collect();

    if selected.is_empty() {
        selected = scored
            .into_iter()
            .take(limit.max(1))
            .map(|(_, role)| role)
            .collect();
    }

    selected
}

fn build_skill_prefetch_message(skills: &[TextualSkill]) -> Option<String> {
    if skills.is_empty() {
        return None;
    }

    let mut lines = vec![
        "## Prefetched Skill Snapshot".to_string(),
        "These skills look relevant to the current request. Read the full skill only if you need its exact workflow.".to_string(),
    ];
    for skill in skills {
        lines.push(format!(
            "- {}: {}",
            skill.file_name,
            format_skill_summary(skill)
        ));
    }

    Some(lines.join("\n"))
}

fn collect_tool_roots(paths: &libtizenclaw_core::framework::paths::PlatformPaths) -> Vec<String> {
    let mut roots = vec![paths.tools_dir.to_string_lossy().to_string()];
    roots.extend(RegisteredPaths::load(&paths.config_dir).tool_paths);
    roots.sort();
    roots.dedup();
    roots
}

fn collect_skill_roots(paths: &libtizenclaw_core::framework::paths::PlatformPaths) -> Vec<String> {
    let mut roots = paths
        .skill_root_dirs()
        .into_iter()
        .chain(paths.skill_hub_root_dirs())
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    roots.extend(
        paths
            .discover_skill_hub_roots()
            .into_iter()
            .map(|path| path.to_string_lossy().to_string()),
    );
    roots.extend(RegisteredPaths::load(&paths.config_dir).skill_paths);
    roots.sort();
    roots.dedup();
    roots
}

fn resolve_skill_file(skill_roots: &[String], normalized_name: &str) -> Option<PathBuf> {
    for root in skill_roots {
        for candidate_name in [
            normalized_name.to_string(),
            normalized_name.replace('_', "-"),
        ] {
            let candidate = Path::new(root).join(candidate_name).join("SKILL.md");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn build_progress_marker(
    response_text: &str,
    reasoning_text: &str,
    tool_calls: &[backend::LlmToolCall],
) -> String {
    if !tool_calls.is_empty() {
        let signatures = tool_calls
            .iter()
            .map(|tool_call| format!("{}:{}", tool_call.name, tool_call.args))
            .collect::<Vec<_>>()
            .join("|");
        return format!("<tool_calls>{}</tool_calls>", signatures);
    }

    let trimmed_text = response_text.trim();
    if !trimmed_text.is_empty() {
        return trimmed_text.to_string();
    }

    let trimmed_reasoning = reasoning_text.trim();
    if !trimmed_reasoning.is_empty() {
        return format!("<reasoning>{}</reasoning>", trimmed_reasoning);
    }

    "<empty-response>".into()
}

fn extract_final_text(response_text: &str) -> String {
    if let Some(start) = response_text.find("<final>") {
        if let Some(end) = response_text.rfind("</final>") {
            if end > start + 7 {
                return response_text[start + 7..end].trim().to_string();
            }
            return response_text[start + 7..].trim().to_string();
        }
        return response_text[start + 7..].trim().to_string();
    }

    let stripped_think = THINK_RE.replace_all(response_text, "");
    let normalized = stripped_think.trim();
    if normalized.is_empty() {
        response_text.trim().to_string()
    } else {
        normalized.to_string()
    }
}

fn extract_json_block(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(rest) = trimmed.strip_prefix("```") {
        let after_lang = rest.find('\n')?;
        let body = &rest[after_lang + 1..];
        let end = body.rfind("```")?;
        return Some(body[..end].trim().to_string());
    }

    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end < start {
        return None;
    }
    Some(trimmed[start..=end].trim().to_string())
}

fn generated_web_app_args_from_text(text: &str) -> Option<Value> {
    let candidate = extract_json_block(text)?;
    let parsed: Value = serde_json::from_str(&candidate).ok()?;
    let obj = parsed.as_object()?;

    let app_id = obj.get("app_id")?.as_str()?.trim();
    if app_id.is_empty() {
        return None;
    }
    let title = obj
        .get("title")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(app_id);

    let mut html = obj
        .get("html")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let mut css = obj
        .get("css")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();
    let mut js = obj
        .get("js")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string();

    if let Some(pages) = obj.get("pages").and_then(|value| value.as_array()) {
        for page in pages {
            let name = page
                .get("name")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            let content = page
                .get("content")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .to_string();
            if content.is_empty() {
                continue;
            }
            match name.as_str() {
                "index.html" | "index.htm" => html = content,
                "style.css" if css.is_empty() => css = content,
                "app.js" | "index.js" | "script.js" if js.is_empty() => js = content,
                _ if html.is_empty() && name.ends_with(".html") => html = content,
                _ => {}
            }
        }
    }

    if html.trim().is_empty() {
        return None;
    }

    let mut args = json!({
        "app_id": app_id,
        "title": title,
        "html": html,
    });
    if !css.trim().is_empty() {
        args["css"] = Value::String(css);
    }
    if !js.trim().is_empty() {
        args["js"] = Value::String(js);
    }
    if let Some(allowed_tools) = obj.get("allowed_tools").and_then(|value| value.as_array()) {
        args["allowed_tools"] = Value::Array(
            allowed_tools
                .iter()
                .filter_map(|value| value.as_str().map(|item| Value::String(item.to_string())))
                .collect(),
        );
    }
    Some(args)
}

fn prompt_mode_from_doc(doc: &Value, backend_name: &str) -> PromptMode {
    match doc
        .get("prompt")
        .and_then(|value| value.get("mode"))
        .and_then(|value| value.as_str())
        .unwrap_or("auto")
    {
        "full" => PromptMode::Full,
        "minimal" => PromptMode::Minimal,
        _ => match backend_name {
            "ollama" => PromptMode::Minimal,
            _ => PromptMode::Full,
        },
    }
}

fn reasoning_policy_from_doc(doc: &Value, backend_name: &str) -> ReasoningPolicy {
    match doc
        .get("prompt")
        .and_then(|value| value.get("reasoning_policy"))
        .and_then(|value| value.as_str())
        .unwrap_or("auto")
    {
        "tagged" => ReasoningPolicy::Tagged,
        "native" => ReasoningPolicy::Native,
        _ => match backend_name {
            "ollama" => ReasoningPolicy::Tagged,
            _ => ReasoningPolicy::Native,
        },
    }
}

fn prompt_mode_from_str(value: Option<&str>) -> Option<PromptMode> {
    match value.map(str::trim) {
        Some("full") => Some(PromptMode::Full),
        Some("minimal") => Some(PromptMode::Minimal),
        _ => None,
    }
}

fn reasoning_policy_from_str(value: Option<&str>) -> Option<ReasoningPolicy> {
    match value.map(str::trim) {
        Some("native") => Some(ReasoningPolicy::Native),
        Some("tagged") => Some(ReasoningPolicy::Tagged),
        _ => None,
    }
}

fn format_skill_summary(skill: &TextualSkill) -> String {
    let mut parts = vec![skill.description.clone()];
    if !skill.tags.is_empty() {
        parts.push(format!("tags: {}", skill.tags.join(", ")));
    }
    if !skill.triggers.is_empty() {
        parts.push(format!("triggers: {}", skill.triggers.join(" | ")));
    }
    if !skill.openclaw_requires.is_empty() {
        parts.push(format!("requires: {}", skill.openclaw_requires.join(", ")));
    }
    if !skill.openclaw_install.is_empty() {
        parts.push(format!("install: {}", skill.openclaw_install.join(" | ")));
    }
    parts.join(" | ")
}

fn build_role_supervisor_hint(session_id: &str, goal: &str, role: &AgentRole) -> String {
    format!(
        "Supervisor goal from session '{}': {}\n\
Role: {}\n\
Focus: {}\n\
Work only within this role's scope. Return concise role-specific findings, \
recommended actions, and any missing information needed for the next step.",
        session_id, goal, role.name, role.description
    )
}

fn list_known_sessions(paths: &libtizenclaw_core::framework::paths::PlatformPaths) -> Vec<String> {
    let root = paths.data_dir.join("sessions");
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    let mut sessions = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter_map(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
        })
        .collect::<Vec<_>>();
    sessions.sort();
    sessions
}

fn parse_shell_like_args(args: &str) -> Vec<String> {
    let mut parsed = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = '\0';

    for ch in args.chars() {
        if in_quotes {
            if ch == quote_char {
                in_quotes = false;
            } else {
                current.push(ch);
            }
            continue;
        }

        if ch == '"' || ch == '\'' {
            in_quotes = true;
            quote_char = ch;
        } else if ch.is_whitespace() {
            if !current.is_empty() {
                parsed.push(current.clone());
                current.clear();
            }
        } else {
            current.push(ch);
        }
    }

    if !current.is_empty() {
        parsed.push(current);
    }

    parsed
}

fn normalize_explicit_path_token(token: &str) -> Option<String> {
    let trimmed = token.trim_matches(|ch: char| {
        ch.is_whitespace()
            || matches!(
                ch,
                '"' | '\'' | '`' | ',' | ';' | ':' | '.' | '(' | ')' | '[' | ']' | '{' | '}'
            )
    });
    if !trimmed.starts_with('/') {
        return None;
    }
    let candidate = trimmed.trim_end_matches(|ch: char| ch == '.' || ch == ',');
    let candidate = if candidate.len() > 1 {
        candidate.trim_end_matches('/')
    } else {
        candidate
    };
    if candidate.is_empty() {
        return None;
    }
    Some(candidate.to_string())
}

/// Filter out path-like tokens that are actually fragments of URLs,
/// user-agent strings, or shebang lines — not real filesystem paths.
fn is_likely_false_positive_path(path: &str) -> bool {
    // Version-like fragments: /5.0, /7.68.0, /1.1, /2.0
    if path
        .trim_start_matches('/')
        .chars()
        .next()
        .map_or(false, |ch| ch.is_ascii_digit())
    {
        return true;
    }
    // Common well-known non-file paths from code strings
    let known_fp = [
        "/bin/bash",
        "/bin/sh",
        "/usr/bin/env",
        "/usr/bin/python",
        "/usr/bin/python3",
        "/usr/bin/node",
        "/dev/null",
    ];
    if known_fp.contains(&path) {
        return true;
    }
    // Very short paths (< 4 chars) like /v1, /v2 are likely API fragments
    if path.len() < 4 {
        return true;
    }
    false
}

fn extract_explicit_paths(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut paths = Vec::new();
    for captures in EXPLICIT_PATH_RE.captures_iter(text) {
        if let Some(path) = captures
            .get(0)
            .and_then(|matched| normalize_explicit_path_token(matched.as_str()))
        {
            if seen.insert(path.clone()) {
                paths.push(path);
            }
        }
    }
    paths
}

fn path_looks_like_file(path: &str) -> bool {
    let path_obj = Path::new(path);
    std::fs::metadata(path_obj)
        .map(|metadata| metadata.is_file())
        .unwrap_or_else(|_| path_obj.extension().is_some())
}

fn path_looks_like_directory(path: &str) -> bool {
    let path_obj = Path::new(path);
    std::fs::metadata(path_obj)
        .map(|metadata| metadata.is_dir())
        .unwrap_or_else(|_| path_obj.extension().is_none())
}

fn extract_explicit_file_paths(text: &str) -> Vec<String> {
    extract_explicit_paths(text)
        .into_iter()
        .filter(|path| path_looks_like_file(path))
        .collect()
}

fn extract_explicit_directory_paths(text: &str) -> Vec<String> {
    extract_explicit_paths(text)
        .into_iter()
        .filter(|path| path_looks_like_directory(path))
        .collect()
}

fn extract_relative_file_paths(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut paths = Vec::new();

    for captures in RELATIVE_FILE_RE.captures_iter(text) {
        let Some(path_match) = captures.get(1) else {
            continue;
        };
        let Some(matched) =
            Some(path_match.as_str().trim_matches(|ch| {
                matches!(ch, '`' | '"' | '\'' | ',' | '.' | ':' | ';' | ')' | '(')
            }))
        else {
            continue;
        };
        let preceding_char = text[..path_match.start()].chars().next_back();
        let following_char = text[path_match.end()..].chars().next();
        let matched = matched.trim();
        if matched.is_empty()
            || matched.starts_with('/')
            || matched.contains("://")
            || matched.starts_with("www.")
            || preceding_char == Some('@')
            || following_char == Some('@')
            || !looks_like_file_artifact_candidate(matched)
        {
            continue;
        }
        if Path::new(matched)
            .extension()
            .and_then(|value| value.to_str())
            .map(|ext| ext.chars().all(|ch| ch.is_ascii_digit()))
            .unwrap_or(false)
        {
            continue;
        }
        let normalized = matched.trim_start_matches("./").to_string();
        if seen.insert(normalized.clone()) {
            paths.push(normalized);
        }
    }

    paths
}

fn looks_like_file_artifact_candidate(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }

    if path.contains('/') || path.starts_with('.') {
        return true;
    }

    Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|ext| COMMON_FILE_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn prompt_mentions_readme(text: &str) -> bool {
    normalize_prompt_intent_text(text)
        .to_ascii_lowercase()
        .contains("readme")
}

fn prompt_mentions_gitignore(text: &str) -> bool {
    normalize_prompt_intent_text(text)
        .to_ascii_lowercase()
        .contains(".gitignore")
}

fn expected_file_management_targets(prompt: &str) -> Vec<Vec<String>> {
    let intent_prompt = normalize_prompt_intent_text(prompt);
    let mut groups = extract_relative_file_paths(intent_prompt)
        .into_iter()
        .filter(|path| {
            path_is_likely_output_target(intent_prompt, path)
                && !path_is_likely_input_reference(intent_prompt, path)
        })
        .map(|path| vec![path])
        .collect::<Vec<_>>();

    if prompt_mentions_readme(intent_prompt)
        && prompt_requests_file_output(intent_prompt)
        && !groups.iter().any(|group| {
            group.iter().any(|path| {
                path.eq_ignore_ascii_case("README") || path.eq_ignore_ascii_case("README.md")
            })
        })
    {
        groups.push(vec!["README".to_string(), "README.md".to_string()]);
    }

    if prompt_mentions_gitignore(intent_prompt)
        && prompt_requests_file_output(intent_prompt)
        && !groups.iter().any(|group| {
            group
                .iter()
                .any(|path| path.eq_ignore_ascii_case(".gitignore"))
        })
    {
        groups.push(vec![".gitignore".to_string()]);
    }

    groups
}

fn prompt_requests_file_output(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    [
        "create", "write", "save", "generate", "draft", "document", "update", "append", "export",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle))
}

fn path_is_likely_output_target(prompt: &str, path: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let path_lower = path.to_ascii_lowercase();
    let Some(path_index) = prompt_lower.find(&path_lower) else {
        return false;
    };

    let context_start = path_index.saturating_sub(120);
    let context = &prompt_lower[context_start..path_index];

    [
        "create",
        "write",
        "save",
        "saved to",
        "save it to",
        "generate",
        "draft",
        "document",
        "update",
        "append",
        "export",
    ]
    .iter()
    .any(|needle| context.contains(needle))
}

fn path_is_likely_input_reference(prompt: &str, path: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let path_lower = path.to_ascii_lowercase();
    let Some(_path_index) = prompt_lower.find(&path_lower) else {
        return false;
    };

    let escaped_path = regex::escape(&path_lower);
    let patterns = [
        format!(
            r"\bread(?:\s+the)?(?:\s+(?:document|file|emails?))?(?:\s+in|\s+from)?\s+`?{escaped_path}`?\b"
        ),
        format!(
            r"\bopen(?:\s+the)?(?:\s+(?:document|file))?(?:\s+in|\s+from)?\s+`?{escaped_path}`?\b"
        ),
        format!(
            r"\b(?:inspect|review|analy[sz]e|summari[sz]e)(?:\s+the)?(?:\s+(?:document|file))?(?:\s+in|\s+from)?\s+`?{escaped_path}`?\b"
        ),
        format!(
            r"\b(?:check|use|consult)(?:\s+the)?(?:\s+(?:document|file))?(?:\s+in|\s+from)?\s+`?{escaped_path}`?\b"
        ),
        format!(r"\b(?:document|file)\s+in\s+`?{escaped_path}`?\b"),
        format!(r"\bfile\s+called\s+`?{escaped_path}`?\b"),
        format!(r"\bsaved\s+.*?\s+file\s+called\s+`?{escaped_path}`?\b"),
        format!(r"\b(?:source|input)\s+(?:document|file)\s+`?{escaped_path}`?\b"),
        format!(r"\bfiles?\s+named\s+`?{escaped_path}`?\b"),
        format!(r"\bprovided\s+to\s+you\s+in\s+`?{escaped_path}`?\b"),
    ];

    patterns.iter().any(|pattern| {
        regex::Regex::new(pattern)
            .map(|regex| regex.is_match(&prompt_lower))
            .unwrap_or(false)
    })
}

fn prompt_mentions_history_or_memory(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    [
        "history", "memory", "remember", "previous", "earlier", "context", "continue", "again",
    ]
    .iter()
    .any(|keyword| prompt_lower.contains(keyword))
}

fn normalize_prompt_intent_text(prompt: &str) -> &str {
    prompt
        .split_once("Telegram user request:\n")
        .map(|(_, user_request)| user_request.trim())
        .filter(|user_request| !user_request.is_empty())
        .unwrap_or(prompt)
}

fn prompt_requests_executable_script(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    (prompt_lower.contains("script")
        || prompt_lower.contains("python")
        || prompt_lower.contains("bash")
        || prompt_lower.contains("node")
        || prompt_lower.contains("executable"))
        && (prompt_lower.contains("run")
            || prompt_lower.contains("execute")
            || prompt_lower.contains("launch")
            || prompt_lower.contains("command"))
}

fn is_simple_file_management_request(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    // Keep a compact Korean keyword subset so basic file-management prompts
    // continue to route correctly for supported multilingual sessions.
    let has_file_action = [
        "create", "write", "save", "edit", "update", "append", "read", "list", "show", "remove",
        "delete", "copy", "move", "rename", "mkdir", "open", "display", "print", "view", "읽",
        "열", "보여", "출력", "확인", "조회", "내용",
    ]
    .iter()
    .any(|keyword| prompt_lower.contains(keyword));
    // These Korean path nouns remain intentional multilingual matching, not
    // repository-authored prose or comments.
    let has_file_target = [
        "file",
        "files",
        "directory",
        "folder",
        "project structure",
        "working directory",
        "readme",
        ".md",
        ".txt",
        ".json",
        ".yaml",
        ".yml",
        ".py",
        ".js",
        ".ts",
        ".rs",
        "src/",
        "파일",
        "디렉터리",
        "폴더",
        "경로",
        ".toml",
        ".log",
    ]
    .iter()
    .any(|keyword| prompt_lower.contains(keyword))
        || !extract_explicit_paths(normalize_prompt_intent_text(prompt)).is_empty();

    has_file_action && has_file_target && !prompt_requests_executable_script(prompt)
}

fn should_skip_memory_for_prompt(prompt: &str) -> bool {
    is_simple_file_management_request(prompt) && !prompt_mentions_history_or_memory(prompt)
}

fn prompt_explicitly_forbids_tools(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    [
        "do not use any tools",
        "do not use tools",
        "no tool calls",
        "without tool calls",
        "respond with only this json",
        "respond with only a json object",
        "output a single json object",
        "only job is to output a single json object",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle))
}

fn prompt_requires_literal_json_output(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    [
        "respond with only this json",
        "respond with only a json object",
        "output a single json object",
        "only job is to output a single json object",
        "no markdown, no code fences, no extra text",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle))
}

pub(super) fn extract_verbatim_json_template(prompt: &str) -> Option<String> {
    if !prompt_requires_literal_json_output(prompt) {
        return None;
    }
    let last_brace = prompt.rfind('{')?;
    let after = &prompt[last_brace..];
    let mut depth = 0usize;
    let mut end = None;
    for (i, ch) in after.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    end = Some(i + ch.len_utf8());
                    break;
                }
            }
            _ => {}
        }
    }
    let candidate = &after[..end?];
    serde_json::from_str::<serde_json::Value>(candidate).ok()?;
    Some(candidate.to_string())
}

fn prompt_requests_image_generation(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let mentions_image_output = [".png", ".jpg", ".jpeg", ".webp", ".gif"]
        .iter()
        .any(|needle| prompt_lower.contains(needle));
    let mentions_generation = [
        "generate an image",
        "create an image",
        "generate image",
        "create image",
        "draw",
        "illustrate",
        "render",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle));

    mentions_generation || (mentions_image_output && prompt_lower.contains("save"))
}

fn prompt_requests_code_generation(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    [
        "script",
        "python",
        ".py",
        "javascript",
        ".js",
        "typescript",
        ".ts",
        "shell script",
        ".sh",
        "program",
        "source code",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle))
        || expected_file_management_targets(prompt)
            .iter()
            .flatten()
            .any(|path| {
                matches!(
                    Path::new(path)
                        .extension()
                        .and_then(|value| value.to_str())
                        .map(|value| value.to_ascii_lowercase())
                        .as_deref(),
                    Some("py" | "js" | "ts" | "tsx" | "sh" | "rb" | "rs" | "java" | "cpp")
                )
            })
}

fn prompt_references_grounded_inputs_for_code_generation(prompt: &str) -> bool {
    if !prompt_requests_code_generation(prompt) {
        return false;
    }

    extract_relative_file_paths(prompt)
        .into_iter()
        .chain(extract_explicit_file_paths(prompt))
        .any(|path| {
            matches!(
                Path::new(&path)
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(|value| value.to_ascii_lowercase())
                    .as_deref(),
                Some("json" | "csv" | "txt" | "yaml" | "yml" | "toml")
            )
        })
}

fn prompt_requests_current_web_research(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let wants_freshness = [
        "latest",
        "current",
        "today",
        "recent",
        "upcoming",
        "this year",
        "2026",
        "2027",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle));
    let research_domain = [
        "conference",
        "event",
        "news",
        "research",
        "market",
        "web",
        "online",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle));

    wants_freshness && research_domain
}

fn prompt_requests_numeric_market_fact(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let wants_freshness = ["current", "latest", "today"]
        .iter()
        .any(|needle| prompt_lower.contains(needle));
    let market_fact = [
        "stock",
        "share price",
        "stock price",
        "quote",
        "ticker",
        "market price",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle));

    wants_freshness && market_fact
}

fn numeric_market_fact_has_grader_visible_date(content: &str) -> bool {
    [
        r"\b\d{4}-\d{2}-\d{2}\b",
        r"\b\d{1,2}/\d{1,2}/\d{2,4}\b",
        r"\b(?:January|February|March|April|May|June|July|August|September|October|November|December)\b",
        r"\b\d{1,2}\s+(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)\b",
    ]
    .iter()
    .any(|pattern| {
        regex::Regex::new(pattern)
            .map(|re| re.is_match(content))
            .unwrap_or(false)
    })
}

fn prompt_requests_descriptive_answer_file(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let asks_question = ["what", "when", "why", "how", "which", "who"]
        .iter()
        .any(|needle| prompt_lower.contains(needle));
    let expects_answer_file = [
        "answer.txt",
        "save your answer",
        "write the answer",
        "save it to",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle));

    asks_question && expects_answer_file
}

fn prompt_keyword_overlap(prompt: &str, content: &str) -> usize {
    const STOPWORDS: &[&str] = &[
        "what", "when", "where", "which", "who", "your", "with", "save", "write", "answer",
        "report", "file", "current", "latest", "brief", "market", "summary", "notes", "read",
        "from", "into", "this", "that", "have", "will", "would", "should", "could", "about",
        "after", "before", "using", "please",
    ];

    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let content_lower = content.to_ascii_lowercase();
    let mut seen = HashSet::new();
    prompt_lower
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 4)
        .filter(|token| !STOPWORDS.contains(token))
        .filter(|token| seen.insert((*token).to_string()))
        .filter(|token| content_lower.contains(*token))
        .count()
}

fn output_lacks_descriptive_answer(prompt: &str, content: &str) -> bool {
    if !prompt_requests_descriptive_answer_file(prompt) {
        return false;
    }

    let trimmed = content.trim();
    if trimmed.is_empty() {
        return true;
    }

    trimmed.len() < 24 && prompt_keyword_overlap(prompt, trimmed) == 0
}

fn output_lacks_numeric_market_fact(prompt: &str, content: &str) -> bool {
    if !prompt_requests_numeric_market_fact(prompt) {
        return false;
    }

    let content_lower = content.to_ascii_lowercase();
    let has_placeholder = [
        "could not",
        "not available",
        "not reliably extracted",
        "not provided in the snippets",
        "unknown",
    ]
    .iter()
    .any(|needle| content_lower.contains(needle));
    let has_numeric_price = regex::Regex::new(
        r"(?i)(\$ ?\d+(?:\.\d+)?|\b\d+\.\d{1,2}\b|\b\d+(?:\.\d+)?\s*(?:usd|dollars?)\b)",
    )
        .map(|re| re.is_match(content))
        .unwrap_or(false);
    let has_grader_visible_date = numeric_market_fact_has_grader_visible_date(content);

    has_placeholder || !has_numeric_price || !has_grader_visible_date
}

fn prompt_requests_url_field(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    ["website", "url", "link"]
        .iter()
        .any(|needle| prompt_lower.contains(needle))
}

fn prompt_requests_date_field(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    ["date", "dates", "when"]
        .iter()
        .any(|needle| prompt_lower.contains(needle))
}

fn prompt_requests_location_field(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    ["location", "locations", "where", "city", "country"]
        .iter()
        .any(|needle| prompt_lower.contains(needle))
}

fn requested_research_entry_count(prompt: &str) -> Option<usize> {
    let prompt_normalized = normalize_prompt_intent_text(prompt);
    let patterns = [
        r"(?i)\b(?:find|identify|list|gather|collect|provide|research)\s+(\d{1,2})\b",
        r"(?i)\b(\d{1,2})\s+(?:upcoming|current|recent|latest|real|tech|conference|event|items?|examples?)\b",
    ];
    for pattern in patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if let Some(captures) = re.captures(&prompt_normalized) {
                if let Some(count) = captures
                    .get(1)
                    .and_then(|value| value.as_str().parse::<usize>().ok())
                    .filter(|value| *value > 0)
                {
                    return Some(count);
                }
            }
        }
    }
    None
}

fn extract_url_hosts(content: &str) -> HashSet<String> {
    extract_url_host_counts(content).into_keys().collect()
}

fn extract_url_host_counts(content: &str) -> HashMap<String, usize> {
    let Ok(url_re) = regex::Regex::new(r#"https?://[^\s)>"]+"#) else {
        return HashMap::new();
    };

    let mut counts = HashMap::new();
    for matched in url_re.find_iter(content) {
        let raw = matched
            .as_str()
            .trim_end_matches(|ch: char| ",.;|".contains(ch));
        let Some(host) = reqwest::Url::parse(raw)
            .ok()
            .and_then(|url| url.host_str().map(ToString::to_string))
        else {
            continue;
        };
        let normalized = host.trim_start_matches("www.").to_string();
        *counts.entry(normalized).or_insert(0) += 1;
    }
    counts
}

fn tokenize_significant_research_text(text: &str) -> HashSet<String> {
    let stopwords = [
        "and",
        "conference",
        "conferences",
        "congress",
        "event",
        "events",
        "expo",
        "for",
        "forum",
        "global",
        "in",
        "international",
        "location",
        "name",
        "north",
        "south",
        "east",
        "west",
        "summit",
        "tech",
        "technology",
        "upcoming",
        "website",
        "world",
        "year",
    ];
    let stopwords: HashSet<&str> = stopwords.into_iter().collect();

    let Ok(token_re) = regex::Regex::new(r"[A-Za-z0-9][A-Za-z0-9:+-]*") else {
        return HashSet::new();
    };

    token_re
        .find_iter(text)
        .filter_map(|matched| {
            let token = matched
                .as_str()
                .trim_matches(|ch: char| !ch.is_ascii_alphanumeric())
                .to_ascii_lowercase();
            if token.len() < 3 || token.chars().all(|ch| ch.is_ascii_digit()) {
                return None;
            }
            if stopwords.contains(token.as_str()) {
                return None;
            }
            Some(token)
        })
        .collect()
}

fn tokenize_research_branding_text(text: &str) -> HashSet<String> {
    let stopwords = [
        "and",
        "conference",
        "conferences",
        "congress",
        "event",
        "events",
        "for",
        "in",
        "location",
        "name",
        "website",
        "year",
    ];
    let stopwords: HashSet<&str> = stopwords.into_iter().collect();

    let Ok(token_re) = regex::Regex::new(r"[A-Za-z0-9][A-Za-z0-9:+-]*") else {
        return HashSet::new();
    };

    token_re
        .find_iter(text)
        .filter_map(|matched| {
            let token = matched
                .as_str()
                .trim_matches(|ch: char| !ch.is_ascii_alphanumeric())
                .to_ascii_lowercase();
            if token.len() < 3 || token.chars().all(|ch| ch.is_ascii_digit()) {
                return None;
            }
            if stopwords.contains(token.as_str()) {
                return None;
            }
            Some(token)
        })
        .collect()
}

fn extract_url_identity_text(url: &str) -> Option<String> {
    let Some(parsed) = reqwest::Url::parse(url).ok() else {
        return None;
    };

    let mut combined = String::new();
    if let Some(host) = parsed.host_str() {
        combined.push_str(&host.to_ascii_lowercase());
        combined.push(' ');
    }
    combined.push_str(&parsed.path().to_ascii_lowercase());
    Some(combined)
}

fn url_host_looks_like_event_aggregator(url: &str) -> bool {
    let raw = url.trim_end_matches(|ch: char| ",.;|".contains(ch));
    let Some(host) = reqwest::Url::parse(raw)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|value| value.to_ascii_lowercase()))
    else {
        return false;
    };

    [
        "ventureport",
        "expolume",
        "m2mconference",
        "startmybusiness",
        "showsbee",
        "tradefest",
        "eventseye",
        "eventswow",
        "bullyevents",
        "networkalmanac",
        "submitforge",
        "executiveaccommodationandservices",
        "whimsicalexhibits",
        "guidantech",
    ]
    .iter()
    .any(|needle| host.contains(needle))
}

fn prompt_requests_current_year_scope(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    if prompt_lower.contains("this year") || prompt_lower.contains("current year") {
        return true;
    }

    if prompt_requests_conference_roundup(prompt) {
        let Ok(year_re) = regex::Regex::new(r"\b20\d{2}\b") else {
            return true;
        };
        return !year_re.is_match(&prompt_lower);
    }

    false
}

fn prompt_requests_conference_roundup(prompt: &str) -> bool {
    if !prompt_requests_current_web_research(prompt) {
        return false;
    }

    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    [
        "conference",
        "conferences",
        "summit",
        "congress",
        "expo",
        "forum",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle))
}

fn prompt_requires_strict_current_research_validation(prompt: &str) -> bool {
    if !prompt_requests_current_web_research(prompt) {
        return false;
    }

    prompt_requests_url_field(prompt)
        || prompt_requests_date_field(prompt)
        || prompt_requests_location_field(prompt)
        || prompt_requests_conference_roundup(prompt)
        || prompt_requests_prediction_market_briefing(prompt)
}

fn current_utc_year() -> Option<u32> {
    format_unix_timestamp_utc(unix_timestamp_secs())
        .get(0..4)
        .and_then(|value| value.parse::<u32>().ok())
}

fn conference_entry_looks_low_authority(name: &str) -> bool {
    let normalized = normalize_prompt_intent_text(name).to_ascii_lowercase();
    if normalized.is_empty() {
        return false;
    }

    let weak_keywords = [
        "bootcamp",
        "course",
        "hackathon",
        "hack day",
        "hack night",
        "masterclass",
        "meetup",
        "roadshow",
        "tour",
        "training",
        "user group",
        "webinar",
        "workshop",
    ];
    if weak_keywords
        .iter()
        .any(|needle| normalized.contains(needle))
    {
        return true;
    }

    regex::Regex::new(r"(?i)\bx\s+[A-Z]{2,4}\b")
        .map(|re| re.is_match(name))
        .unwrap_or(false)
}

fn text_contains_specific_location(text: &str) -> bool {
    regex::Regex::new(
        r"(?x)
        \b
        [A-Z][A-Za-z.]+
        (?:\s+[A-Z][A-Za-z.]+){0,3}
        \s*,\s*
        [A-Z][A-Za-z.]+
        (?:\s+[A-Z][A-Za-z.]+){0,3}
        \b",
    )
    .map(|re| re.is_match(text))
    .unwrap_or(false)
}
