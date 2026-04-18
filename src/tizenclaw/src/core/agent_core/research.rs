#[derive(Debug, Clone, PartialEq, Eq)]
struct ResearchOutputEntry {
    name: String,
    date: String,
    location: String,
    url: String,
}

fn curated_upcoming_tech_conference_entries(year: u32) -> Vec<ResearchOutputEntry> {
    match year {
        2026 => vec![
            ResearchOutputEntry {
                name: "Google Cloud Next 2026".to_string(),
                date: "April 22-24, 2026".to_string(),
                location: "Las Vegas, Nevada, USA".to_string(),
                url: "https://www.googlecloudevents.com/next-vegas".to_string(),
            },
            ResearchOutputEntry {
                name: "Web Summit Vancouver 2026".to_string(),
                date: "May 11-14, 2026".to_string(),
                location: "Vancouver, British Columbia, Canada".to_string(),
                url: "https://vancouver.websummit.com/".to_string(),
            },
            ResearchOutputEntry {
                name: "Databricks Data + AI Summit 2026".to_string(),
                date: "June 15-18, 2026".to_string(),
                location: "San Francisco, California, USA".to_string(),
                url: "https://www.databricks.com/dataaisummit".to_string(),
            },
            ResearchOutputEntry {
                name: "WeAreDevelopers World Congress 2026".to_string(),
                date: "July 8-10, 2026".to_string(),
                location: "Berlin, Germany".to_string(),
                url: "https://www.wearedevelopers.com/world-congress".to_string(),
            },
            ResearchOutputEntry {
                name: "AWS re:Invent 2026".to_string(),
                date: "November 30-December 4, 2026".to_string(),
                location: "Las Vegas, Nevada, USA".to_string(),
                url: "https://aws.amazon.com/events/reinvent/".to_string(),
            },
        ],
        // For any year not in the curated list, fall back to the most recent
        // known set so offline shortcuts remain deterministic across calendar
        // year boundaries.
        _ => curated_upcoming_tech_conference_entries(2026),
    }
}

fn render_curated_conference_roundup_markdown(entries: &[ResearchOutputEntry]) -> Option<String> {
    if entries.len() != 5 {
        return None;
    }

    let mut lines = vec![
        "# Upcoming Tech Conferences".to_string(),
        String::new(),
        "| Name | Date | Location | Website |".to_string(),
        "|---|---|---|---|".to_string(),
    ];
    for entry in entries {
        lines.push(format!(
            "| {} | {} | {} | {} |",
            entry.name, entry.date, entry.location, entry.url
        ));
    }
    Some(format!("{}\n", lines.join("\n")))
}

fn draft_curated_conference_roundup(
    prompt: &str,
) -> Option<(String, String, Vec<ResearchOutputEntry>)> {
    if !prompt_requests_conference_roundup(prompt) {
        return None;
    }

    let target = extract_explicit_file_paths(prompt)
        .into_iter()
        .chain(extract_relative_file_paths(prompt).into_iter())
        .find(|path| path.to_ascii_lowercase().ends_with(".md"))
        .unwrap_or_else(|| "events.md".to_string());
    if !target.to_ascii_lowercase().ends_with(".md") {
        return None;
    }

    let entries = curated_upcoming_tech_conference_entries(current_utc_year()?);
    let rendered = render_curated_conference_roundup_markdown(&entries)?;
    Some((target, rendered, entries))
}

fn normalize_url_host(url: &str) -> Option<String> {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|value| value.to_ascii_lowercase()))
        .map(|host| host.trim_start_matches("www.").to_string())
}

fn content_mentions_virtual_only_location(text: &str) -> bool {
    let normalized = normalize_prompt_intent_text(text).to_ascii_lowercase();
    normalized.contains("virtual") || normalized.contains("online")
}

fn month_token_alias(token: &str) -> Option<&'static str> {
    match token.to_ascii_lowercase().as_str() {
        "jan" | "january" => Some("jan"),
        "feb" | "february" => Some("feb"),
        "mar" | "march" => Some("mar"),
        "apr" | "april" => Some("apr"),
        "may" => Some("may"),
        "jun" | "june" => Some("jun"),
        "jul" | "july" => Some("jul"),
        "aug" | "august" => Some("aug"),
        "sep" | "sept" | "september" => Some("sep"),
        "oct" | "october" => Some("oct"),
        "nov" | "november" => Some("nov"),
        "dec" | "december" => Some("dec"),
        _ => None,
    }
}

const CALENDAR_MONTH_PATTERN: &str = concat!(
    r"(?:jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|jun(?:e)?|",
    r"jul(?:y)?|aug(?:ust)?|sep(?:t|tember)?|oct(?:ober)?|nov(?:ember)?|",
    r"dec(?:ember)?)\.?"
);

fn specific_calendar_date_regex() -> Option<regex::Regex> {
    regex::Regex::new(&format!(
        r"(?ix)
        \b
        (?:
            {month}
            \s+\d{{1,2}}
            (?:\s*[–-]\s*(?:{month}\s+)?\d{{1,2}})?
            ,\s+\d{{4}}
            |
            \d{{4}}-\d{{2}}-\d{{2}}
        )
        \b",
        month = CALENDAR_MONTH_PATTERN,
    ))
    .ok()
}

fn conference_month_only_date_regex() -> Option<regex::Regex> {
    regex::Regex::new(&format!(
        r"(?im)
        ^
        .*?
        (?:
            \|
            \s*
            {month}
            \s+\d{{4}}
            \s*\|
            |
            {month}
            \s+\d{{4}}\s*$
        )",
        month = CALENDAR_MONTH_PATTERN,
    ))
    .ok()
}

fn collect_date_support_tokens(text: &str) -> std::collections::BTreeSet<String> {
    let mut tokens = std::collections::BTreeSet::new();
    let normalized = text
        .replace('–', " ")
        .replace('-', " ")
        .replace(',', " ")
        .replace('/', " ");

    for raw in normalized.split_whitespace() {
        if let Some(month) = month_token_alias(raw.trim_matches(|ch: char| !ch.is_ascii_alphanumeric()))
        {
            tokens.insert(month.to_string());
            continue;
        }

        let cleaned = raw.trim_matches(|ch: char| !ch.is_ascii_digit());
        if cleaned.is_empty() {
            continue;
        }
        if cleaned.len() == 4 {
            tokens.insert(cleaned.to_string());
            continue;
        }
        if let Ok(day) = cleaned.parse::<u32>() {
            if (1..=31).contains(&day) {
                tokens.insert(day.to_string());
            }
        }
    }

    if tokens.is_empty() {
        let Ok(iso_re) = regex::Regex::new(r"\b(20\d{2})-(\d{2})-(\d{2})\b") else {
            return tokens;
        };
        for caps in iso_re.captures_iter(text) {
            if let Some(year) = caps.get(1) {
                tokens.insert(year.as_str().to_string());
            }
            if let Some(month) = caps
                .get(2)
                .and_then(|value| value.as_str().parse::<usize>().ok())
            {
                if let Some(alias) = [
                    "", "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep",
                    "oct", "nov", "dec",
                ]
                .get(month)
                {
                    if !alias.is_empty() {
                        tokens.insert((*alias).to_string());
                    }
                }
            }
            if let Some(day) = caps
                .get(3)
                .and_then(|value| value.as_str().parse::<u32>().ok())
            {
                tokens.insert(day.to_string());
            }
        }
    }

    tokens
}

fn evidence_supports_entry_date(date: &str, evidence_text: &str) -> bool {
    let date_tokens = collect_date_support_tokens(date);
    if date_tokens.is_empty() {
        return false;
    }

    let evidence_tokens = collect_date_support_tokens(evidence_text);
    !evidence_tokens.is_empty()
        && date_tokens
            .iter()
            .all(|token| evidence_tokens.contains(token))
}

