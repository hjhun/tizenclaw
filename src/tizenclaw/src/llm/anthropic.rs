//! Anthropic LLM backend (Claude) — uses serde_json + ureq.

#![allow(clippy::all)]

use super::backend::*;
use crate::infra::http_client;
use serde_json::{json, Value};

const ANTHROPIC_REQUIRED_FALLBACK_MAX_TOKENS: u32 = 4096;

pub struct AnthropicBackend {
    api_key: String,
    model: String,
    endpoint: String,
    temperature: Option<f64>,
    default_max_tokens: Option<u32>,
    /// Thinking level: "off", "low", "medium", "high".
    /// For Claude Sonnet/Opus 4.5+: uses extended thinking with budget_tokens.
    thinking_level: Option<String>,
    /// When `true`, request Anthropic prompt caching headers and annotate
    /// the system prompt with cache_control metadata.
    prompt_cache_enabled: bool,
}

impl Default for AnthropicBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AnthropicBackend {
    pub fn new() -> Self {
        AnthropicBackend {
            api_key: String::new(),
            model: "claude-sonnet-4-20250514".into(),
            endpoint: "https://api.anthropic.com/v1".into(),
            temperature: None,
            default_max_tokens: None,
            thinking_level: None,
            prompt_cache_enabled: false,
        }
    }

    fn apply_usage(resp: &mut LlmResponse, usage: &Value) {
        resp.prompt_tokens = usage["input_tokens"].as_i64().unwrap_or(0) as i32;
        resp.completion_tokens = usage["output_tokens"].as_i64().unwrap_or(0) as i32;
        resp.total_tokens = resp.prompt_tokens + resp.completion_tokens;
        resp.cache_creation_input_tokens =
            usage["cache_creation_input_tokens"].as_i64().unwrap_or(0) as i32;
        resp.cache_read_input_tokens =
            usage["cache_read_input_tokens"].as_i64().unwrap_or(0) as i32;
        if resp.cache_read_input_tokens > 0 {
            log::info!(
                "[AnthropicCache] Cache hit: {} cached input tokens reused",
                resp.cache_read_input_tokens
            );
        }
        if resp.cache_creation_input_tokens > 0 {
            log::debug!(
                "[AnthropicCache] Cache write: {} input tokens added to cache",
                resp.cache_creation_input_tokens
            );
        }
    }

    fn trimmed_text(text: &str) -> String {
        text.trim().to_string()
    }

    fn normalized_model(model: &str) -> String {
        if model.starts_with("claude-") && model.contains('.') {
            let normalized = model.replace('.', "-");
            if normalized != model {
                log::info!(
                    "[Anthropic] Normalized model alias '{}' -> '{}'",
                    model,
                    normalized
                );
            }
            normalized
        } else {
            model.to_string()
        }
    }

    fn request_url(endpoint: &str) -> String {
        let trimmed = endpoint.trim().trim_end_matches('/');
        if trimmed.ends_with("/messages") {
            trimmed.to_string()
        } else {
            format!("{}/messages", trimmed)
        }
    }

    fn extract_error_message(body: &str) -> Option<String> {
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
            .map(|message| message.to_string())
    }

    fn configured_api_key(config: &Value) -> String {
        config["api_key"]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .or_else(|| {
                std::env::var("ANTHROPIC_API_KEY")
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            })
            .unwrap_or_default()
    }
}

#[async_trait::async_trait]
impl LlmBackend for AnthropicBackend {
    fn initialize(&mut self, config: &Value) -> bool {
        self.api_key = Self::configured_api_key(config);
        if let Some(m) = config["model"].as_str() {
            self.model = m.into();
        }
        if let Some(e) = config["endpoint"].as_str() {
            self.endpoint = e.into();
        }
        if let Some(t) = config["temperature"].as_f64() {
            self.temperature = Some(t);
        }
        if let Some(tokens) = config["max_tokens"].as_u64() {
            self.default_max_tokens = Some(tokens as u32);
        }
        if let Some(enabled) = config["prompt_cache_enabled"]
            .as_bool()
            .or_else(|| config["prompt_cache"].as_bool())
        {
            self.prompt_cache_enabled = enabled;
        }
        if let Some(tl) = config["thinking_level"].as_str() {
            self.thinking_level = Some(tl.into());
        }
        !self.api_key.is_empty()
    }

