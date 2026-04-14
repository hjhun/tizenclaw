use serde_json::Value;

pub(crate) fn trimmed_text(text: &str) -> String {
    text.trim().to_string()
}

pub(crate) fn config_string<'a>(config: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut cursor = config;
    for segment in path {
        cursor = cursor.get(*segment)?;
    }
    cursor
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(crate) fn config_i64(config: &Value, path: &[&str]) -> Option<i64> {
    let mut cursor = config;
    for segment in path {
        cursor = cursor.get(*segment)?;
    }
    cursor.as_i64()
}

pub(crate) fn positive_config_i64(config: &Value, path: &[&str]) -> Option<i64> {
    config_i64(config, path).filter(|value| *value > 0)
}

pub(crate) fn json_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(ToString::to_string)
}

pub(crate) fn configured_api_key(
    config: &Value,
    config_path: &[&str],
    env_var: &str,
) -> Option<String> {
    config_string(config, config_path)
        .map(ToString::to_string)
        .or_else(|| {
            std::env::var(env_var)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

pub(crate) fn request_url(endpoint: &str, suffix: &str) -> String {
    let trimmed = endpoint.trim().trim_end_matches('/');
    if trimmed.ends_with(suffix) {
        trimmed.to_string()
    } else {
        format!("{}/{}", trimmed, suffix)
    }
}

pub(crate) fn extract_error_message(body: &str) -> Option<String> {
    let json = serde_json::from_str::<Value>(body).ok()?;
    if let Some(message) = json
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
    {
        return Some(message.to_string());
    }
    json.get("message")
        .and_then(Value::as_str)
        .map(ToString::to_string)
}