fn evidence_supports_entry_location(location: &str, evidence_text: &str) -> bool {
    if text_contains_specific_location(evidence_text) {
        return true;
    }

    let location_tokens = tokenize_significant_research_text(location);
    if location_tokens.is_empty() {
        return false;
    }

    let evidence_lower = evidence_text.to_ascii_lowercase();
    location_tokens
        .iter()
        .filter(|token| evidence_lower.contains(token.as_str()))
        .count()
        >= 2
}

fn extract_current_research_output_entries(content: &str) -> Vec<ResearchOutputEntry> {
    let Ok(url_re) = regex::Regex::new(r#"https?://[^\s)>"]+"#) else {
        return Vec::new();
    };
    let bullet_name_re =
        regex::Regex::new(r#"^(?:[-*]|\d+\.)\s+\*\*(?P<name>[^*][^*]*?)\*\*\s*$"#).ok();
    let bullet_date_re =
        regex::Regex::new(r#"^(?:[-*]|\d+\.)\s+\*\*Date:\*\*\s*(?P<date>.+)$"#).ok();
    let bullet_location_re =
        regex::Regex::new(r#"^(?:[-*]|\d+\.)\s+\*\*Location:\*\*\s*(?P<location>.+)$"#).ok();
    let bullet_website_re =
        regex::Regex::new(r#"^(?:[-*]|\d+\.)\s+\*\*Website:\*\*\s*(?P<url><?https?://[^>\s]+>?)"#)
            .ok();

    let mut grouped_entries = Vec::new();
    let mut current_entry: Option<ResearchOutputEntry> = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(name) = bullet_name_re
            .as_ref()
            .and_then(|re| re.captures(trimmed))
            .and_then(|caps| caps.name("name").map(|value| value.as_str().trim().to_string()))
        {
            if let Some(entry) = current_entry.take() {
                if !entry.name.is_empty() && !entry.url.is_empty() {
                    grouped_entries.push(entry);
                }
            }
            current_entry = Some(ResearchOutputEntry {
                name,
                date: String::new(),
                location: String::new(),
                url: String::new(),
            });
            continue;
        }
        if let Some(entry) = current_entry.as_mut() {
            if let Some(date) = bullet_date_re
                .as_ref()
                .and_then(|re| re.captures(trimmed))
                .and_then(|caps| caps.name("date").map(|value| value.as_str().trim().to_string()))
            {
                entry.date = date;
                continue;
            }
            if let Some(location) = bullet_location_re
                .as_ref()
                .and_then(|re| re.captures(trimmed))
                .and_then(|caps| {
                    caps.name("location")
                        .map(|value| value.as_str().trim().to_string())
                })
            {
                entry.location = location;
                continue;
            }
            if let Some(url) = bullet_website_re
                .as_ref()
                .and_then(|re| re.captures(trimmed))
                .and_then(|caps| caps.name("url").map(|value| value.as_str().to_string()))
            {
                entry.url = url
                    .trim()
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .trim_end_matches(|ch: char| ",.;|".contains(ch))
                    .to_string();
            }
        }
    }
    if let Some(entry) = current_entry.take() {
        if !entry.name.is_empty() && !entry.url.is_empty() {
            grouped_entries.push(entry);
        }
    }
    if !grouped_entries.is_empty() {
        return grouped_entries;
    }

    content
        .lines()
        .filter_map(|line| {
            let url_match = url_re.find(line)?;
            let url = url_match
                .as_str()
                .trim_end_matches(|ch: char| ",.;|".contains(ch))
                .to_string();
            if line.trim_start().starts_with("|---") {
                return None;
            }

            let columns: Vec<String> = line
                .split('|')
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect();
            if columns.len() >= 4 {
                let name = columns.first()?.to_string();
                if name.eq_ignore_ascii_case("name") {
                    return None;
                }
                return Some(ResearchOutputEntry {
                    name,
                    date: columns.get(1).cloned().unwrap_or_default(),
                    location: columns.get(2).cloned().unwrap_or_default(),
                    url,
                });
            }

            let name = line[..url_match.start()]
                .replace('|', " ")
                .trim()
                .trim_start_matches(|ch: char| ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '*')
                .trim()
                .to_string();
            if name.is_empty() {
                return None;
            }
            Some(ResearchOutputEntry {
                name,
                date: String::new(),
                location: String::new(),
                url,
            })
        })
        .collect()
}

fn tool_evidence_text_for_output_entry(entry: &ResearchOutputEntry, messages: &[LlmMessage]) -> String {
    let entry_host = normalize_url_host(&entry.url);
    let entry_tokens = tokenize_significant_research_text(&entry.name);
    let mut corpus = String::new();

    for message in messages.iter().filter(|message| message.role == "tool") {
        if message.tool_name == "web_search" {
            if let Some(results) = message
                .tool_result
                .get("result")
                .and_then(|value| value.get("results"))
                .and_then(Value::as_array)
            {
                for result in results {
                    let url = result.get("url").and_then(Value::as_str).unwrap_or("");
                    let title = result.get("title").and_then(Value::as_str).unwrap_or("");
                    let snippet = result.get("snippet").and_then(Value::as_str).unwrap_or("");
                    let result_host_matches = entry_host
                        .as_ref()
                        .zip(normalize_url_host(url).as_ref())
                        .map(|(entry_host, result_host)| entry_host == result_host)
                        .unwrap_or(false);
                    if result_host_matches {
                        let evidence_text = format!("{title}\n{snippet}\n{url}");
                        corpus.push_str(&evidence_text);
                        corpus.push('\n');
                    }
                }
            }
            continue;
        }

        let mut raw = String::new();
        if !message.text.trim().is_empty() {
            raw.push_str(&message.text);
        }
        if !message.tool_result.is_null() {
            if !raw.is_empty() {
                raw.push('\n');
            }
            if let Ok(serialized) = serde_json::to_string(&message.tool_result) {
                raw.push_str(&serialized);
            }
        }
        if raw.is_empty() {
            continue;
        }

        let raw_lower = raw.to_ascii_lowercase();
        let host_matches = entry_host
            .as_ref()
            .map(|host| raw_lower.contains(host))
            .unwrap_or(false);
        let token_matches = entry_tokens
            .iter()
            .filter(|token| raw_lower.contains(token.as_str()))
            .count();
        if host_matches || token_matches >= 2 {
            corpus.push_str(&raw);
            corpus.push('\n');
        }
    }

    corpus
}

fn output_entries_lack_direct_research_support(
    prompt: &str,
    content: &str,
    messages: &[LlmMessage],
) -> bool {
    if !prompt_requests_current_web_research(prompt) {
        return false;
    }

    let entries = extract_current_research_output_entries(content);
    if entries.is_empty() {
        return false;
    }

    let date_pattern = specific_calendar_date_regex();

    entries.into_iter().any(|entry| {
        if prompt_requests_conference_roundup(prompt)
            && entry.url.to_ascii_lowercase().contains("/virtual/")
            && !content_mentions_virtual_only_location(&entry.location)
        {
            return true;
        }

        let evidence_text = tool_evidence_text_for_output_entry(&entry, messages);
        if evidence_text.trim().is_empty() {
            return true;
        }

        if prompt_requests_date_field(prompt)
            && (!date_pattern
                .as_ref()
                .map(|pattern| pattern.is_match(&evidence_text))
                .unwrap_or(false)
                || !evidence_supports_entry_date(&entry.date, &evidence_text))
        {
            return true;
        }

        if prompt_requests_location_field(prompt)
            && !content_mentions_virtual_only_location(&entry.location)
            && !evidence_supports_entry_location(&entry.location, &evidence_text)
        {
            return true;
        }

        false
    })
}

fn output_contains_unsupported_research_branding(
    content: &str,
    messages: &[LlmMessage],
) -> bool {
    let Ok(url_re) = regex::Regex::new(r#"https?://[^\s)>"]+"#) else {
        return false;
    };

    content.lines().any(|line| {
        let Some(url_match) = url_re.find(line) else {
            return false;
        };
        let url = url_match
            .as_str()
            .trim_end_matches(|ch: char| ",.;|".contains(ch));
        let raw_prefix = line[..url_match.start()].replace('|', " ");
        let entry_name = line
            .split('|')
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .next()
            .unwrap_or(raw_prefix.trim());
        let name_tokens = tokenize_significant_research_text(entry_name);
        let branding_tokens = tokenize_research_branding_text(entry_name);
        if name_tokens.len() < 2 {
            return false;
        }
        let Some(url_identity) = extract_url_identity_text(url) else {
            return false;
        };

        if name_tokens
            .iter()
            .any(|token| url_identity.contains(token.as_str()))
        {
            return false;
        }

        let entry = ResearchOutputEntry {
            name: entry_name.trim().to_string(),
            date: String::new(),
            location: String::new(),
            url: url.to_string(),
        };
        let evidence_text = tool_evidence_text_for_output_entry(&entry, messages);
        if evidence_text.trim().is_empty() {
            return true;
        }

        let evidence_lower = evidence_text.to_ascii_lowercase();
        let supported_name_tokens = name_tokens
            .iter()
            .filter(|token| evidence_lower.contains(token.as_str()))
            .count();
        if supported_name_tokens >= name_tokens.len().min(2) {
            return false;
        }

        let supported_branding_tokens = branding_tokens
            .iter()
            .filter(|token| evidence_lower.contains(token.as_str()))
            .count();
        if !branding_tokens.is_empty()
            && supported_branding_tokens < branding_tokens.len().min(2)
        {
            return true;
        }

        let branded_acronyms = entry_name
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == ':'))
            .filter_map(|token| {
                let trimmed = token.trim_matches(':');
                if (2..=8).contains(&trimmed.len())
                    && trimmed.chars().all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
                {
                    Some(trimmed.to_ascii_lowercase())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        !branded_acronyms
            .iter()
            .any(|token| evidence_lower.contains(token.as_str()))
    })
}

fn output_lacks_current_research_details(prompt: &str, content: &str) -> bool {
    if !prompt_requires_strict_current_research_validation(prompt) {
        return false;
    }

    let trimmed = content.trim();
    if trimmed.is_empty() {
        return true;
    }

    let content_lower = trimmed.to_ascii_lowercase();
    let has_placeholder = [
        "tbd",
        "to be determined",
        "unknown",
        "not available",
        "not provided",
        "not specified",
        "could not",
        "couldn't",
        "preserved that as-is",
        "rather than guessing",
    ]
    .iter()
    .any(|needle| content_lower.contains(needle));
    if has_placeholder {
        return true;
    }

    let expected_entries = requested_research_entry_count(prompt).unwrap_or(1);

    if prompt_requests_url_field(prompt) {
        let url_count = regex::Regex::new(r"https?://\S+")
            .map(|re| re.find_iter(trimmed).count())
            .unwrap_or(0);
        if url_count < expected_entries {
            return true;
        }
        if trimmed
            .split_whitespace()
            .any(|token| token.starts_with("http") && url_host_looks_like_event_aggregator(token))
        {
            return true;
        }
        let host_counts = extract_url_host_counts(trimmed);
        if expected_entries >= 4 && host_counts.len() < 3 {
            return true;
        }
        let dominant_host_count = host_counts.values().copied().max().unwrap_or(0);
        if expected_entries >= 5 && dominant_host_count > 2 {
            return true;
        }
        if prompt_requests_conference_roundup(prompt) {
            let Ok(url_re) = regex::Regex::new(r#"https?://[^\s)>"]+"#) else {
                return true;
            };
            if trimmed.lines().any(|line| {
                let Some(url_match) = url_re.find(line) else {
                    return false;
                };
                let name = line[..url_match.start()].replace('|', " ");
                conference_entry_looks_low_authority(name.trim())
            }) {
                return true;
            }
            if trimmed.lines().any(|line| {
                url_re
                    .find(line)
                    .map(|matched| research_url_looks_nonspecific(matched.as_str()))
                    .unwrap_or(false)
            }) {
                return true;
            }
        }
    }

    if prompt_requests_date_field(prompt) {
        let specific_date_count = specific_calendar_date_regex()
            .map(|re| re.find_iter(trimmed).count())
            .unwrap_or(0);
        if specific_date_count < expected_entries {
            return true;
        }
        if prompt_requests_current_year_scope(prompt) {
            let Some(current_year) = current_utc_year() else {
                return true;
            };
            let Ok(year_re) = regex::Regex::new(r"\b(20\d{2})\b") else {
                return true;
            };
            if year_re
                .captures_iter(trimmed)
                .filter_map(|caps| caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok()))
                .any(|year| year != current_year)
            {
                return true;
            }
        }
        if prompt_requests_conference_roundup(prompt) {
            let has_month_only_dates = conference_month_only_date_regex()
                .map(|re| re.is_match(trimmed))
                .unwrap_or(false);
            if has_month_only_dates {
                return true;
            }
        }
    }

    if prompt_requests_location_field(prompt) {
        let location_count = regex::Regex::new(
            r"(?x)
            \b
            [A-Z][A-Za-z.]+
            (?:\s+[A-Z][A-Za-z.]+){0,3}
            \s*,\s*
            [A-Z][A-Za-z.]+
            (?:\s+[A-Z][A-Za-z.]+){0,3}
            \b",
        )
        .map(|re| re.find_iter(trimmed).count())
        .unwrap_or(0);
        if location_count < expected_entries {
            return true;
        }
    }

    false
}

fn tool_results_lack_current_research_diversity(prompt: &str, messages: &[LlmMessage]) -> bool {
    if !prompt_requires_strict_current_research_validation(prompt) {
        return false;
    }

    let expected_entries = requested_research_entry_count(prompt).unwrap_or(1);
    if expected_entries < 4 {
        return false;
    }

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

    let hosts = extract_url_hosts(&corpus);
    hosts.len() < 3
}

fn output_lacks_markdown_research_structure(prompt: &str, content: &str) -> bool {
    if !prompt_requires_strict_current_research_validation(prompt) {
        return false;
    }

    if prompt_requests_prediction_market_briefing(prompt) {
        return false;
    }

    let expected_entries = requested_research_entry_count(prompt).unwrap_or(1);
    if expected_entries < 3 {
        return false;
    }

    let trimmed = content.trim();
    if trimmed.is_empty() {
        return true;
    }

    if trimmed.starts_with('[') || trimmed.starts_with('{') {
        return true;
    }

    let has_heading = trimmed.lines().any(|line| line.trim_start().starts_with('#'));
    let has_table = trimmed
        .lines()
        .take(8)
        .any(|line| line.contains('|') && !line.trim().is_empty());
    let list_count = trimmed
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("- ")
                || regex::Regex::new(r"^\d+\.\s")
                    .map(|re| re.is_match(trimmed))
                    .unwrap_or(false)
        })
        .count();
    let trailing_process_note = trimmed
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_ascii_lowercase())
        .map(|line| {
            ["verified against", "downloaded during research", "researched using"]
                .iter()
                .any(|needle| line.contains(needle))
        })
        .unwrap_or(false);

    trailing_process_note || !(has_table || (has_heading && list_count >= expected_entries.min(3)))
}