    async fn chat(
        &self,
        messages: &[LlmMessage],
        tools: &[LlmToolDecl],
        _on_chunk: Option<&(dyn Fn(&str) + Send + Sync)>,
        system_prompt: &str,
        max_tokens: Option<u32>,
    ) -> LlmResponse {
        let mut req = json!({
            "model": Self::normalized_model(&self.model),
            "max_tokens": max_tokens
                .or(self.default_max_tokens)
                .unwrap_or(ANTHROPIC_REQUIRED_FALLBACK_MAX_TOKENS)
        });
        if let Some(temperature) = self.temperature {
            req["temperature"] = json!(temperature);
        }

        // Extended thinking configuration
        if let Some(ref level) = self.thinking_level {
            let lower = level.to_lowercase();
            if lower != "off" {
                let budget: u32 = match lower.as_str() {
                    "low" => 2048,
                    "medium" => 8192,
                    "high" => 32768,
                    _ => 8192,
                };
                req["thinking"] = json!({
                    "type": "enabled",
                    "budget_tokens": budget
                });
                // Temperature is incompatible with extended thinking — remove it
                if self.temperature.is_some() {
                    log::info!(
                        "[Anthropic] Removing temperature (incompatible with extended thinking)"
                    );
                }
                if let Some(obj) = req.as_object_mut() {
                    obj.remove("temperature");
                }
                // Ensure max_tokens can accommodate thinking budget + output
                let current_max = req["max_tokens"]
                    .as_u64()
                    .unwrap_or(ANTHROPIC_REQUIRED_FALLBACK_MAX_TOKENS as u64);
                if current_max < (budget as u64 + 4096) {
                    req["max_tokens"] = json!(budget + 4096);
                }
            }
        }

        if !system_prompt.is_empty() {
            if self.prompt_cache_enabled {
                req["system"] = json!([{
                    "type": "text",
                    "text": system_prompt,
                    "cache_control": {"type": "ephemeral"}
                }]);
            } else {
                req["system"] = json!([{
                    "type": "text",
                    "text": system_prompt
                }]);
            }
        }

        let mut valid_tools = std::collections::HashSet::new();
        for t in tools {
            valid_tools.insert(t.name.as_str());
        }

        let mut msgs = vec![];
        for msg in messages {
            let text = Self::trimmed_text(&msg.text);
            let mut is_downgraded = false;
            if msg.role == "tool" && !valid_tools.contains(msg.tool_name.as_str()) {
                is_downgraded = true;
            }
            if !msg.tool_calls.is_empty()
                && msg
                    .tool_calls
                    .iter()
                    .any(|tc| !valid_tools.contains(tc.name.as_str()))
            {
                is_downgraded = true;
            }

            if is_downgraded {
                if msg.role == "tool" {
                    msgs.push(json!({"role": "user", "content": format!("[Historical Tool Result for '{}']: {}", msg.tool_name, msg.tool_result)}));
                } else if !msg.tool_calls.is_empty() {
                    let calls_text = msg
                        .tool_calls
                        .iter()
                        .map(|tc| format!("Called tool '{}' with args '{}'", tc.name, tc.args))
                        .collect::<Vec<_>>()
                        .join("\n");
                    let full_text = if text.is_empty() {
                        calls_text
                    } else {
                        format!("{}\n\n{}", text, calls_text)
                    };
                    msgs.push(json!({"role": "assistant", "content": full_text}));
                } else if !text.is_empty() {
                    msgs.push(json!({"role": msg.role, "content": text}));
                }
            } else if msg.role == "tool" {
                let content_str = match msg.tool_result.as_str() {
                    Some(s) => s.to_string(),
                    None => msg.tool_result.to_string(),
                };
                msgs.push(json!({"role": "user", "content": [{
                    "type": "tool_result", "tool_use_id": msg.tool_call_id,
                    "content": content_str
                }]}));
            } else if msg.role == "assistant" && !msg.tool_calls.is_empty() {
                let mut content = vec![];
                if !text.is_empty() {
                    content.push(json!({"type": "text", "text": text}));
                }
                for tc in &msg.tool_calls {
                    content.push(json!({
                        "type": "tool_use",
                        "id": tc.id,
                        "name": tc.name,
                        "input": tc.args
                    }));
                }
                msgs.push(json!({"role": "assistant", "content": content}));
            } else if !text.is_empty() {
                msgs.push(json!({"role": msg.role, "content": text}));
            }
        }
        req["messages"] = Value::Array(msgs);
        if !tools.is_empty() {
            let arr: Vec<Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "name": t.name, "description": t.description, "input_schema": t.parameters
                    })
                })
                .collect();
            req["tools"] = Value::Array(arr);
        }

        let url = Self::request_url(&self.endpoint);
        let http_resp = if self.prompt_cache_enabled {
            let headers = [
                ("x-api-key", self.api_key.as_str()),
                ("anthropic-version", "2023-06-01"),
                ("anthropic-beta", "prompt-caching-2024-07-31"),
            ];
            http_client::http_post(&url, &headers, &req.to_string(), 1, 60).await
        } else {
            let headers = [
                ("x-api-key", self.api_key.as_str()),
                ("anthropic-version", "2023-06-01"),
            ];
            http_client::http_post(&url, &headers, &req.to_string(), 1, 60).await
        };

        let mut resp = LlmResponse::default();
        resp.http_status = http_resp.status_code as i32;
        if !http_resp.success {
            resp.error_message = Self::extract_error_message(&http_resp.body)
                .map(|message| format!("{} ({})", message, http_resp.error))
                .unwrap_or(http_resp.error);
            return resp;
        }

        if let Ok(json) = serde_json::from_str::<Value>(&http_resp.body) {
            if let Some(content) = json["content"].as_array() {
                for block in content {
                    match block["type"].as_str() {
                        Some("thinking") => {
                            resp.reasoning_text
                                .push_str(block["thinking"].as_str().unwrap_or(""));
                        }
                        Some("text") => {
                            resp.text.push_str(block["text"].as_str().unwrap_or(""));
                        }
                        Some("tool_use") => {
                            resp.tool_calls.push(LlmToolCall {
                                id: block["id"].as_str().unwrap_or("").into(),
                                name: block["name"].as_str().unwrap_or("").into(),
                                args: block.get("input").cloned().unwrap_or(json!({})),
                            });
                        }
                        _ => {}
                    }
                }
            }
            if let Some(u) = json.get("usage") {
                Self::apply_usage(&mut resp, u);
            }
            resp.success = true;
        }
        resp
    }

    fn get_name(&self) -> &str {
        "anthropic"
    }

    fn is_configured(&self) -> bool {
        !self.api_key.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_usage_reads_cache_counters() {
        let mut resp = LlmResponse::default();
        let usage = json!({
            "input_tokens": 1000,
            "output_tokens": 250,
            "cache_creation_input_tokens": 800,
            "cache_read_input_tokens": 640
        });

        AnthropicBackend::apply_usage(&mut resp, &usage);

        assert_eq!(resp.prompt_tokens, 1000);
        assert_eq!(resp.completion_tokens, 250);
        assert_eq!(resp.total_tokens, 1250);
        assert_eq!(resp.cache_creation_input_tokens, 800);
        assert_eq!(resp.cache_read_input_tokens, 640);
    }

    #[test]
    fn normalized_model_accepts_short_alias_with_dot() {
        assert_eq!(
            AnthropicBackend::normalized_model("claude-sonnet-4.6"),
            "claude-sonnet-4-6"
        );
        assert_eq!(
            AnthropicBackend::normalized_model("claude-opus-4.1"),
            "claude-opus-4-1"
        );
    }

    #[test]
    fn request_url_accepts_base_or_messages_endpoint() {
        assert_eq!(
            AnthropicBackend::request_url("https://api.anthropic.com/v1"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            AnthropicBackend::request_url("https://api.anthropic.com/v1/"),
            "https://api.anthropic.com/v1/messages"
        );
        assert_eq!(
            AnthropicBackend::request_url("https://api.anthropic.com/v1/messages"),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn extract_error_message_reads_nested_anthropic_error() {
        let body =
            r#"{"type":"error","error":{"type":"not_found_error","message":"model not found"}}"#;
        assert_eq!(
            AnthropicBackend::extract_error_message(body).as_deref(),
            Some("model not found")
        );
    }

    #[test]
    fn initialize_reads_prompt_cache_and_thinking_level() {
        let mut backend = AnthropicBackend::new();
        let ok = backend.initialize(&json!({
            "api_key": "test-key",
            "prompt_cache_enabled": true,
            "thinking_level": "medium"
        }));

        assert!(ok);
        assert!(backend.prompt_cache_enabled);
        assert_eq!(backend.thinking_level.as_deref(), Some("medium"));
    }

    #[test]
    fn initialize_reads_api_key_from_env() {
        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "env-key");
        }

        let mut backend = AnthropicBackend::new();
        let ok = backend.initialize(&json!({}));

        unsafe {
            std::env::remove_var("ANTHROPIC_API_KEY");
        }

        assert!(ok);
        assert!(backend.is_configured());
    }
}
