async fn prefetch_polymarket_market_snapshot(
    session_workdir: &Path,
) -> Result<(String, String), String> {
    async fn fetch_gamma_snapshot(url: &str) -> Result<Vec<Value>, String> {
        let response = crate::generic::infra::http_client::default_client()
            .get(url)
            .timeout(std::time::Duration::from_secs(4))
            .send()
            .await
            .map_err(|error| format!("Failed to fetch Polymarket Gamma API: {}", error))?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!(
                "Polymarket Gamma API returned HTTP {}",
                status.as_u16()
            ));
        }

        let body = response
            .text()
            .await
            .map_err(|error| format!("Failed to read Polymarket Gamma API body: {}", error))?;
        let parsed = serde_json::from_str::<Value>(&body)
            .map_err(|error| format!("Failed to parse Polymarket Gamma API JSON: {}", error))?;
        Ok(parsed.as_array().cloned().unwrap_or_default())
    }

    fn trim_polymarket_snapshot_entries(entries: &[Value]) -> Vec<Value> {
        entries
            .iter()
            .map(|entry| {
                json!({
                    "question": entry.get("question").and_then(Value::as_str).unwrap_or(""),
                    "volumeNum": entry.get("volumeNum").cloned().unwrap_or(Value::Null),
                    "volume24hr": entry.get("volume24hr").cloned().unwrap_or(Value::Null),
                    "outcomes": entry.get("outcomes").cloned().unwrap_or(Value::Null),
                    "outcomePrices": entry.get("outcomePrices").cloned().unwrap_or(Value::Null),
                    "endDate": entry.get("endDate").cloned().unwrap_or(Value::Null),
                    "endDateIso": entry.get("endDateIso").cloned().unwrap_or(Value::Null),
                    "updatedAt": entry.get("updatedAt").cloned().unwrap_or(Value::Null),
                    "description": entry.get("description").cloned().unwrap_or(Value::Null),
                    "active": entry.get("active").cloned().unwrap_or(Value::Null),
                    "slug": entry.get("slug").cloned().unwrap_or(Value::Null),
                })
            })
            .collect()
    }

    let mut merged = Vec::new();
    let mut seen_questions = HashSet::new();
    let mut last_error = None;
    let feeds = [
        "https://gamma-api.polymarket.com/markets?active=true&order=volumeNum&ascending=false&limit=10",
        "https://gamma-api.polymarket.com/markets?active=true&order=volume24hr&ascending=false&limit=40",
    ];
    for url in feeds {
        let snapshot = match fetch_gamma_snapshot(url).await {
            Ok(entries) => entries,
            Err(error) => {
                last_error = Some(error);
                continue;
            }
        };
        for entry in snapshot {
            let question = entry
                .get("question")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            if question.is_empty() || !seen_questions.insert(question) {
                continue;
            }
            merged.push(entry);
        }
    }
    if merged.is_empty() {
        return Err(last_error
            .unwrap_or_else(|| "Polymarket Gamma API returned no active markets".to_string()));
    }

    let trimmed = trim_polymarket_snapshot_entries(&merged);

    let snapshot_path = session_workdir.join("polymarket_markets.json");
    let content = serde_json::to_string_pretty(&trimmed)
        .map_err(|error| format!("Failed to serialize Polymarket snapshot: {}", error))?;
    std::fs::write(&snapshot_path, &content)
        .map_err(|error| format!("Failed to persist Polymarket snapshot: {}", error))?;

    Ok((snapshot_path.to_string_lossy().to_string(), content))
}

fn polymarket_yes_no_percentages(entry: &Value) -> Option<(u32, u32)> {
    let outcomes = entry.get("outcomes")?.as_str()?;
    let outcome_prices = entry.get("outcomePrices")?.as_str()?;
    let outcomes = serde_json::from_str::<Vec<String>>(outcomes).ok()?;
    let prices = serde_json::from_str::<Vec<String>>(outcome_prices).ok()?;
    if outcomes.len() != prices.len() {
        return None;
    }

    let mut yes = None;
    let mut no = None;
    for (outcome, price) in outcomes.iter().zip(prices.iter()) {
        let parsed = price.parse::<f64>().ok()?;
        if outcome.eq_ignore_ascii_case("yes") {
            yes = Some(parsed);
        } else if outcome.eq_ignore_ascii_case("no") {
            no = Some(parsed);
        }
    }
    let yes = yes?;
    let no = no?;
    let total = yes + no;
    if total <= f64::EPSILON {
        return None;
    }
    let yes_pct = ((yes / total) * 100.0).floor().clamp(0.0, 100.0) as u32;
    Some((yes_pct, 100_u32.saturating_sub(yes_pct)))
}

fn polymarket_numeric_field(entry: &Value, field: &str) -> Option<f64> {
    entry.get(field).and_then(|value| match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    })
}

fn polymarket_primary_volume(entry: &Value) -> f64 {
    polymarket_numeric_field(entry, "volumeNum")
        .or_else(|| polymarket_numeric_field(entry, "volume24hr"))
        .unwrap_or(0.0)
}

fn polymarket_recent_volume(entry: &Value) -> f64 {
    polymarket_numeric_field(entry, "volume24hr").unwrap_or(0.0)
}

fn format_polymarket_volume(value: f64) -> String {
    if value >= 1_000_000.0 {
        format!("{:.2}M", value / 1_000_000.0)
    } else if value >= 1_000.0 {
        format!("{:.1}K", value / 1_000.0)
    } else {
        format!("{:.0}", value)
    }
}

fn eligible_polymarket_briefing_market(entry: &Value) -> bool {
    let question = entry
        .get("question")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if question.is_empty() || !question.contains('?') {
        return false;
    }

    let today = format_unix_timestamp_utc(unix_timestamp_secs())
        .get(..10)
        .unwrap_or("")
        .to_string();
    let end_date = entry.get("endDateIso").and_then(Value::as_str).or_else(|| {
        entry
            .get("endDate")
            .and_then(Value::as_str)
            .and_then(|value| value.get(..10))
    });
    if end_date
        .map(|value| value < today.as_str())
        .unwrap_or(false)
    {
        return false;
    }

    let Some((yes_pct, no_pct)) = polymarket_yes_no_percentages(entry) else {
        return false;
    };
    let certainty = yes_pct.max(no_pct);

    if let Some(end_date) = end_date {
        if let Some(days_until) = approximate_days_until(end_date) {
            // Briefings need markets with a realistic near-term news hook.
            if days_until > 75 {
                return false;
            }
            if certainty >= 97 && days_until > 21 {
                return false;
            }
            if certainty >= 95 && days_until > 45 {
                return false;
            }
        }
    }

    true
}