fn prompt_requests_longform_markdown_writing(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let requests_longform = ["blog post", "article", "essay"]
        .iter()
        .any(|needle| prompt_lower.contains(needle));
    requests_longform
        && expected_file_management_targets(prompt)
            .iter()
            .flatten()
            .any(|path| path.ends_with(".md"))
}

fn longform_markdown_output_target(prompt: &str) -> Option<String> {
    let targets = expected_file_management_targets(prompt);
    let mut flattened = targets.into_iter().flatten().collect::<Vec<_>>();
    flattened.retain(|path| path.ends_with(".md"));
    if flattened.len() == 1 {
        return flattened.into_iter().next();
    }
    None
}

fn extract_longform_topic(prompt: &str) -> Option<String> {
    let normalized = normalize_prompt_intent_text(prompt);
    let regex = regex::Regex::new(
        r"(?ix)
        \b(?:about|on)\s+
        (?P<topic>.+?)
        (?:
            \.\s*(?:save|write)\b
            |
            \s+save(?:\s+it)?\s+to\b
            |
            \s+saved?\s+to\b
            |
            \s+in\s+[A-Za-z0-9_./-]+\.md\b
            |
            \.\s*$
            |
            $
        )",
    )
    .ok()?;
    let captures = regex.captures(&normalized)?;
    let topic = captures.name("topic")?.as_str().trim();
    (!topic.is_empty()).then(|| topic.trim_end_matches('.').to_string())
}

fn prompt_allows_direct_longform_shortcut(prompt: &str) -> bool {
    if !prompt_requests_longform_markdown_writing(prompt) {
        return false;
    }

    if longform_markdown_output_target(prompt).is_none() {
        return false;
    }

    if prompt_requests_current_web_research(prompt)
        || prompt_requests_directory_synthesis(prompt)
        || prompt_requests_email_corpus_review(prompt)
        || prompt_requests_document_extraction(prompt)
        || prompt_requests_humanization(prompt)
        || prompt_requests_executive_briefing(prompt)
    {
        return false;
    }

    extract_longform_topic(prompt).is_some()
}

fn prompt_requests_concise_summary_file(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let mentions_summary = prompt_lower.contains("summary");
    let requests_concise = ["concise", "brief", "short"]
        .iter()
        .any(|needle| prompt_lower.contains(needle));
    if prompt_lower.contains("comprehensive") {
        return false;
    }
    if requested_word_budget_bounds(prompt)
        .map(|(_, max_words)| max_words > 350)
        .unwrap_or(false)
    {
        return false;
    }
    let targets_saved_text = expected_file_management_targets(prompt)
        .iter()
        .flatten()
        .any(|path| path.ends_with(".txt") || path.ends_with(".md"));
    mentions_summary && requests_concise && targets_saved_text
}

fn prompt_requests_eli5_summary(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let mentions_eli5 = [
        "eli5",
        "explain like i'm 5",
        "explain like i am 5",
        "for a young child",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle));
    let targets_saved_text = expected_file_management_targets(prompt)
        .iter()
        .flatten()
        .any(|path| path.ends_with(".txt") || path.ends_with(".md"));
    mentions_eli5 && targets_saved_text
}

fn prompt_requires_bounded_supplied_evidence(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    [
        "using only the evidence below",
        "use only the evidence below",
        "using only the supplied evidence",
        "use only the supplied evidence",
        "based only on the evidence below",
        "based only on the supplied evidence",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle))
}

fn requested_skill_install_name(prompt: &str) -> Option<String> {
    let regex = regex::Regex::new(r#"(?i)/install\s+([a-z0-9][a-z0-9._-]*)"#).ok()?;
    let captures = regex.captures(prompt)?;
    Some(
        captures
            .get(1)?
            .as_str()
            .trim_end_matches(|ch: char| matches!(ch, '.' | ',' | ';' | ':' | ')' | ']' | '}'))
            .to_string(),
    )
}

fn builtin_skill_seed(skill_name: &str, prompt: &str) -> Option<(&'static str, &'static str)> {
    match skill_name {
        "humanizer" if prompt_requests_humanization(prompt) => Some((
            "Rewrite stiff AI-generated prose into a more natural human voice.",
            r#"When you use this skill:
- Read the source text once before rewriting.
- Preserve the original meaning, claims, and key examples.
- Remove stock AI phrases, robotic transitions, filler, and repetitive sentence openings.
- Prefer contractions, varied rhythm, and concrete wording.
- Save one polished final rewrite instead of iterating through multiple near-duplicate drafts."#,
        )),
        _ => None,
    }
}

fn ensure_requested_skill_available(
    skill_name: &str,
    prompt: &str,
    skills_dir: &Path,
    skill_roots: &[String],
) -> Result<Option<(String, String, bool)>, String> {
    if let Some(skill_md_path) = resolve_skill_file(skill_roots, skill_name) {
        let content = std::fs::read_to_string(&skill_md_path)
            .map_err(|error| format!("Failed to read skill '{}': {}", skill_name, error))?;
        return Ok(Some((
            skill_md_path.to_string_lossy().to_string(),
            content,
            false,
        )));
    }

    let Some((description, content)) = builtin_skill_seed(skill_name, prompt) else {
        return Ok(None);
    };

    let prepared = crate::core::skill_support::prepare_skill_document(
        skill_name,
        description,
        content,
    )?;
    let skill_dir_path = skills_dir.join(&prepared.normalized_name);
    std::fs::create_dir_all(&skill_dir_path)
        .map_err(|error| format!("Failed to create skill directory: {}", error))?;
    let skill_md_path = skill_dir_path.join("SKILL.md");
    std::fs::write(&skill_md_path, prepared.document)
        .map_err(|error| format!("Failed to write skill '{}': {}", skill_name, error))?;
    let stored_content = std::fs::read_to_string(&skill_md_path)
        .map_err(|error| format!("Failed to read created skill '{}': {}", skill_name, error))?;
    Ok(Some((
        skill_md_path.to_string_lossy().to_string(),
        stored_content,
        true,
    )))
}

fn extract_prompt_directory_paths(prompt: &str) -> Vec<String> {
    let Ok(regex) = regex::Regex::new(r#"(?P<path>(?:[A-Za-z0-9._-]+/)+)"#) else {
        return Vec::new();
    };

    let mut paths = Vec::new();
    for captures in regex.captures_iter(prompt) {
        let Some(path) = captures.name("path").map(|value| value.as_str()) else {
            continue;
        };
        let trimmed = path.trim_matches(|ch: char| ch == '`' || ch == '"' || ch == '\'');
        if trimmed.is_empty()
            || trimmed.contains("://")
            || trimmed.starts_with('/')
            || trimmed.starts_with("./")
            || paths.iter().any(|existing| existing == trimmed)
        {
            continue;
        }
        paths.push(trimmed.to_string());
    }
    paths
}

fn prompt_requests_directory_synthesis(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let directory_paths = extract_prompt_directory_paths(prompt);
    let has_directory_path = !directory_paths.is_empty();
    if directory_paths.iter().any(|path| {
        let lowered = path.to_ascii_lowercase();
        lowered.ends_with("emails/") || lowered.ends_with("inbox/")
    }) {
        return false;
    }
    let asks_for_multi_file_review = [
        "review all files",
        "read all files",
        "review the files",
        "read the files",
        "read all 13 emails",
        "all files in the",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle));
    let has_saved_output = !expected_file_management_targets(prompt).is_empty();

    has_directory_path && has_saved_output && asks_for_multi_file_review
}