fn build_polymarket_ranked_snapshot_context(snapshot_content: &str) -> Option<String> {
    let entries = serde_json::from_str::<Value>(snapshot_content).ok()?;
    let markets = entries.as_array()?;

    let mut ranked = markets
        .iter()
        .filter(|entry| eligible_polymarket_briefing_market(entry))
        .collect::<Vec<_>>();
    if ranked.is_empty() {
        return None;
    }

    ranked.sort_by(|left, right| {
        polymarket_market_candidate_score(right)
            .cmp(&polymarket_market_candidate_score(left))
            .then_with(|| {
                polymarket_recent_volume(right)
                    .partial_cmp(&polymarket_recent_volume(left))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                polymarket_primary_volume(right)
                    .partial_cmp(&polymarket_primary_volume(left))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    let mut seen_topics = HashSet::new();
    ranked.retain(|entry| seen_topics.insert(polymarket_market_topic_key(entry)));
    if ranked.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    lines.push(
        "## Ranked Polymarket Snapshot\nUse the highest-signal active markets from the prefetched Gamma API snapshot below. Prefer rows near the top because they combine meaningful trading activity with a stronger chance of finding recent grounded news."
            .to_string(),
    );
    lines.push(String::new());
    lines.push("| Rank | Question | 24h Volume | Total Volume | End Date | Odds |".to_string());
    lines.push("| --- | --- | ---: | ---: | --- | --- |".to_string());

    for (index, entry) in ranked.iter().take(8).enumerate() {
        let question = entry
            .get("question")
            .and_then(Value::as_str)
            .unwrap_or("")
            .replace('|', "\\|");
        let volume_24h =
            format_polymarket_volume(polymarket_numeric_field(entry, "volume24hr").unwrap_or(0.0));
        let total_volume =
            format_polymarket_volume(polymarket_numeric_field(entry, "volumeNum").unwrap_or(0.0));
        let end_date = entry
            .get("endDateIso")
            .and_then(Value::as_str)
            .or_else(|| entry.get("endDate").and_then(Value::as_str))
            .unwrap_or("")
            .chars()
            .take(10)
            .collect::<String>();
        let odds = polymarket_yes_no_percentages(entry)
            .map(|(yes_pct, no_pct)| format!("Yes {}% / No {}%", yes_pct, no_pct))
            .unwrap_or_else(|| "Unavailable".to_string());
        lines.push(format!(
            "| {} | {} | {} | {} | {} | {} |",
            index + 1,
            question,
            volume_24h,
            total_volume,
            end_date,
            odds
        ));
    }

    Some(lines.join("\n"))
}

fn top_polymarket_briefing_entries(snapshot_content: &str, limit: usize) -> Vec<Value> {
    let Ok(entries) = serde_json::from_str::<Value>(snapshot_content) else {
        return Vec::new();
    };
    let Some(markets) = entries.as_array() else {
        return Vec::new();
    };

    let mut ranked = markets
        .iter()
        .filter(|entry| eligible_polymarket_briefing_market(entry))
        .cloned()
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        polymarket_market_candidate_score(right)
            .cmp(&polymarket_market_candidate_score(left))
            .then_with(|| {
                polymarket_recent_volume(right)
                    .partial_cmp(&polymarket_recent_volume(left))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                polymarket_primary_volume(right)
                    .partial_cmp(&polymarket_primary_volume(left))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    let mut seen_topics = HashSet::new();
    ranked.retain(|entry| seen_topics.insert(polymarket_market_topic_key(entry)));
    ranked.truncate(limit);
    ranked
}

fn basic_polymarket_briefing_candidates(snapshot_content: &str, limit: usize) -> Vec<Value> {
    let Ok(entries) = serde_json::from_str::<Value>(snapshot_content) else {
        return Vec::new();
    };
    let Some(markets) = entries.as_array() else {
        return Vec::new();
    };

    let mut ranked = markets
        .iter()
        .filter(|entry| {
            entry
                .get("active")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .filter(|entry| {
            entry
                .get("question")
                .and_then(Value::as_str)
                .map(|value| !value.trim().is_empty() && value.contains('?'))
                .unwrap_or(false)
        })
        .filter(|entry| polymarket_yes_no_percentages(entry).is_some())
        .filter(|entry| {
            entry
                .get("endDateIso")
                .and_then(Value::as_str)
                .or_else(|| {
                    entry
                        .get("endDate")
                        .and_then(Value::as_str)
                        .and_then(|value| value.get(..10))
                })
                .and_then(approximate_days_until)
                .map(|days_until| (0..=120).contains(&days_until))
                .unwrap_or(true)
        })
        .cloned()
        .collect::<Vec<_>>();

    ranked.sort_by(|left, right| {
        polymarket_market_candidate_score(right)
            .cmp(&polymarket_market_candidate_score(left))
            .then_with(|| {
                polymarket_primary_volume(right)
                    .partial_cmp(&polymarket_primary_volume(left))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                polymarket_recent_volume(right)
                    .partial_cmp(&polymarket_recent_volume(left))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    let mut seen_topics = HashSet::new();
    ranked.retain(|entry| seen_topics.insert(polymarket_market_topic_key(entry)));
    ranked.truncate(limit);
    ranked
}

fn recent_news_summary_is_strong(summary: &str) -> bool {
    let normalized = clean_news_text_component(summary);
    if normalized.split_whitespace().count() < 10 {
        return false;
    }

    let lower = normalized.to_ascii_lowercase();
    if lower.contains("wikipedia")
        || lower.contains("source: wikipedia")
        || lower.contains("possible knockout stage")
        || lower.contains("first match(es) will be played")
    {
        return false;
    }

    text_has_specific_calendar_date(&normalized)
        || lower.contains("today")
        || lower.contains("yesterday")
        || lower.contains("hours ago")
        || lower.contains("breaking")
}

fn strip_wrapping_markdown_fence(text: &str) -> String {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_string();
    }

    let mut lines = trimmed.lines();
    let Some(first) = lines.next() else {
        return trimmed.to_string();
    };
    if !first.trim_start().starts_with("```") {
        return trimmed.to_string();
    }
    let inner = lines.collect::<Vec<_>>();
    if let Some(last) = inner.last() {
        if last.trim() == "```" {
            return inner[..inner.len().saturating_sub(1)]
                .join("\n")
                .trim()
                .to_string();
        }
    }
    trimmed.to_string()
}

fn preferred_news_hosts() -> &'static [&'static str] {
    &[
        "reuters.com",
        "apnews.com",
        "bloomberg.com",
        "wsj.com",
        "ft.com",
        "cnbc.com",
        "cnn.com",
        "bbc.com",
        "abcnews.go.com",
        "nbcnews.com",
        "cbsnews.com",
        "pbs.org",
        "nytimes.com",
        "latimes.com",
        "theguardian.com",
        "politico.com",
        "aljazeera.com",
    ]
}

fn host_is_preferred_news_source(host: &str) -> bool {
    preferred_news_hosts()
        .iter()
        .any(|preferred| host.ends_with(preferred))
}

fn preferred_news_source_label(label: &str) -> bool {
    let normalized = label.trim().to_ascii_lowercase();
    [
        "reuters",
        "associated press",
        "ap news",
        "bloomberg",
        "the wall street journal",
        "wall street journal",
        "financial times",
        "cnbc",
        "cnn",
        "bbc",
        "abc news",
        "nbc news",
        "cbs news",
        "pbs",
        "new york times",
        "los angeles times",
        "the guardian",
        "politico",
        "al jazeera",
    ]
    .iter()
    .any(|preferred| normalized.contains(preferred))
}

fn preferred_news_host_display_name(host: &str) -> Option<&'static str> {
    let normalized = host.trim().to_ascii_lowercase();
    [
        ("reuters.com", "Reuters"),
        ("apnews.com", "AP News"),
        ("bloomberg.com", "Bloomberg"),
        ("wsj.com", "The Wall Street Journal"),
        ("ft.com", "Financial Times"),
        ("cnbc.com", "CNBC"),
        ("cnn.com", "CNN"),
        ("bbc.com", "BBC"),
        ("abcnews.go.com", "ABC News"),
        ("nbcnews.com", "NBC News"),
        ("cbsnews.com", "CBS News"),
        ("pbs.org", "PBS"),
        ("nytimes.com", "New York Times"),
        ("latimes.com", "Los Angeles Times"),
        ("theguardian.com", "The Guardian"),
        ("politico.com", "Politico"),
        ("aljazeera.com", "Al Jazeera"),
    ]
    .iter()
    .find_map(|(suffix, display_name)| normalized.ends_with(suffix).then_some(*display_name))
}

fn blocked_news_source_label(label: &str) -> bool {
    let normalized = label.trim().to_ascii_lowercase();
    [
        "facebook.com",
        "facebook",
        "instagram.com",
        "instagram",
        "x.com",
        "twitter",
        "twitter.com",
        "youtube.com",
        "youtube",
        "tiktok.com",
        "tiktok",
        "reddit.com",
        "polysimulator",
        "prediction market",
        "odds checker",
        "new york post",
        "oilprice.com",
        "gambling 911",
        "tradingview",
        "startuphub.ai",
        "investing.com",
        "track all markets",
        "trade ideas",
        "predscope.com",
        "predictioncircle.com",
        "mantapex",
        "zerohedge",
        "theburningplatform.com",
        "the burning platform",
    ]
    .iter()
    .any(|blocked| normalized.contains(blocked))
}

fn extract_google_news_source_label(title: &str) -> Option<String> {
    let cleaned = clean_news_text_component(title);
    let (_, tail) = cleaned.rsplit_once(" - ")?;
    let label = tail.trim();
    if label.is_empty() {
        None
    } else {
        Some(label.to_string())
    }
}

fn polymarket_market_topic_key(entry: &Value) -> String {
    let description = entry
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let question = entry
        .get("question")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let end_date = entry
        .get("endDateIso")
        .and_then(Value::as_str)
        .or_else(|| entry.get("endDate").and_then(Value::as_str))
        .unwrap_or("");

    if !description.is_empty() {
        format!("{}::{}", description, end_date)
    } else {
        format!("{}::{}", question, end_date)
    }
}

fn clean_news_text_component(text: &str) -> String {
    let cleaned = text
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#8217;", "'")
        .replace("&#8216;", "'")
        .replace('‘', "'")
        .replace('’', "'")
        .replace('“', "\"")
        .replace('”', "\"")
        .replace('–', "-")
        .replace('—', "-")
        .replace("&amp;", "&")
        .replace("&nbsp;", " ")
        .replace('\n', " ");
    let without_urls = regex::Regex::new(r"https?://\S+")
        .ok()
        .map(|re| re.replace_all(&cleaned, "").into_owned())
        .unwrap_or(cleaned);
    without_urls
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn specific_calendar_date_from_url(url: &str) -> Option<String> {
    regex::Regex::new(r"\b(20\d{2}-\d{2}-\d{2})\b")
        .ok()?
        .captures(url)
        .and_then(|caps| caps.get(1).map(|value| value.as_str().to_string()))
}

fn prediction_market_is_single_fixture_sports_market(question: &str) -> bool {
    let lower = question.trim().to_ascii_lowercase();
    if lower.is_empty()
        || !regex::Regex::new(r"\b20\d{2}-\d{2}-\d{2}\b")
            .ok()
            .map(|re| re.is_match(&lower))
            .unwrap_or(false)
    {
        return false;
    }

    [
        " win on ",
        " fc ",
        "football",
        "soccer",
        "premier league",
        "champions league",
        "la liga",
        "serie a",
        "bundesliga",
        "mlb",
        "nba",
        "nfl",
        "nhl",
        "matchday",
        "playoffs",
    ]
    .iter()
    .any(|signal| lower.contains(signal))
}

fn polymarket_market_candidate_score(entry: &Value) -> i64 {
    let question = entry
        .get("question")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if question.is_empty() {
        return i64::MIN;
    }

    let today = format_unix_timestamp_utc(unix_timestamp_secs())
        .get(..10)
        .unwrap_or("")
        .to_string();
    let end_date = entry.get("endDateIso").and_then(Value::as_str).or_else(|| {
        entry
            .get("endDate")
            .and_then(Value::as_str)
            .and_then(|value| value.get(..10))
    });
    if end_date
        .map(|value| value < today.as_str())
        .unwrap_or(false)
    {
        return i64::MIN;
    }

    let recent_volume = polymarket_recent_volume(entry);
    let total_volume = polymarket_primary_volume(entry);
    let mut score =
        ((recent_volume / 25_000.0).round() as i64) + ((total_volume / 250_000.0).round() as i64);

    if question.contains('?') {
        score += 10;
    }
    if let Some((yes_pct, no_pct)) = polymarket_yes_no_percentages(entry) {
        let certainty = yes_pct.max(no_pct);
        if certainty >= 99 {
            score -= 260;
        } else if certainty >= 98 {
            score -= 180;
        } else if certainty >= 97 {
            score -= 120;
        } else if certainty >= 85 {
            score -= 40;
        } else if (35..=65).contains(&yes_pct) {
            score += 30;
        }
    } else {
        score -= 60;
    }
    if let Some(end_date) = end_date {
        if let Some(days_until) = approximate_days_until(end_date) {
            if days_until <= 10 {
                score += 60;
            } else if days_until <= 21 {
                score += 35;
            } else if days_until <= 35 {
                score += 25;
            } else if days_until <= 60 {
                score += 10;
            } else if days_until > 75 {
                score -= 400;
            } else if days_until > 365 {
                score -= 500;
            } else if days_until > 180 {
                score -= 250;
            } else if days_until > 90 {
                score -= 120;
            } else if days_until > 60 {
                score -= 60;
            }
        }
    }
    if let Some(year) = end_date
        .and_then(|value| value.get(..4))
        .and_then(|value| value.parse::<i32>().ok())
    {
        if let Some(current_year) = current_utc_year().map(|value| value as i32) {
            if year == current_year {
                score += 25;
            } else if year > current_year + 1 {
                score -= 250;
            } else if year > current_year {
                score -= 120;
            }
        }
    }
    if prediction_market_is_single_fixture_sports_market(question) {
        score -= 220;
    }
    if question.contains(" win the ")
        && question.to_ascii_lowercase().contains("world cup")
        && end_date
            .and_then(approximate_days_until)
            .map(|days_until| days_until > 45)
            .unwrap_or(false)
    {
        score -= 180;
    }
    let question_lower = question.to_ascii_lowercase();
    if question_lower.contains("champions league")
        || question_lower.contains("premier league")
        || question_lower.contains("la liga")
        || question_lower.contains("serie a")
        || question_lower.contains("nfl")
        || question_lower.contains("mlb")
        || question_lower.contains("stanley cup")
    {
        score -= 140;
    }
    if question.contains(" win the ")
        && question.to_ascii_lowercase().contains("nba finals")
        && end_date
            .and_then(approximate_days_until)
            .map(|days_until| days_until > 45)
            .unwrap_or(false)
    {
        score -= 120;
    }

    score
}

fn prediction_market_anchor_tokens(question: &str, description: &str) -> Vec<String> {
    let generic = [
        "will",
        "after",
        "before",
        "today",
        "right",
        "now",
        "market",
        "markets",
        "question",
        "current",
        "odds",
        "recent",
        "latest",
        "news",
        "this",
        "that",
        "active",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
        "high",
        "low",
        "hit",
        "reach",
        "returns",
        "return",
        "normal",
        "regime",
        "fall",
        "ends",
        "end",
        "deal",
        "peace",
        "conflict",
        "price",
        "prices",
        "2026",
        "2027",
    ];
    let mut tokens = tokenize_grounded_keywords(&format!(
        "{} {}",
        question,
        description.lines().take(2).collect::<Vec<_>>().join(" ")
    ))
    .into_iter()
    .filter(|token| token.len() >= 4)
    .filter(|token| !generic.iter().any(|blocked| blocked == token))
    .collect::<Vec<_>>();
    tokens.sort();
    tokens.dedup();
    tokens
}

fn prediction_market_token_variants(token: &str) -> Vec<String> {
    let normalized = token.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Vec::new();
    }

    let mut variants = vec![normalized.clone()];
    if normalized.ends_with("ian") && normalized.len() > 6 {
        let root = normalized[..normalized.len() - 4].to_string();
        if root.len() >= 4 {
            variants.push(root);
        }
    }

    variants.sort();
    variants.dedup();
    variants
}

fn prediction_market_text_matches_token(text: &str, token: &str) -> bool {
    prediction_market_token_variants(token)
        .into_iter()
        .any(|variant| text.contains(&variant))
}

fn prediction_market_match_count<'a>(
    text: &str,
    tokens: impl IntoIterator<Item = &'a str>,
) -> usize {
    tokens
        .into_iter()
        .filter(|token| prediction_market_text_matches_token(text, token))
        .count()
}

fn score_recent_news_result(question: &str, description: &str, result: &Value) -> i64 {
    let title =
        clean_news_text_component(result.get("title").and_then(Value::as_str).unwrap_or(""));
    let snippet =
        clean_news_text_component(result.get("snippet").and_then(Value::as_str).unwrap_or(""));
    let url = result.get("url").and_then(Value::as_str).unwrap_or("");
    let host = normalize_url_host(url).unwrap_or_default();
    let source_label = extract_google_news_source_label(&title).unwrap_or_default();
    let url_date = specific_calendar_date_from_url(url);
    if host.is_empty() && source_label.is_empty() {
        return i64::MIN / 4;
    }

    let blocked_hosts = [
        "polymarket.com",
        "gamma-api.polymarket.com",
        "frenflow.com",
        "pulptastic.com",
        "nypost.com",
        "oilprice.com",
        "gambling911.com",
        "manifold.markets",
        "kalshi.com",
        "lines.com",
        "thetradefox.com",
        "wangr.com",
        "predictioncircle.com",
        "predictionpulse.io",
        "betmoar.fun",
        "slipy.ai",
    ];
    if blocked_hosts.iter().any(|blocked| host == *blocked)
        || blocked_news_source_label(&source_label)
    {
        return -200;
    }

    let combined = format!("{} {}", title, snippet).to_ascii_lowercase();
    if combined.contains("no more results found") {
        return i64::MIN / 8;
    }
    if [
        "this market will resolve",
        "24h volume",
        "7d volume",
        "best bid",
        "best ask",
        "liquidity",
        "page shortcut menu",
        "page legend",
        "potential contenders",
        "odds tracker",
        "track all markets",
        "trade ideas",
        "historical data",
        "prices today",
        "prediction market tracker",
        "technical analysis",
        "news analysis:",
        "live updates:",
        "opinion |",
        "opinion:",
        "match center",
        "matchcentre",
        "box score",
        "match sheet",
        "full coverage of",
        "live match coverage",
    ]
    .iter()
    .any(|needle| combined.contains(needle))
    {
        return -220;
    }
    let anchor_tokens = prediction_market_anchor_tokens(question, description);
    let anchor_overlap =
        prediction_market_match_count(&combined, anchor_tokens.iter().map(|token| token.as_str()))
            as i64;
    if !anchor_tokens.is_empty() && anchor_overlap == 0 {
        return -180;
    }
    let question_tokens = tokenize_grounded_keywords(question);
    let overlap = prediction_market_match_count(
        &combined,
        question_tokens.iter().map(|token| token.as_str()),
    ) as i64;
    let mut score = overlap * 8 + anchor_overlap * 14;

    if host_is_preferred_news_source(&host) || preferred_news_source_label(&source_label) {
        score += 40;
    }
    if [
        "schedule",
        "squad",
        "draw",
        "analysis",
        "preview",
        "odds",
        "fixtures",
        "#predictionmarkets",
        "#shorts",
        "youtube",
        "viral",
        "live updates",
        "analysis:",
    ]
    .iter()
    .any(|needle| combined.contains(needle))
    {
        score -= 35;
    }
    if combined.contains("opinion |")
        || combined.starts_with("opinion ")
        || combined.contains("news analysis:")
        || combined.contains("live updates:")
    {
        score -= 120;
    }
    if let Some(current_year) = current_utc_year().map(|value| value.to_string()) {
        let current_year_num = current_year.parse::<i32>().ok().unwrap_or_default();
        let url = result.get("url").and_then(Value::as_str).unwrap_or("");
        for stale_year in 2000..current_year_num {
            let stale_year = stale_year.to_string();
            if title.contains(&stale_year)
                || snippet.contains(&stale_year)
                || url.contains(&format!("/{}/", stale_year))
            {
                score -= if stale_year == (current_year_num - 1).to_string() {
                    60
                } else {
                    120
                };
                break;
            }
        }
    }
    let has_specific_date = text_has_specific_calendar_date(&title)
        || text_has_specific_calendar_date(&snippet)
        || url_date.is_some();
    if has_specific_date {
        score += 10;
    }
    if let Some(url_date) = url_date.as_deref() {
        if !text_contains_recent_news_date(url_date, 2) {
            return -260;
        }
    }
    let date_text = format!(
        "{}\n{}\n{}",
        title,
        snippet,
        url_date.clone().unwrap_or_default()
    );
    if has_specific_date && !text_contains_recent_news_date(&date_text, 2) {
        score -= 140;
    }
    if combined.contains("today")
        || combined.contains("yesterday")
        || combined.contains("hours ago")
        || combined.contains("breaking")
    {
        score += 8;
    }
    if combined.contains("prediction market") || combined.contains("polymarket") {
        score -= 20;
    }
    if anchor_overlap < 1 && !anchor_tokens.is_empty() {
        score -= 120;
    }
    if overlap < 2
        && !host_is_preferred_news_source(&host)
        && !preferred_news_source_label(&source_label)
    {
        score -= 50;
    }
    if snippet.split_whitespace().count() >= 12 {
        score += 6;
    }

    score
}

fn recent_news_selection_score(result: &Value, base_score: i64) -> i64 {
    let url = result.get("url").and_then(Value::as_str).unwrap_or("");
    let host = normalize_url_host(url).unwrap_or_default();
    let title = result.get("title").and_then(Value::as_str).unwrap_or("");
    let source = result.get("source").and_then(Value::as_str).unwrap_or("");
    let source_label = if source.trim().is_empty() {
        extract_google_news_source_label(title).unwrap_or_default()
    } else {
        source.trim().to_string()
    };

    let mut score = base_score;
    if host == "news.google.com" {
        score -= 30;
    } else if host_is_preferred_news_source(&host) {
        score += 18;
    }
    if preferred_news_source_label(&source_label) {
        score += 10;
    }
    score
}

fn strip_source_suffix(text: &str, source_name: &str) -> String {
    let trimmed = text.trim();
    let source = source_name.trim();
    if trimmed.is_empty() || source.is_empty() {
        return trimmed.to_string();
    }

    let patterns = [
        format!(" - {}", source),
        format!(" | {}", source),
        format!(" — {}", source),
        format!(" – {}", source),
    ];
    for pattern in patterns {
        if let Some(stripped) = trimmed.strip_suffix(&pattern) {
            return stripped
                .trim_end_matches(['.', ' ', '-', '|', '—', '–'])
                .to_string();
        }
    }
    trimmed.to_string()
}

fn prediction_market_odds_context_sentence(yes_pct: u32, no_pct: u32) -> &'static str {
    if yes_pct >= 70 {
        "That supports the market's strong Yes bias."
    } else if no_pct >= 70 {
        "That supports the market's strong No bias."
    } else if yes_pct > no_pct {
        "That helps explain why traders currently lean Yes."
    } else if no_pct > yes_pct {
        "That helps explain why traders currently lean No."
    } else {
        "That helps explain why traders are pricing the market close to even."
    }
}

fn summarize_recent_news_result(
    question: &str,
    description: &str,
    result: &Value,
) -> Option<String> {
    let score = score_recent_news_result(question, description, result);
    if score < 24 {
        return None;
    }

    let title =
        clean_news_text_component(result.get("title").and_then(Value::as_str).unwrap_or(""));
    let snippet =
        clean_news_text_component(result.get("snippet").and_then(Value::as_str).unwrap_or(""));
    let host = normalize_url_host(result.get("url").and_then(Value::as_str).unwrap_or(""))
        .unwrap_or_default();
    let url_date =
        specific_calendar_date_from_url(result.get("url").and_then(Value::as_str).unwrap_or(""));
    let source_label = extract_google_news_source_label(&title).unwrap_or_default();
    let combined = format!("{} {}", title, snippet).to_ascii_lowercase();
    let anchor_tokens = prediction_market_anchor_tokens(question, description);
    let anchor_overlap =
        prediction_market_match_count(&combined, anchor_tokens.iter().map(|token| token.as_str()));
    let has_recency_signal = text_has_specific_calendar_date(&title)
        || text_has_specific_calendar_date(&snippet)
        || url_date.is_some()
        || combined.contains("today")
        || combined.contains("yesterday")
        || combined.contains("hours ago")
        || combined.contains("breaking");
    let date_text = format!(
        "{}\n{}\n{}",
        title,
        snippet,
        url_date.clone().unwrap_or_default()
    );
    if combined.contains("opinion |")
        || combined.starts_with("opinion ")
        || combined.contains("news analysis:")
        || combined.contains("live updates:")
    {
        return None;
    }
    if let Some(url_date) = url_date.as_deref() {
        if !text_contains_recent_news_date(url_date, 2) {
            return None;
        }
    }
    if has_recency_signal && !text_contains_recent_news_date(&date_text, 2) {
        return None;
    }
    if !host_is_preferred_news_source(&host)
        && !preferred_news_source_label(&source_label)
        && !has_recency_signal
    {
        return None;
    }
    if anchor_overlap == 0 {
        return None;
    }
    let source_name = result
        .get("source")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            if !source_label.is_empty() {
                Some(source_label.clone())
            } else if let Some(display_name) = preferred_news_host_display_name(&host) {
                Some(display_name.to_string())
            } else if !host.is_empty() {
                Some(host.clone())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "Recent coverage".to_string());
    let published_date = result
        .get("published")
        .and_then(Value::as_str)
        .and_then(|value| extract_specific_calendar_dates(value).into_iter().max())
        .map(|(year, month, day)| format_specific_calendar_date(year, month, day))
        .or_else(|| {
            extract_specific_calendar_dates(&format!("{}\n{}", title, snippet))
                .into_iter()
                .max()
                .map(|(year, month, day)| format_specific_calendar_date(year, month, day))
        })
        .or(url_date.clone())
        .unwrap_or_else(|| "recently".to_string());
    let article_url = result
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("");
    let cleaned_snippet =
        regex::Regex::new(r"(?i)^(?:[A-Za-z]{3,9}\s+\d{1,2}(?:,)?\s+\d{4}\s+reported\s+)")
            .ok()
            .map(|re| re.replace(&snippet, "").into_owned())
            .unwrap_or_else(|| snippet.clone());
    let lead = if !cleaned_snippet.is_empty() {
        cleaned_snippet.trim_end_matches('.').to_string()
    } else {
        strip_source_suffix(title.trim_end_matches('.'), &source_name)
    };
    let lead = strip_source_suffix(&lead, &source_name);
    let source_markup = if article_url.is_empty() {
        source_name.clone()
    } else {
        format!("[{}]({})", source_name, article_url)
    };
    let summary = format!(
        "{} reported on {} that {}. {}",
        source_name, published_date, lead, source_markup,
    );
    recent_news_summary_is_strong(&summary).then_some(summary)
}

fn format_prediction_market_related_news(yes_pct: u32, no_pct: u32, news_summary: &str) -> String {
    format!(
        "{} {}",
        news_summary.trim(),
        prediction_market_odds_context_sentence(yes_pct, no_pct),
    )
}

fn select_best_recent_news_summary(
    question: &str,
    description: &str,
    results: &[Value],
) -> Option<(i64, String)> {
    results
        .iter()
        .filter_map(|result| {
            let base_score = score_recent_news_result(question, description, result);
            summarize_recent_news_result(question, description, result)
                .map(|summary| (recent_news_selection_score(result, base_score), summary))
        })
        .max_by_key(|(score, _)| *score)
}

fn decode_basic_html_entities(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

async fn fetch_recent_news_rss_results(query: &str) -> Option<Vec<Value>> {
    let mut url = reqwest::Url::parse("https://news.google.com/rss/search").ok()?;
    url.query_pairs_mut()
        .append_pair("q", &format!("{} when:2d", query))
        .append_pair("hl", "en-US")
        .append_pair("gl", "US")
        .append_pair("ceid", "US:en");
    let response = crate::generic::infra::http_client::default_client()
        .get(url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body = response.text().await.ok()?;
    let item_re = regex::Regex::new(
        r#"(?is)<item>.*?<title>(?P<title>.*?)</title>.*?<link>(?P<link>.*?)</link>.*?<pubDate>(?P<pub>.*?)</pubDate>.*?(?:<source\s+url="(?P<source_url>[^"]*)">(?P<source>.*?)</source>)?.*?</item>"#,
    )
    .ok()?;
    let date_re = regex::Regex::new(r"(?i)(\d{1,2})\s+([A-Za-z]{3})\s+(\d{4})").ok()?;
    let mut results = Vec::new();
    for captures in item_re.captures_iter(&body).take(8) {
        let title = decode_basic_html_entities(
            captures
                .name("title")
                .map(|value| value.as_str())
                .unwrap_or(""),
        );
        let link = decode_basic_html_entities(
            captures
                .name("link")
                .map(|value| value.as_str())
                .unwrap_or(""),
        );
        let pub_date = captures
            .name("pub")
            .map(|value| value.as_str())
            .unwrap_or("")
            .trim();
        let source = decode_basic_html_entities(
            captures
                .name("source")
                .map(|value| value.as_str())
                .unwrap_or(""),
        );
        let source_url = decode_basic_html_entities(
            captures
                .name("source_url")
                .map(|value| value.as_str())
                .unwrap_or(""),
        );
        let specific_date = date_re
            .captures(pub_date)
            .and_then(|caps| {
                Some(format!(
                    "{} {} {}",
                    caps.get(2)?.as_str(),
                    caps.get(1)?.as_str(),
                    caps.get(3)?.as_str()
                ))
            })
            .unwrap_or_else(|| pub_date.to_string());
        let snippet = if specific_date.is_empty() {
            format!("Recent coverage about {}.", title)
        } else {
            format!("{} reported {}.", specific_date, title)
        };
        results.push(json!({
            "title": title,
            "snippet": snippet,
            "url": link,
            "source": source,
            "source_url": source_url,
            "published": specific_date,
        }));
    }
    (!results.is_empty()).then_some(results)
}

fn prediction_market_news_queries(question: &str, description: &str) -> Vec<String> {
    let base = question.trim().trim_end_matches('?');
    let blocked_tokens = [
        "will", "the", "next", "after", "before", "today", "right", "now", "meeting", "market",
    ];
    let mut queries = Vec::new();
    queries.push(format!(
        "\"{}\" recent news -site:polymarket.com -site:frenflow.com -site:pulptastic.com",
        base
    ));

    let topical_tokens = tokenize_grounded_keywords(question)
        .into_iter()
        .flat_map(|token| prediction_market_token_variants(&token))
        .filter(|token| !blocked_tokens.iter().any(|blocked| blocked == token))
        .collect::<Vec<_>>();
    let mut topical_tokens = topical_tokens;
    topical_tokens.sort();
    topical_tokens.dedup();
    topical_tokens.truncate(7);
    if !topical_tokens.is_empty() {
        queries.push(format!(
            "{} recent news -site:polymarket.com -site:frenflow.com -site:pulptastic.com",
            topical_tokens.join(" ")
        ));
    }
    let description_tokens = tokenize_grounded_keywords(description)
        .into_iter()
        .flat_map(|token| prediction_market_token_variants(&token))
        .filter(|token| !blocked_tokens.iter().any(|blocked| blocked == token))
        .collect::<Vec<_>>();
    let mut description_tokens = description_tokens;
    description_tokens.sort();
    description_tokens.dedup();
    description_tokens.truncate(5);
    if !topical_tokens.is_empty() && !description_tokens.is_empty() {
        let mut combined_tokens = topical_tokens.clone();
        combined_tokens.extend(description_tokens);
        combined_tokens.sort();
        combined_tokens.dedup();
        queries.push(format!(
            "{} latest news -site:polymarket.com -site:frenflow.com -site:pulptastic.com",
            combined_tokens.join(" ")
        ));
    }

    if let Some(country) = regex::Regex::new(r"(?i)^Will (.+) win the 2026 FIFA World Cup\??$")
        .ok()
        .and_then(|re| re.captures(base))
        .and_then(|caps| caps.get(1).map(|value| value.as_str().trim().to_string()))
    {
        queries.push(format!(
            "{} FIFA World Cup 2026 qualifiers recent news -site:polymarket.com -site:frenflow.com -site:pulptastic.com",
            country
        ));
    }

    if let Some(team) = regex::Regex::new(r"(?i)^Will the (.+) win the 2026 NBA Finals\??$")
        .ok()
        .and_then(|re| re.captures(base))
        .and_then(|caps| caps.get(1).map(|value| value.as_str().trim().to_string()))
    {
        queries.push(format!(
            "{} NBA playoffs recent news -site:polymarket.com -site:frenflow.com -site:pulptastic.com",
            team
        ));
    }

    if let Some(team) = regex::Regex::new(r"(?i)^Will (.+) win the 2025[–-]26 La Liga\??$")
        .ok()
        .and_then(|re| re.captures(base))
        .and_then(|caps| caps.get(1).map(|value| value.as_str().trim().to_string()))
    {
        queries.push(format!(
            "{} La Liga recent news -site:polymarket.com -site:frenflow.com -site:pulptastic.com",
            team
        ));
    }

    if let Some(team) = regex::Regex::new(r"(?i)^Will (.+) win the 2025[–-]26 Champions League\??$")
        .ok()
        .and_then(|re| re.captures(base))
        .and_then(|caps| caps.get(1).map(|value| value.as_str().trim().to_string()))
    {
        queries.push(format!(
            "{} Champions League recent news -site:polymarket.com -site:frenflow.com -site:pulptastic.com",
            team
        ));
    }

    queries.dedup();
    queries
}

fn prediction_market_direct_news_queries(question: &str, description: &str) -> Vec<String> {
    let mut queries = Vec::new();
    for query in prediction_market_news_queries(question, description) {
        queries.push(query.clone());
        queries.push(format!("site:reuters.com {}", query));
        queries.push(format!("site:apnews.com {}", query));
        queries.push(format!("site:nytimes.com {}", query));
    }
    queries.dedup();
    queries
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = month as i32;
    let day = day as i32;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146097 + doe - 719468) as i64
}

fn current_utc_ymd() -> Option<(i32, u32, u32)> {
    let stamp = format_unix_timestamp_utc(unix_timestamp_secs());
    let year = stamp.get(0..4)?.parse::<i32>().ok()?;
    let month = stamp.get(5..7)?.parse::<u32>().ok()?;
    let day = stamp.get(8..10)?.parse::<u32>().ok()?;
    Some((year, month, day))
}

fn extract_specific_calendar_dates(text: &str) -> Vec<(i32, u32, u32)> {
    let mut dates = Vec::new();

    if let Ok(iso_re) = regex::Regex::new(r"\b(20\d{2})-(\d{2})-(\d{2})\b") {
        for caps in iso_re.captures_iter(text) {
            let Some(year) = caps.get(1).and_then(|v| v.as_str().parse::<i32>().ok()) else {
                continue;
            };
            let Some(month) = caps.get(2).and_then(|v| v.as_str().parse::<u32>().ok()) else {
                continue;
            };
            let Some(day) = caps.get(3).and_then(|v| v.as_str().parse::<u32>().ok()) else {
                continue;
            };
            dates.push((year, month, day));
        }
    }

    let Ok(named_re) = regex::Regex::new(
        r"(?i)\b(jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|jun(?:e)?|jul(?:y)?|aug(?:ust)?|sep(?:t|tember)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)\.?\s+(\d{1,2})(?:,)?\s+(20\d{2})\b",
    ) else {
        return dates;
    };

    for caps in named_re.captures_iter(text) {
        let Some(month) = caps
            .get(1)
            .and_then(|v| month_token_alias(v.as_str()))
            .and_then(|alias| match alias {
                "jan" => Some(1),
                "feb" => Some(2),
                "mar" => Some(3),
                "apr" => Some(4),
                "may" => Some(5),
                "jun" => Some(6),
                "jul" => Some(7),
                "aug" => Some(8),
                "sep" => Some(9),
                "oct" => Some(10),
                "nov" => Some(11),
                "dec" => Some(12),
                _ => None,
            })
        else {
            continue;
        };
        let Some(day) = caps.get(2).and_then(|v| v.as_str().parse::<u32>().ok()) else {
            continue;
        };
        let Some(year) = caps.get(3).and_then(|v| v.as_str().parse::<i32>().ok()) else {
            continue;
        };
        dates.push((year, month, day));
    }

    dates.sort_unstable();
    dates.dedup();
    dates
}

fn text_contains_recent_news_date(text: &str, max_age_days: i64) -> bool {
    if text.to_ascii_lowercase().contains("hours ago")
        || text.to_ascii_lowercase().contains("today")
        || text.to_ascii_lowercase().contains("yesterday")
    {
        return true;
    }

    let Some((current_year, current_month, current_day)) = current_utc_ymd() else {
        return false;
    };
    let current_days = days_from_civil(current_year, current_month, current_day);
    extract_specific_calendar_dates(text)
        .into_iter()
        .any(|(year, month, day)| {
            let delta = current_days - days_from_civil(year, month, day);
            (0..=max_age_days).contains(&delta)
        })
}

fn format_specific_calendar_date(year: i32, month: u32, day: u32) -> String {
    format!("{:04}-{:02}-{:02}", year, month, day)
}

fn parse_markdown_level_requirements(markdown: &str) -> Vec<(String, String)> {
    let mut current_level: Option<String> = None;
    let mut requirements = Vec::new();

    for line in markdown.lines() {
        let trimmed = line.trim();
        // Keep Korean worksheet markers because existing grounded task
        // fixtures use them in production daemon flows.
        if let Some(captures) = MARKDOWN_LEVEL_HEADING_RE.captures(trimmed) {
            current_level = captures.get(1).map(|value| value.as_str().to_string());
            continue;
        }

        if let Some(level) = current_level.as_ref() {
            // Korean `**문제:**` labels remain supported for those fixtures.
            if let Some(question) = trimmed.strip_prefix("**문제:**") {
                requirements.push((level.clone(), question.trim().to_string()));
                current_level = None;
            }
        }
    }

    requirements
}

fn resolve_markdown_requirement_csv_path(problem_path: &str, requirement: &str) -> Option<String> {
    if let Some(path) = extract_explicit_file_paths(requirement).into_iter().next() {
        return Some(path);
    }

    let file_name = CSV_FILE_NAME_RE
        .captures(requirement)
        .and_then(|captures| captures.get(1).map(|value| value.as_str().to_string()))?;
    let parent = Path::new(problem_path).parent()?;
    Some(parent.join(file_name).to_string_lossy().to_string())
}

fn summarize_requirement_directive(requirement: &str, csv_path: Option<&str>) -> String {
    let csv_suffix = csv_path
        .map(|path| format!(" using {}", path))
        .unwrap_or_default();

    // These localized phrases map supported Korean worksheet prompts onto the
    // same normalized directive shape used for grounded tabular reasoning.
    if requirement.contains("가장 많이 팔렸") {
        return format!(
            "Count the frequency of Fruit{} and print only the single most common fruit name.",
            csv_suffix
        );
    }
    if requirement.contains("평균") {
        return format!(
            "Compute the arithmetic mean of Hours{} and print only that single average value.",
            csv_suffix
        );
    }
    if requirement.contains("차이") || requirement.contains("Range") {
        return format!(
            "Compute max(Height_cm) - min(Height_cm){} and print only that single range value.",
            csv_suffix
        );
    }
    if requirement.contains("이상치") {
        return format!(
            "Use the IQR rule on Score{} and print only the final outlier count.",
            csv_suffix
        );
    }
    if requirement.contains("회귀 계수")
        || requirement.contains("beta_1")
        || requirement.contains("β1")
    {
        return format!(
            "Estimate the normal-equation OLS coefficients from X1, X2, Y{} and print only beta_1 rounded to three decimals.",
            csv_suffix
        );
    }

    requirement.to_string()
}

fn build_authoritative_problem_requirements_context(paths: &[String]) -> Option<String> {
    let previews = load_prefetched_prompt_file_previews(paths);
    let mut lines = Vec::new();

    for (path, preview, _truncated) in previews {
        for (level, requirement) in parse_markdown_level_requirements(&preview) {
            let csv_path = resolve_markdown_requirement_csv_path(&path, &requirement);
            let directive = summarize_requirement_directive(&requirement, csv_path.as_deref());
            match csv_path {
                Some(path) => lines.push(format!(
                    "- Level {} uses {}. Final directive: {}",
                    level, path, directive
                )),
                None => lines.push(format!("- Level {} final directive: {}", level, directive)),
            }
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(format!(
            "## Authoritative Level Requirements\nUse these exact level tasks from the prefetched problem document. Do not add extra metrics, helper outputs, or invented columns.\n{}\nEach generated script should print exactly one final answer value after '[Level N] Answer:'. All generated scripts must execute successfully on the current target runtime. If pandas or numpy are unavailable, fall back to the Python standard library instead of failing.",
            lines.join("\n")
        ))
    }
}

fn parse_tool_stdout_json(result: &Value) -> Option<Value> {
    let stdout = result
        .get("stdout")
        .and_then(|value| value.as_str())?
        .trim();
    serde_json::from_str(stdout).ok()
}

fn collect_grounded_paths_from_value(value: &Value, grounded_paths: &mut HashSet<String>) {
    if let Some(path) = value.get("path").and_then(|item| item.as_str()) {
        grounded_paths.insert(path.to_string());
        if let Some(entries) = value.get("entries").and_then(|item| item.as_array()) {
            for entry in entries {
                if let Some(name) = entry.get("name").and_then(|item| item.as_str()) {
                    grounded_paths.insert(format!("{}/{}", path.trim_end_matches('/'), name));
                }
            }
        }
    }
}

fn collect_grounded_paths(messages: &[LlmMessage]) -> HashSet<String> {
    let mut grounded_paths = HashSet::new();
    for message in messages.iter().filter(|item| item.role == "tool") {
        match message.tool_name.as_str() {
            "read_file" | "list_files" | "extract_document_text" | "inspect_tabular_data" => {
                collect_grounded_paths_from_value(&message.tool_result, &mut grounded_paths);
                if let Some(stdout_json) = parse_tool_stdout_json(&message.tool_result) {
                    collect_grounded_paths_from_value(&stdout_json, &mut grounded_paths);
                }
            }
            _ => {}
        }
    }
    grounded_paths
}

fn script_references_any_known_input(script_text: &str, known_paths: &HashSet<String>) -> bool {
    known_paths.iter().any(|path| {
        script_text.contains(path)
            || std::path::Path::new(path)
                .file_name()
                .and_then(|value| value.to_str())
                .map(|name| script_text.contains(name))
                .unwrap_or(false)
    })
}

fn path_is_within_any_directory(path: &str, directories: &HashSet<String>) -> bool {
    let path_obj = Path::new(path);
    directories
        .iter()
        .any(|dir| path_obj.starts_with(Path::new(dir)))
}

fn prompt_requires_persisted_level_scripts(prompt: &str) -> bool {
    prompt.contains("level-{problem level}-solution.py")
        || (prompt.contains("/result/library-used") && prompt.contains("/result/library-not-used"))
}

fn extract_prompt_level_numbers(prompt: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut levels = extract_explicit_file_paths(prompt)
        .into_iter()
        .filter_map(|path| {
            let file_name = Path::new(&path).file_name()?.to_str()?;
            let captures = LEVEL_INPUT_RE.captures(file_name)?;
            let level = captures.get(1)?.as_str().to_string();
            if seen.insert(level.clone()) {
                Some(level)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    levels.sort_by_key(|value| value.parse::<usize>().unwrap_or(usize::MAX));
    levels
}

fn extract_level_answer_markers(code: &str) -> HashSet<String> {
    LEVEL_ANSWER_RE
        .captures_iter(code)
        .filter_map(|captures| captures.get(1).map(|value| value.as_str().to_string()))
        .collect()
}

fn extract_declared_output_path_from_leading_comment(code: &str) -> Option<String> {
    let first_content_line = code.lines().map(str::trim).find(|line| !line.is_empty())?;
    let comment_body = first_content_line
        .strip_prefix('#')
        .or_else(|| first_content_line.strip_prefix("//"))?
        .trim();
    let candidate = normalize_explicit_path_token(comment_body)?;
    if candidate != comment_body || !path_looks_like_file(&candidate) {
        return None;
    }
    Some(candidate)
}

fn extract_level_number_from_output_path(path: &str) -> Option<String> {
    let file_name = Path::new(path).file_name()?.to_str()?;
    LEVEL_OUTPUT_LEVEL_RE
        .captures(file_name)
        .and_then(|captures| captures.get(1).map(|value| value.as_str().to_string()))
}

fn expected_persisted_level_script_paths(prompt: &str) -> Vec<String> {
    if !prompt_requires_persisted_level_scripts(prompt) {
        return Vec::new();
    }

    let output_dirs = extract_explicit_directory_paths(prompt);
    let levels = extract_prompt_level_numbers(prompt);
    if output_dirs.is_empty() || levels.is_empty() {
        return Vec::new();
    }

    let mut expected_paths = output_dirs
        .into_iter()
        .flat_map(|dir| {
            levels.iter().map(move |level| {
                format!("{}/level-{}-solution.py", dir.trim_end_matches('/'), level)
            })
        })
        .collect::<Vec<_>>();
    expected_paths.sort();
    expected_paths
}

fn parse_csv_headers_from_preview(preview: &str) -> HashSet<String> {
    preview
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| {
            line.split(',')
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string())
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default()
}

fn collect_grounded_csv_headers(messages: &[LlmMessage]) -> HashSet<String> {
    let mut headers = HashSet::new();

    for message in messages.iter().filter(|message| message.role == "tool") {
        match message.tool_name.as_str() {
            "extract_document_text" => {
                let path = message
                    .tool_result
                    .get("path")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");
                if !path.ends_with(".csv") {
                    continue;
                }
                if let Some(preview) = message
                    .tool_result
                    .get("text_preview")
                    .and_then(|value| value.as_str())
                {
                    headers.extend(parse_csv_headers_from_preview(preview));
                }
            }
            "inspect_tabular_data" => {
                if let Some(sheets) = message
                    .tool_result
                    .get("inspection")
                    .and_then(|value| value.get("sheets"))
                    .and_then(|value| value.as_array())
                {
                    for sheet in sheets {
                        if let Some(sheet_headers) =
                            sheet.get("headers").and_then(|value| value.as_array())
                        {
                            headers.extend(
                                sheet_headers
                                    .iter()
                                    .filter_map(|value| value.as_str())
                                    .map(|value| value.to_string()),
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }

    headers
}

fn extract_indexed_column_names(code: &str) -> HashSet<String> {
    let mut columns = HashSet::new();
    let chars = code.char_indices().collect::<Vec<_>>();

    for (idx, ch) in chars.iter().copied() {
        if ch != '[' {
            continue;
        }

        let prev_non_ws = code[..idx]
            .chars()
            .rev()
            .find(|value| !value.is_whitespace());
        if !matches!(prev_non_ws, Some(value) if value.is_ascii_alphanumeric() || value == '_' || value == ')' || value == ']')
        {
            continue;
        }

        let mut depth = 0usize;
        let mut end_idx = None;
        for (candidate_idx, candidate) in code[idx..].char_indices() {
            match candidate {
                '[' => depth += 1,
                ']' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        end_idx = Some(idx + candidate_idx + candidate.len_utf8());
                        break;
                    }
                }
                _ => {}
            }
        }

        let Some(end_idx) = end_idx else {
            continue;
        };
        let segment = &code[idx..end_idx];
        if segment.contains('/') {
            continue;
        }
        columns.extend(
            QUOTED_IDENTIFIER_RE
                .captures_iter(segment)
                .filter_map(|captures| captures.get(1).map(|value| value.as_str().to_string())),
        );
    }

    columns
}

fn collect_successful_saved_output_paths(messages: &[LlmMessage]) -> HashSet<String> {
    messages
        .iter()
        .filter(|message| message.role == "tool" && message.tool_name == "run_generated_code")
        .filter(|message| {
            message
                .tool_result
                .get("result")
                .and_then(|value| value.get("success"))
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
        })
        .filter_map(|message| {
            message
                .tool_result
                .get("saved_output_path")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        })
        .collect()
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct GeneratedCodeGrounding {
    declared_output_path: Option<String>,
    declared_output_level: Option<String>,
}

fn prompt_requires_atomic_level_answer(prompt: &str) -> bool {
    prompt.contains("[Level X] Answer:")
}

fn validate_generated_code_execution_output(
    stdout: &str,
    expected_level: Option<&str>,
    enforce_atomic_answer: bool,
) -> Result<(), String> {
    let answer_lines = LEVEL_ANSWER_LINE_RE
        .captures_iter(stdout)
        .filter_map(|captures| {
            let level = captures.get(1)?.as_str().to_string();
            let answer = captures.get(2)?.as_str().trim().to_string();
            Some((level, answer))
        })
        .collect::<Vec<_>>();

    if let Some(level) = expected_level {
        if answer_lines.len() != 1 {
            return Err(format!(
                "Generated script for level {} must print exactly one '[Level {}] Answer:' line.",
                level, level
            ));
        }
        if answer_lines[0].0 != level {
            return Err(format!(
                "Generated script was saved as level {}, but printed a level {} answer line.",
                level, answer_lines[0].0
            ));
        }
    }

    if enforce_atomic_answer {
        for (level, answer) in answer_lines {
            if answer.is_empty() {
                return Err(format!(
                    "Generated script for level {} printed an empty answer value.",
                    level
                ));
            }
            if answer.split_whitespace().count() > 1 {
                return Err(format!(
                    "Generated script for level {} printed multiple answer tokens ('{}'). Print exactly one final answer value after '[Level {}] Answer:'.",
                    level, answer, level
                ));
            }
            if answer.contains('|') || answer.contains('\n') || answer.contains('\t') {
                return Err(format!(
                    "Generated script for level {} printed a compound answer ('{}'). Print exactly one final answer value after '[Level {}] Answer:'.",
                    level, answer, level
                ));
            }
        }
    }

    Ok(())
}

fn validate_generated_code_grounding(
    prompt: &str,
    grounded_paths: &HashSet<String>,
    grounded_csv_headers: &HashSet<String>,
    code: &str,
    args: &str,
) -> Result<GeneratedCodeGrounding, String> {
    let prompt_paths = extract_explicit_paths(prompt);
    let prompt_files = extract_explicit_file_paths(prompt);
    let prompt_directories = extract_explicit_directory_paths(prompt)
        .into_iter()
        .collect::<HashSet<_>>();
    let prompt_lower = normalize_prompt_intent_text(prompt).to_ascii_lowercase();
    if prompt_files.is_empty() && prompt_directories.is_empty() {
        return Ok(GeneratedCodeGrounding::default());
    }

    let missing_paths = prompt_files
        .iter()
        .filter(|path| {
            // Only require prior inspection for files that actually exist on disk.
            // Files referenced in the prompt that don't exist yet are likely output
            // targets — the agent is creating them, not reading them.
            !grounded_paths.contains(path.as_str()) && Path::new(path).exists()
        })
        .cloned()
        .collect::<Vec<_>>();
    if !missing_paths.is_empty() {
        return Err(format!(
            "Inspect the referenced input files before executing generated code: {}. Use read_file, extract_document_text, or inspect_tabular_data first.",
            missing_paths.join(", ")
        ));
    }

    let combined = if args.trim().is_empty() {
        code.to_string()
    } else {
        format!("{}\n{}", code, args)
    };
    let combined_paths = extract_explicit_file_paths(&combined);
    let known_paths = prompt_paths
        .iter()
        .chain(grounded_paths.iter())
        .cloned()
        .collect::<HashSet<_>>();
    let unexpected_paths = combined_paths
        .into_iter()
        .filter(|path| {
            !known_paths.contains(path)
                && !path_is_within_any_directory(path, &prompt_directories)
                && !is_likely_false_positive_path(path)
        })
        .collect::<Vec<_>>();
    if !unexpected_paths.is_empty() {
        return Err(format!(
            "Generated code references unverified file paths: {}. Use only the inspected inputs or the user-provided paths.",
            unexpected_paths.join(", ")
        ));
    }

    let required_json_inputs = prompt_files
        .iter()
        .filter(|path| path.to_ascii_lowercase().ends_with(".json"))
        .filter(|path| Path::new(path.as_str()).exists())
        .cloned()
        .collect::<Vec<_>>();
    if !required_json_inputs.is_empty()
        && prompt_lower.contains("read")
        && (prompt_lower.contains("script") || prompt_lower.contains(".py"))
    {
        let mentions_runtime_json_input = required_json_inputs.iter().any(|path| {
            combined.contains(path)
                || Path::new(path)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .map(|name| combined.contains(name))
                    .unwrap_or(false)
        });
        if !mentions_runtime_json_input {
            return Err(format!(
                "Generated code must read the provided JSON input at runtime instead of hardcoding extracted values: {}.",
                required_json_inputs.join(", ")
            ));
        }
    }

    let known_input_paths = prompt_files
        .iter()
        .filter(|path| Path::new(path.as_str()).exists())
        .chain(
            grounded_paths
                .iter()
                .filter(|path| path_looks_like_file(path)),
        )
        .cloned()
        .collect::<HashSet<_>>();
    // Only enforce input-grounding when there are known input files.
    // Tasks without input files (e.g. "create a weather script") should
    // not be blocked by the grounding check.
    if !known_input_paths.is_empty()
        && !script_references_any_known_input(&combined, &known_input_paths)
    {
        return Err(
            "Generated code is not grounded in the inspected input files. Do not substitute mock data or invented datasets; reference the real inputs or pass them through args."
                .to_string(),
        );
    }

    if SPECULATION_RE.is_match(code) {
        return Err(
            "Generated code contains speculative assumptions or placeholder logic. Use only the exact problem statement, real CSV headers, and real data values."
                .to_string(),
        );
    }

    if !grounded_csv_headers.is_empty() {
        let unexpected_columns = extract_indexed_column_names(&combined)
            .into_iter()
            .filter(|column| !grounded_csv_headers.contains(column))
            .collect::<Vec<_>>();
        if !unexpected_columns.is_empty() {
            let mut known_headers = grounded_csv_headers.iter().cloned().collect::<Vec<_>>();
            known_headers.sort();
            return Err(format!(
                "Generated code references unverified CSV columns: {}. Use only inspected headers: {}.",
                unexpected_columns.join(", "),
                known_headers.join(", ")
            ));
        }
    }

    let declared_output_path = extract_declared_output_path_from_leading_comment(code)
        .filter(|path| path_is_within_any_directory(path, &prompt_directories));
    let declared_output_level = declared_output_path
        .as_deref()
        .and_then(extract_level_number_from_output_path);

    if prompt_requires_persisted_level_scripts(prompt) && !prompt_directories.is_empty() {
        let Some(output_path) = declared_output_path.as_ref() else {
            return Err(
                "Persist each generated script by placing the exact absolute output file path in a leading comment line, for example '# /tmp/ds_olympiad/result/library-used/level-1-solution.py'."
                    .to_string(),
            );
        };
        let file_name = Path::new(output_path)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if !LEVEL_OUTPUT_FILE_RE.is_match(file_name) {
            return Err(format!(
                "Use the requested filename format 'level-N-solution.py' for generated scripts, not '{}'.",
                file_name
            ));
        }

        if let Some(level) = declared_output_level.as_ref() {
            let matching_level_inputs = prompt_files
                .iter()
                .filter(|path| {
                    Path::new(path)
                        .file_name()
                        .and_then(|value| value.to_str())
                        .and_then(|file_name| LEVEL_INPUT_RE.captures(file_name))
                        .and_then(|captures| {
                            captures.get(1).map(|value| value.as_str().to_string())
                        })
                        .as_deref()
                        == Some(level.as_str())
                })
                .cloned()
                .collect::<Vec<_>>();
            let matching_level_input_set = matching_level_inputs
                .iter()
                .cloned()
                .collect::<HashSet<_>>();
            if !matching_level_inputs.is_empty()
                && !script_references_any_known_input(&combined, &matching_level_input_set)
            {
                return Err(format!(
                    "Generated level {} script must use the matching inspected input file(s): {}.",
                    level,
                    matching_level_inputs.join(", ")
                ));
            }

            let unrelated_level_inputs = prompt_files
                .iter()
                .filter(|path| {
                    Path::new(path)
                        .file_name()
                        .and_then(|value| value.to_str())
                        .and_then(|file_name| LEVEL_INPUT_RE.captures(file_name))
                        .and_then(|captures| {
                            captures.get(1).map(|value| value.as_str().to_string())
                        })
                        .is_some_and(|candidate| candidate != *level)
                })
                .cloned()
                .collect::<Vec<_>>();
            let cross_level_inputs = unrelated_level_inputs
                .into_iter()
                .filter(|path| {
                    combined.contains(path)
                        || Path::new(path)
                            .file_name()
                            .and_then(|value| value.to_str())
                            .map(|name| combined.contains(name))
                            .unwrap_or(false)
                })
                .collect::<Vec<_>>();
            if !cross_level_inputs.is_empty() {
                return Err(format!(
                    "Generated level {} script references other levels' input files: {}.",
                    level,
                    cross_level_inputs.join(", ")
                ));
            }
        }

        let level_markers = extract_level_answer_markers(code);
        if level_markers.len() > 1 {
            return Err(
                "Generate exactly one level per script. One run_generated_code call must produce one level-N-solution.py file."
                    .to_string(),
            );
        }
    }

    Ok(GeneratedCodeGrounding {
        declared_output_path,
        declared_output_level,
    })
}

fn persist_generated_code_copy(code: &str, output_path: &Path) -> Result<(), String> {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            format!(
                "Failed to create generated output directory '{}': {}",
                parent.display(),
                err
            )
        })?;
    }

    let mut file = std::fs::File::create(output_path).map_err(|err| {
        format!(
            "Failed to create generated output file '{}': {}",
            output_path.display(),
            err
        )
    })?;
    file.write_all(code.as_bytes()).map_err(|err| {
        format!(
            "Failed to write generated output file '{}': {}",
            output_path.display(),
            err
        )
    })?;
    file.flush().map_err(|err| {
        format!(
            "Failed to flush generated output file '{}': {}",
            output_path.display(),
            err
        )
    })
}

fn resolve_workspace_path(session_workdir: &Path, path_str: &str) -> Result<PathBuf, String> {
    if path_str.trim().is_empty() {
        return Err("Missing required path".to_string());
    }

    let path = Path::new(path_str);
    Ok(if path.is_absolute() {
        path.to_path_buf()
    } else {
        session_workdir.join(path)
    })
}

fn specialized_read_tool_for_path(path: &Path) -> Option<&'static str> {
    let ext = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match ext.as_str() {
        "pdf" => Some("extract_document_text"),
        "xlsx" | "csv" => Some("inspect_tabular_data"),
        _ => None,
    }
}

fn decode_tool_uri_component(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = &value[index + 1..index + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    decoded.push(byte);
                    index += 3;
                } else {
                    decoded.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn parse_tool_uri(url: &str) -> Option<(String, HashMap<String, String>)> {
    let raw = url.strip_prefix("tool://")?;
    let (tool_name, raw_query) = raw.split_once('?').unwrap_or((raw, ""));
    if tool_name.trim().is_empty() {
        return None;
    }

    let mut params = HashMap::new();
    for pair in raw_query.split('&') {
        if pair.trim().is_empty() {
            continue;
        }
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key.trim().is_empty() {
            continue;
        }
        params.insert(
            decode_tool_uri_component(key.trim()),
            decode_tool_uri_component(value.trim()),
        );
    }

    Some((tool_name.trim().to_string(), params))
}

fn file_looks_binary(path: &Path) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    if bytes.is_empty() {
        return false;
    }
    let sample = &bytes[..bytes.len().min(4096)];
    if sample.contains(&0) {
        return true;
    }
    let suspicious = sample
        .iter()
        .filter(|byte| !matches!(byte, b'\n' | b'\r' | b'\t' | 0x20..=0x7e))
        .count();
    suspicious * 5 > sample.len()
}

fn collect_successful_file_management_actions(messages: &[LlmMessage]) -> usize {
    messages
        .iter()
        .filter(|message| message.role == "tool")
        .filter(|message| matches!(message.tool_name.as_str(), "file_manager" | "file_write"))
        .filter(|message| {
            message
                .tool_result
                .get("success")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
        })
        .count()
}

fn has_file_completion_candidate_activity(messages: &[LlmMessage]) -> bool {
    collect_successful_file_management_actions(messages) > 0
}

fn collect_successful_file_management_paths(messages: &[LlmMessage]) -> HashSet<String> {
    messages
        .iter()
        .filter(|message| message.role == "tool")
        .filter(|message| matches!(message.tool_name.as_str(), "file_manager" | "file_write"))
        .filter(|message| {
            message
                .tool_result
                .get("success")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
        })
        .filter_map(|message| {
            message
                .tool_result
                .get("path")
                .and_then(|value| value.as_str())
                .map(|value| value.to_string())
        })
        .collect()
}

fn successful_file_management_cutoff_for_target(
    messages: &[LlmMessage],
    candidate: &str,
    absolute: &Path,
) -> usize {
    let absolute_path = absolute.to_string_lossy();

    messages
        .iter()
        .enumerate()
        .rev()
        .find(|(_, message)| {
            message.role == "tool"
                && matches!(message.tool_name.as_str(), "file_manager" | "file_write")
                && message
                    .tool_result
                    .get("success")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false)
                && message
                    .tool_result
                    .get("path")
                    .and_then(|value| value.as_str())
                    .map(|path| path == candidate || path == absolute_path.as_ref())
                    .unwrap_or(false)
        })
        .map(|(index, _)| index + 1)
        .unwrap_or(messages.len())
}

fn missing_file_management_targets(
    prompt: &str,
    session_workdir: &Path,
    messages: &[LlmMessage],
) -> Vec<String> {
    let expected_groups = expected_file_management_targets(prompt);
    if expected_groups.is_empty() {
        return Vec::new();
    }

    let created_paths = collect_successful_file_management_paths(messages);
    expected_groups
        .into_iter()
        .filter_map(|group| {
            let satisfied = group.iter().any(|candidate| {
                let absolute = session_workdir.join(candidate);
                created_paths.contains(absolute.to_string_lossy().as_ref())
                    || created_paths.contains(candidate.as_str())
            });
            if satisfied {
                None
            } else {
                group.first().cloned()
            }
        })
        .collect()
}