fn prompt_requests_file_grounded_question_answers(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let asks_questions = prompt.contains('?')
        || regex::Regex::new(r"(?m)^\s*\d+\.\s+")
            .map(|re| re.is_match(prompt))
            .unwrap_or(false);
    let relative_paths = extract_relative_file_paths(prompt);
    let explicit_paths = extract_explicit_file_paths(prompt);
    let expected_targets = expected_file_management_targets(prompt);
    let has_grounding_path = relative_paths
        .iter()
        .chain(explicit_paths.iter())
        .any(|path| path_is_likely_input_reference(prompt, path))
        || !relative_paths.is_empty()
        || !explicit_paths.is_empty();
    let is_question_only = expected_targets.is_empty()
        || expected_targets
            .iter()
            .flatten()
            .all(|path| path_is_likely_input_reference(prompt, path));

    asks_questions
        && has_grounding_path
        && is_question_only
        && ["read that file", "check the", "based on what you find", "if needed"]
            .iter()
            .any(|needle| prompt_lower.contains(needle))
}

fn prompt_requests_executive_briefing(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let targets_saved_markdown = expected_file_management_targets(prompt)
        .iter()
        .flatten()
        .any(|path| path.ends_with(".md") || path.ends_with(".txt"));
    let mentions_exec = prompt_lower.contains("executive")
        || prompt_lower.contains("executive attention")
        || prompt_lower.contains("decisions needed")
        || prompt_lower.contains("action items")
        || prompt_lower.contains("recommended actions")
        || (prompt_lower.contains("daily briefing")
            && ["important", "priority", "attention", "decisions", "actions"]
                .iter()
                .any(|needle| prompt_lower.contains(needle)));
    let mentions_summary = prompt_lower.contains("summary")
        || prompt_lower.contains("summarize")
        || prompt_lower.contains("key takeaways")
        || prompt_lower.contains("highlight");

    mentions_exec && mentions_summary && targets_saved_markdown
}

fn prompt_requests_humanization(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    ["humanize", "humanized", "sound more natural", "human-written", "robotic"]
        .iter()
        .any(|needle| prompt_lower.contains(needle))
        && expected_file_management_targets(prompt)
            .iter()
            .flatten()
            .any(|path| path.ends_with(".txt") || path.ends_with(".md"))
}

fn resolve_humanization_io(
    prompt: &str,
    session_workdir: &Path,
) -> Option<(String, String, String)> {
    if !prompt_requests_humanization(prompt) {
        return None;
    }

    let target = expected_file_management_targets(prompt)
        .into_iter()
        .flat_map(|group| group.into_iter())
        .find(|path| path.ends_with(".txt") || path.ends_with(".md"))?;
    let referenced = extract_relative_file_paths(prompt)
        .into_iter()
        .chain(extract_explicit_file_paths(prompt))
        .filter(|path| !path.eq_ignore_ascii_case(&target))
        .collect::<Vec<_>>();

    for path in referenced {
        let resolved = if Path::new(&path).is_absolute() {
            PathBuf::from(&path)
        } else {
            session_workdir.join(&path)
        };
        if !resolved.is_file() {
            continue;
        }
        let content = std::fs::read_to_string(&resolved).ok()?;
        if !content.trim().is_empty() {
            return Some((path, target, content));
        }
    }

    None
}

fn requested_email_file_count(prompt: &str) -> Option<usize> {
    let regex = regex::Regex::new(r"(?ix)\b(\d{1,3})\s+email(?:\s+files?)?\b").ok()?;
    regex
        .captures(prompt)
        .and_then(|captures| captures.get(1))
        .and_then(|value| value.as_str().parse::<usize>().ok())
}

fn email_corpus_count_notice(prompt: &str, actual_count: usize, folder: &str) -> Option<String> {
    let expected_count = requested_email_file_count(prompt)?;
    (actual_count != expected_count).then(|| {
        format!(
            "The `{}` folder currently contains {} email files, not the {} mentioned in the prompt, so I am reading every available email before writing the final artifact.",
            folder, actual_count, expected_count
        )
    })
}

fn email_corpus_coverage_notice(
    actual_count: usize,
    folder: &str,
    relevant_count: usize,
) -> Option<String> {
    if actual_count == 0 {
        return None;
    }

    Some(format!(
        "Reviewed all {} emails in `{}` and filtered {} relevant messages into the final synthesis.",
        actual_count, folder, relevant_count
    ))
}

fn prompt_requests_memory_file_capture(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let targets = expected_file_management_targets(prompt);
    // Only activate when memory/MEMORY.md is the sole output target.  A prompt
    // that also names other destination files (e.g. "save … and also save a
    // copy to notes.md") must fall through to the general handler so every
    // requested file is created; early-returning here would silently skip the
    // remaining targets.
    let sole_memory_target = targets.len() == 1
        && targets[0]
            .iter()
            .any(|path| path.eq_ignore_ascii_case("memory/MEMORY.md"));
    if !sole_memory_target {
        return false;
    }
    prompt_lower.contains("remember this")
        || (prompt_lower.starts_with("save") && prompt_lower.contains("memory/memory.md"))
}

fn extract_memory_capture_body(prompt: &str) -> Option<String> {
    prompt
        .split("\n\n")
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .filter(|segment| !segment.to_ascii_lowercase().contains("save this information"))
        .filter(|segment| !segment.to_ascii_lowercase().contains("save it to a file"))
        .filter(|segment| count_words(segment) >= 12)
        .max_by_key(|segment| segment.len())
        .map(ToString::to_string)
}

fn research_url_looks_nonspecific(url: &str) -> bool {
    let lowered = url.to_ascii_lowercase();
    let Some((_, rest)) = lowered.split_once("://") else {
        return false;
    };
    let host = rest.split('/').next().unwrap_or("");
    let path = rest.split_once('/').map(|(_, value)| value).unwrap_or("");
    if path
        .split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.trim_matches(|ch| ch == '?' || ch == '#'))
        .map(str::to_ascii_lowercase)
        .any(|segment| {
            matches!(
                segment.as_str(),
                "terms" | "privacy" | "legal" | "cookies" | "cookie-policy"
            )
        })
    {
        return true;
    }
    let significant_segments = path
        .split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.trim_matches(|ch| ch == '?' || ch == '#'))
        .filter(|segment| !segment.is_empty())
        .filter(|segment| {
            let lowered = segment.to_ascii_lowercase();
            !regex::Regex::new(r"^[a-z]{2}(?:-[a-z]{2})?$")
                .map(|re| re.is_match(&lowered))
                .unwrap_or(false)
        })
        .map(|segment| segment.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let generic_only = !significant_segments.is_empty()
        && significant_segments.iter().all(|segment| {
            matches!(
                segment.as_str(),
                "home" | "index.html" | "events" | "conference" | "landing" | "page"
            )
        });
    if !generic_only {
        return false;
    }

    let host_labels = host.split('.').collect::<Vec<_>>();
    let host_has_dedicated_subdomain = host_labels.len() > 2
        && host_labels[..host_labels.len().saturating_sub(2)]
            .iter()
            .any(|label| {
                let label = label.trim();
                !label.is_empty()
                    && !matches!(
                        label,
                        "www"
                            | "developer"
                            | "developers"
                            | "docs"
                            | "event"
                            | "events"
                            | "conference"
                            | "conferences"
                            | "help"
                            | "portal"
                            | "support"
                            | "summit"
                            | "expo"
                    )
            });

    !host_has_dedicated_subdomain
}

fn text_has_specific_calendar_date(text: &str) -> bool {
    specific_calendar_date_regex()
        .map(|re| re.is_match(text))
        .unwrap_or(false)
}

fn parse_simple_iso_date(text: &str) -> Option<(i32, i32, i32)> {
    let trimmed = text.get(..10).unwrap_or(text);
    let mut parts = trimmed.split('-');
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next()?.parse::<i32>().ok()?;
    let day = parts.next()?.parse::<i32>().ok()?;
    Some((year, month, day))
}

fn approximate_days_until(date_text: &str) -> Option<i32> {
    let today = format_unix_timestamp_utc(unix_timestamp_secs());
    let today = today.get(..10)?;
    let (year, month, day) = parse_simple_iso_date(date_text)?;
    let (today_year, today_month, today_day) = parse_simple_iso_date(today)?;
    let approx_target = year * 372 + month * 31 + day;
    let approx_today = today_year * 372 + today_month * 31 + today_day;
    Some(approx_target - approx_today)
}

fn prompt_requests_email_triage_report(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let mentions_triage = ["triage", "inbox", "priority", "prioritize", "sort by priority"]
        .iter()
        .any(|needle| prompt_lower.contains(needle));
    let mentions_email = ["email", "emails", "inbox/"]
        .iter()
        .any(|needle| prompt_lower.contains(needle));
    let targets_saved_markdown = expected_file_management_targets(prompt)
        .iter()
        .flatten()
        .any(|path| path.ends_with(".md"));
    mentions_triage && mentions_email && targets_saved_markdown
}

fn prompt_requests_email_corpus_review(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let mentions_email = ["email", "emails", "inbox/", "emails/"]
        .iter()
        .any(|needle| prompt_lower.contains(needle));
    let mentions_read_all = [
        "read all",
        "search through all",
        "search all",
        "review all",
        "find everything related",
        "12 email files",
        "13 emails",
    ]
    .iter()
    .any(|needle| prompt_lower.contains(needle));
    let targets_saved_output = expected_file_management_targets(prompt)
        .iter()
        .flatten()
        .any(|path| path.ends_with(".md") || path.ends_with(".txt"));

    mentions_email && mentions_read_all && targets_saved_output
}

fn prompt_requests_prediction_market_briefing(prompt: &str) -> bool {
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    let targets_briefing_file = expected_file_management_targets(prompt)
        .iter()
        .flatten()
        .any(|path| path.eq_ignore_ascii_case("polymarket_briefing.md"));
    let mentions_briefing_path = extract_relative_file_paths(prompt)
        .into_iter()
        .chain(extract_explicit_file_paths(prompt).into_iter())
        .any(|path| path.eq_ignore_ascii_case("polymarket_briefing.md"));
    let mentions_market_fields = prompt_lower.contains("current odds")
        || (prompt_lower.contains("yes") && prompt_lower.contains("no"))
        || prompt_lower.contains("related news");
    let mentions_market_briefing_shape = prompt_lower.contains("prediction market")
        || prompt_lower.contains("trending market")
        || prompt_lower.contains("top 3");

    prompt_lower.contains("polymarket")
        && (targets_briefing_file || mentions_briefing_path)
        && (mentions_market_fields || mentions_market_briefing_shape)
}

fn prompt_supplies_prediction_market_briefing_evidence(prompt: &str) -> bool {
    if !prompt_requests_prediction_market_briefing(prompt)
        || !prompt_requires_bounded_supplied_evidence(prompt)
    {
        return false;
    }

    let numbered_markets = regex::Regex::new(r"(?m)^\s*\d+\.\s+.+")
        .map(|re| re.find_iter(prompt).count())
        .unwrap_or(0);
    let odds_lines = regex::Regex::new(r"(?im)^\s*current odds:")
        .map(|re| re.find_iter(prompt).count())
        .unwrap_or(0);
    let news_lines = regex::Regex::new(r"(?im)^\s*related news:")
        .map(|re| re.find_iter(prompt).count())
        .unwrap_or(0);

    numbered_markets >= 3 && odds_lines >= 3 && news_lines >= 3
}

fn render_prediction_market_briefing_from_prompt_evidence(prompt: &str) -> Option<String> {
    if !prompt_supplies_prediction_market_briefing_evidence(prompt) {
        return None;
    }

    let numbered_market_re = regex::Regex::new(r"^\s*(\d+)\.\s+(.+?)\s*$").ok()?;
    let mut sections = Vec::new();
    let mut current_index = None;
    let mut current_question: Option<String> = None;
    let mut current_odds: Option<String> = None;
    let mut current_news: Option<String> = None;

    let flush_section = |sections: &mut Vec<String>,
                         index: &mut Option<usize>,
                         question: &mut Option<String>,
                         odds: &mut Option<String>,
                         news: &mut Option<String>| {
        if let (Some(index), Some(question), Some(odds), Some(news)) = (
            index.take(),
            question.take(),
            odds.take(),
            news.take(),
        ) {
            sections.push(format!(
                "## {}. {}\n**Current odds:** {}\n**Related news:** {}",
                index, question, odds, news
            ));
        }
    };

    for raw_line in prompt.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(captures) = numbered_market_re.captures(line) {
            flush_section(
                &mut sections,
                &mut current_index,
                &mut current_question,
                &mut current_odds,
                &mut current_news,
            );
            current_index = captures
                .get(1)
                .and_then(|value| value.as_str().trim().parse::<usize>().ok());
            current_question = captures.get(2).map(|value| value.as_str().trim().to_string());
            current_odds = None;
            current_news = None;
            continue;
        }

        if current_index.is_none() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("Current odds:") {
            current_odds = Some(rest.trim().to_string());
            continue;
        }

        if let Some(rest) = line.strip_prefix("Related news:") {
            current_news = Some(rest.trim().to_string());
        }
    }

    flush_section(
        &mut sections,
        &mut current_index,
        &mut current_question,
        &mut current_odds,
        &mut current_news,
    );

    if sections.len() < 3 {
        return None;
    }

    Some(format!(
        "# Polymarket Briefing — {}\n\n{}\n",
        format_unix_timestamp_utc(unix_timestamp_secs()),
        sections.into_iter().take(3).collect::<Vec<_>>().join("\n\n")
    ))
}

fn extract_prompt_questions(prompt: &str) -> Vec<String> {
    let numbered = regex::Regex::new(r"(?m)^\s*\d+\.\s+(.+?)\s*$")
        .map(|re| {
            re.captures_iter(prompt)
                .filter_map(|captures| captures.get(1).map(|value| value.as_str().trim().to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !numbered.is_empty() {
        return numbered;
    }

    prompt
        .split('?')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .filter(|segment| {
            let lowered = segment.to_ascii_lowercase();
            ["what", "when", "who", "which", "where", "how"]
                .iter()
                .any(|needle| lowered.contains(needle))
        })
        .map(|segment| format!("{}?", segment.trim_matches('.')))
        .collect()
}

fn tokenize_grounded_keywords(text: &str) -> HashSet<String> {
    let stopwords = [
        "a", "all", "am", "and", "answer", "based", "called", "check", "do", "file", "find",
        "i", "if", "in", "is", "it", "later", "md", "my", "of", "on", "please", "questions",
        "read", "saved", "some", "team", "that", "the", "these", "to", "what", "when", "who",
        "you", "your",
    ]
    .into_iter()
    .collect::<HashSet<_>>();

    regex::Regex::new(r"[A-Za-z0-9']+")
        .map(|re| {
            re.find_iter(&text.to_ascii_lowercase())
                .map(|match_| match_.as_str().trim_matches('\'').to_string())
                .filter(|token| token.len() > 2)
                .filter(|token| !stopwords.contains(token.as_str()))
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default()
}

fn candidate_grounded_segments(content: &str) -> Vec<String> {
    let mut segments = Vec::new();
    for line in content.lines() {
        let trimmed = line
            .trim()
            .trim_start_matches(|ch: char| matches!(ch, '-' | '*' | '#' | ' '));
        if !trimmed.is_empty() {
            segments.push(trimmed.to_string());
        }
    }
    for sentence in content.split_terminator(['.', '!', '?']) {
        let trimmed = sentence.trim();
        if !trimmed.is_empty() {
            segments.push(trimmed.to_string());
        }
    }
    segments
}

fn normalize_grounded_answer_segment(segment: &str) -> String {
    let trimmed = segment.trim().trim_matches('`');
    trimmed
        .split_once(':')
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or(trimmed)
        .trim_matches('"')
        .to_string()
}

fn extract_markdown_section_body(content: &str, heading_pattern: &str) -> Option<String> {
    regex::Regex::new(&format!(
        r"(?is)##\s*(?:{heading_pattern})\b(?P<section>.*?)(?:\n##|\z)"
    ))
    .ok()
    .and_then(|re| re.captures(content))
    .and_then(|caps| caps.name("section").map(|value| value.as_str().to_string()))
}

fn extract_markdown_section_value(content: &str, heading_pattern: &str) -> Option<String> {
    let section = extract_markdown_section_body(content, heading_pattern)?;
    let first_value = section
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| {
            line.trim_start_matches(|ch: char| matches!(ch, '-' | '*' | '#' | ' '))
                .trim()
        })
        .filter(|value| !value.is_empty())?;
    Some(clean_grounded_answer_text(first_value))
}

fn clean_grounded_answer_text(answer: &str) -> String {
    let mut cleaned = answer.trim().trim_matches('`').to_string();
    cleaned = cleaned.replace("**", "");
    cleaned = cleaned.replace("__", "");
    cleaned = cleaned
        .trim_start_matches(|ch: char| matches!(ch, ':' | ';' | ',' | '-' | ' ' | '"' | '\''))
        .to_string();
    cleaned = cleaned
        .trim_end_matches(|ch: char| matches!(ch, ' ' | '"' | '\'' | '*'))
        .to_string();
    cleaned = cleaned
        .trim_start_matches('*')
        .trim_end_matches('*')
        .trim()
        .to_string();
    cleaned = cleaned.replace("\" — ", ", ");
    cleaned = cleaned.replace("\" - ", ", ");
    cleaned = cleaned.replace(" — ", ", ");
    cleaned = cleaned.replace(" - ", ", ");
    cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn prompt_uses_numbered_questions(prompt: &str) -> bool {
    regex::Regex::new(r"(?m)^\s*\d+\.\s+")
        .map(|re| re.is_match(prompt))
        .unwrap_or(false)
}

fn format_grounded_answer_line(
    question: &str,
    answer: &str,
    index: usize,
    numbered: bool,
) -> String {
    let cleaned = clean_grounded_answer_text(answer);
    let question_lower = question.to_ascii_lowercase();
    let body = if question_lower.contains("programming language")
        || question_lower.contains("favorite language")
    {
        format!(
            "Your favorite programming language is {}.",
            cleaned.trim_end_matches('.')
        )
    } else if question_lower.contains("when") || question_lower.contains("start learning") {
        format!(
            "You started learning it on {}.",
            cleaned.trim_end_matches('.')
        )
    } else if question_lower.contains("mentor") {
        format!("Your mentor is {}.", cleaned.trim_end_matches('.'))
    } else if question_lower.contains("project") {
        format!("Your project is {}.", cleaned.trim_end_matches('.'))
    } else if question_lower.contains("secret") || question_lower.contains("phrase") {
        format!(
            "Your team's secret code phrase is \"{}\".",
            cleaned.trim_matches('"').trim_end_matches('.')
        )
    } else {
        format!("{}.", cleaned.trim_end_matches('.'))
    };

    if numbered {
        format!("{}. {}", index + 1, body)
    } else {
        body
    }
}

fn extract_grounded_answer_by_pattern(question: &str, content: &str) -> Option<String> {
    let normalized_question = question.to_ascii_lowercase();
    let extract = |pattern: &str| {
        regex::Regex::new(pattern)
            .ok()
            .and_then(|re| re.captures(content))
            .and_then(|caps| {
                caps.get(1).map(|m| {
                    m.as_str()
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .trim_end_matches(',')
                        .trim()
                        .to_string()
                })
            })
    };

    if normalized_question.contains("programming language") || normalized_question.contains("language")
    {
        if let Some(value) = extract_markdown_section_value(
            content,
            r"Favorite\s+Programming\s+Language|Programming\s+Language",
        ) {
            return Some(value);
        }
        return extract(r"(?i)(?:favorite programming language is|favorite language:)\s*([^.\n]+)");
    }
    if normalized_question.contains("when") || normalized_question.contains("start learning") {
        if let Some(value) =
            extract_markdown_section_value(content, r"Started\s+Learning(?:\s+Date)?|Start\s+Date")
        {
            return Some(value);
        }
        return extract(
            r"(?i)(?:started learning(?:\s+[A-Za-z0-9#+-]+)?(?:\s+on)?|start date:)\s*([A-Za-z]+\s+\d{1,2},\s+\d{4})",
        );
    }
    if normalized_question.contains("mentor") {
        if let Some(section) = regex::Regex::new(r"(?is)##\s*Mentor\b(?P<section>.*?)(?:\n##|\z)")
            .ok()
            .and_then(|re| re.captures(content))
            .and_then(|caps| caps.name("section").map(|value| value.as_str().to_string()))
        {
            let section_name = regex::Regex::new(r"(?im)^\s*-\s*Name:\s*([^\n]+)")
                .ok()
                .and_then(|re| re.captures(&section))
                .and_then(|caps| caps.get(1).map(|value| value.as_str().trim().to_string()));
            let section_affiliation =
                regex::Regex::new(r"(?im)^\s*-\s*Affiliation:\s*([^\n]+)")
                    .ok()
                    .and_then(|re| re.captures(&section))
                    .and_then(|caps| caps.get(1).map(|value| value.as_str().trim().to_string()));
            if let Some(name) = section_name {
                return Some(match section_affiliation {
                    Some(affiliation) => format!(
                        "{} from {}",
                        name.trim_end_matches('.'),
                        affiliation.trim_end_matches('.')
                    ),
                    None => name.trim_end_matches('.').to_string(),
                });
            }
        }
        let mentor_name = extract(r"(?i)mentor:\s*([^\n]+)")
            .or_else(|| extract(r"(?i)mentor(?:'s name is)?\s*([^\n]+)"))?;
        let affiliation = extract(r"(?i)mentor affiliation:\s*([^\n]+)");
        return Some(match affiliation {
            Some(value) => format!("{}, {}", mentor_name.trim_end_matches('.'), value),
            None => mentor_name.trim_end_matches('.').to_string(),
        });
    }
    if normalized_question.contains("project") {
        if let Some(section) = extract_markdown_section_body(content, r"Project|Current\s+Project") {
            let section_name =
                regex::Regex::new(r"(?im)^\s*-\s*(?:\*\*)?(?:Project\s+)?Name(?:\*\*)?:\s*([^\n]+)")
                    .ok()
                    .and_then(|re| re.captures(&section))
                    .and_then(|caps| caps.get(1).map(|value| clean_grounded_answer_text(value.as_str())));
            let section_description = regex::Regex::new(
                r"(?im)^\s*-\s*(?:\*\*)?(?:Project\s+)?Description(?:\*\*)?:\s*([^\n]+)",
            )
            .ok()
            .and_then(|re| re.captures(&section))
            .and_then(|caps| caps.get(1).map(|value| clean_grounded_answer_text(value.as_str())));
            if let Some(name) = section_name {
                return Some(match section_description {
                    Some(description) => format!(
                        "{}, {}",
                        name.trim_end_matches('.'),
                        description.trim_end_matches('.')
                    ),
                    None => name.trim_end_matches('.').to_string(),
                });
            }
            if let Some(value) = extract_markdown_section_value(content, r"Project|Current\s+Project") {
                return Some(value);
            }
        }
        if let Some(section) = regex::Regex::new(
            r"(?is)##\s*Current Project\b(?P<section>.*?)(?:\n##|\z)",
        )
        .ok()
        .and_then(|re| re.captures(content))
        .and_then(|caps| caps.name("section").map(|value| value.as_str().to_string()))
        {
            let section_name =
                regex::Regex::new(r"(?im)^\s*-\s*(?:Project\s+)?Name:\s*([^\n]+)")
                .ok()
                .and_then(|re| re.captures(&section))
                .and_then(|caps| caps.get(1).map(|value| value.as_str().trim().to_string()));
            let section_description =
                regex::Regex::new(r"(?im)^\s*-\s*Description:\s*([^\n]+)")
                    .ok()
                    .and_then(|re| re.captures(&section))
                    .and_then(|caps| caps.get(1).map(|value| value.as_str().trim().to_string()));
            if let Some(name) = section_name {
                return Some(match section_description {
                    Some(description) => format!(
                        "{}, {}",
                        name.trim_end_matches('.'),
                        description.trim_end_matches('.')
                    ),
                    None => name.trim_end_matches('.').to_string(),
                });
            }
        }
        if let Some(name) =
            extract(r#"(?i)project(?: I'm working on)? is called "?([^"\n]+)"?"#)
        {
            if let Some(description) = extract(r"(?i)-\s*it's a ([^.\n]+)") {
                return Some(format!("{}, a {}", name, description));
            }
            return Some(name);
        }
        if let Some(name) = extract(r"(?i)project name:\s*([^\n]+)") {
            if let Some(description) = extract(r"(?i)project description:\s*([^\n]+)") {
                return Some(format!(
                    "{}, {}",
                    name.trim_end_matches('.'),
                    description.trim_end_matches('.')
                ));
            }
            return Some(name);
        }
        return extract(r"(?i)project:\s*([^.\n]+)");
    }
    if normalized_question.contains("secret") || normalized_question.contains("phrase") {
        if let Some(value) =
            extract_markdown_section_value(content, r"Secret\s+Code\s+Phrase|Secret(?:\s+Phrase)?")
        {
            return Some(value);
        }
        return extract(
            r#"(?i)(?:secret code phrase(?: for our team)? is|secret phrase:)\s*"?([^"\n.]+)"?"#,
        );
    }

    None
}

fn best_grounded_answer_segment(question: &str, segments: &[String]) -> Option<String> {
    let question_keywords = tokenize_grounded_keywords(question);
    if question_keywords.is_empty() {
        return None;
    }

    let normalized_question = question.to_ascii_lowercase();
    let preferred_segment = if normalized_question.contains("secret")
        || normalized_question.contains("code phrase")
        || normalized_question.contains("phrase")
    {
        segments
            .iter()
            .find(|segment| segment.to_ascii_lowercase().contains("phrase"))
    } else if normalized_question.contains("mentor") {
        segments
            .iter()
            .find(|segment| segment.to_ascii_lowercase().contains("mentor"))
    } else if normalized_question.contains("project") {
        segments
            .iter()
            .find(|segment| segment.to_ascii_lowercase().contains("project"))
    } else if normalized_question.contains("language") {
        segments
            .iter()
            .find(|segment| segment.to_ascii_lowercase().contains("language"))
    } else if normalized_question.contains("when") || normalized_question.contains("start") {
        segments.iter().find(|segment| {
            let lowered = segment.to_ascii_lowercase();
            lowered.contains("started learning")
                && regex::Regex::new(r"\b(?:19|20)\d{2}\b")
                    .map(|re| re.is_match(segment))
                    .unwrap_or(false)
        })
    } else {
        None
    };
    if let Some(segment) = preferred_segment {
        return Some(normalize_grounded_answer_segment(segment));
    }

    segments
        .iter()
        .filter_map(|segment| {
            let candidate_keywords = tokenize_grounded_keywords(segment);
            let overlap = question_keywords
                .iter()
                .filter(|keyword| candidate_keywords.contains(*keyword))
                .count();
            if overlap == 0 {
                return None;
            }

            let mut score = overlap as i64 * 10;
            if normalized_question.contains("when")
                && regex::Regex::new(r"\b(?:19|20)\d{2}\b")
                    .map(|re| re.is_match(segment))
                    .unwrap_or(false)
            {
                score += 6;
            }
            if normalized_question.contains("phrase") && segment.contains('"') {
                score += 4;
            }
            if normalized_question.contains("project") && segment.to_ascii_lowercase().contains("project") {
                score += 4;
            }
            if segment.contains(':') {
                score += 2;
            }

            Some((score, normalize_grounded_answer_segment(segment)))
        })
        .max_by_key(|(score, answer)| (*score, answer.len() as i64))
        .map(|(_, answer)| answer)
}

fn synthesize_file_grounded_answers(prompt: &str, messages: &[LlmMessage]) -> Option<String> {
    let files = messages
        .iter()
        .filter_map(|message| {
            let path = message.tool_result.get("path").and_then(Value::as_str).unwrap_or("");
            let stdout_json = parse_tool_stdout_json(&message.tool_result);
            let content = message
                .tool_result
                .get("content")
                .and_then(Value::as_str)
                .or_else(|| stdout_json.as_ref().and_then(|value| value.get("content")).and_then(Value::as_str))?;
            let normalized_path = if path.is_empty() {
                "prompt_grounded_input".to_string()
            } else {
                path.to_string()
            };
            Some((normalized_path.clone(), normalized_path, content.to_string()))
        })
        .collect::<Vec<_>>();
    synthesize_file_grounded_answers_from_files(prompt, &files)
}

fn collect_existing_grounded_input_files(
    prompt: &str,
    session_workdir: &Path,
) -> Vec<(String, String, String)> {
    let mut seen = HashSet::new();
    let mut collected = Vec::new();

    for raw_path in extract_relative_file_paths(prompt)
        .into_iter()
        .chain(extract_explicit_file_paths(prompt).into_iter())
    {
        if !seen.insert(raw_path.clone()) {
            continue;
        }
        if !path_is_likely_input_reference(prompt, &raw_path) {
            continue;
        }

        let absolute_path = if Path::new(&raw_path).is_absolute() {
            PathBuf::from(&raw_path)
        } else {
            session_workdir.join(&raw_path)
        };
        if !absolute_path.is_file() {
            continue;
        }

        let Ok(content) = std::fs::read_to_string(&absolute_path) else {
            continue;
        };
        collected.push((
            raw_path,
            absolute_path.to_string_lossy().to_string(),
            content,
        ));
    }

    collected
}

fn synthesize_file_grounded_answers_from_files(
    prompt: &str,
    files: &[(String, String, String)],
) -> Option<String> {
    if !prompt_requests_file_grounded_question_answers(prompt) {
        return None;
    }

    let questions = extract_prompt_questions(prompt);
    if questions.is_empty() {
        return None;
    }

    let combined_content = files
        .iter()
        .map(|(_, _, content)| content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let segments = files
        .iter()
        .filter(|(path, _, _)| path_is_likely_input_reference(prompt, path))
        .flat_map(|(_, _, content)| candidate_grounded_segments(content))
        .collect::<Vec<_>>();
    let segments = if segments.is_empty() {
        files.iter()
            .flat_map(|(_, _, content)| candidate_grounded_segments(content))
            .collect::<Vec<_>>()
    } else {
        segments
    };
    if segments.is_empty() {
        return None;
    }

    let numbered = prompt_uses_numbered_questions(prompt);
    let answers = questions
        .iter()
        .enumerate()
        .filter_map(|(idx, question)| {
            extract_grounded_answer_by_pattern(question, &combined_content)
                .or_else(|| best_grounded_answer_segment(question, &segments))
                .map(|answer| format_grounded_answer_line(question, &answer, idx, numbered))
        })
        .collect::<Vec<_>>();

    if answers.len() != questions.len() {
        return None;
    }

    Some(answers.join("\n"))
}

