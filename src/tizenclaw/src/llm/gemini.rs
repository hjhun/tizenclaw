//! Gemini LLM backend (Google AI) — uses serde_json + ureq.
//!
//! ## Prompt Caching
//! Supports Gemini `CachedContent` API to avoid re-sending the system prompt
//! on every round. Call `prepare_cache()` before `chat()` to create/refresh
//! the cache. The `cached_content_name` is stored in a `RwLock` so multiple
//! concurrent sessions share a single cached system prompt reference.
//!
//! Fallback: if cache creation fails, `chat()` falls back to inline
//! `system_instruction` transparently.

#![allow(clippy::all)]

use super::backend::*;
use crate::infra::http_client;
use serde_json::{json, Value};
use std::sync::RwLock;

pub struct GeminiBackend {
    api_key: String,
    model: String,
    endpoint: String,
    temperature: Option<f64>,
    default_max_tokens: Option<u32>,
    /// Cached system-prompt name returned by Gemini CachedContent API.
    /// `None` means no cache is active; fall back to inline system_instruction.
    cached_content_name: RwLock<Option<String>>,
    /// When `true`, `prepare_cache()` will attempt to create a server-side
    /// CachedContent. Default: `false`.
    prompt_cache_enabled: bool,
}

impl Default for GeminiBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl GeminiBackend {
    pub fn new() -> Self {
        GeminiBackend {
            api_key: String::new(),
            model: "gemini-2.5-flash".into(),
            endpoint: "https://generativelanguage.googleapis.com/v1beta".into(),
            temperature: None,
            default_max_tokens: Some(4096),
            cached_content_name: RwLock::new(None),
            prompt_cache_enabled: false,
        }
    }

    fn trimmed_text(text: &str) -> String {
        text.trim().to_string()
    }

    /// Build the generateContent request body.
    ///
    /// If `cached_name` is `Some`, the request references the cached system
    /// prompt instead of embedding it inline. Otherwise, `system_instruction`
    /// is inlined as before.
    fn build_request(
        &self,
        messages: &[LlmMessage],
        tools: &[LlmToolDecl],
        system_prompt: &str,
        cached_name: Option<&str>,
        max_tokens: Option<u32>,
    ) -> Value {
        let mut req = json!({});

        let resolved_tokens = max_tokens.or(self.default_max_tokens);
        if resolved_tokens.is_some() || self.temperature.is_some() {
            let mut generation_config = json!({});
            if let Some(tokens) = resolved_tokens {
                generation_config["maxOutputTokens"] = json!(tokens);
            }
            if let Some(temperature) = self.temperature {
                generation_config["temperature"] = json!(temperature);
            }
            req["generationConfig"] = generation_config;
        }

        if let Some(name) = cached_name {
            // Reference the server-side cached content (avoids re-sending
            // the full system prompt, which is the largest token cost).
            req["cachedContent"] = json!(name);
        } else if !system_prompt.is_empty() {
            req["system_instruction"] = json!({
                "parts": [{"text": system_prompt}]
            });
        }

        let mut valid_tools = std::collections::HashSet::new();
        for t in tools {
            valid_tools.insert(t.name.as_str());
        }

        let mut contents = vec![];
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

            let role = match msg.role.as_str() {
                "assistant" => "model",
                "tool" => "user", // openclaw aligns toolResult with "user" role
                _ => "user",
            };

            let parts = if is_downgraded {
                if msg.role == "tool" {
                    json!([{"text": format!("[Historical Tool Result for '{}']: {}", msg.tool_name, msg.tool_result)}])
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
                    json!([{"text": full_text}])
                } else if !text.is_empty() {
                    json!([{"text": text}])
                } else {
                    json!([])
                }
            } else if msg.role == "tool" {
                json!([{"functionResponse": {"name": msg.tool_name, "response": {"output": msg.tool_result.to_string()}}}])
            } else if !msg.tool_calls.is_empty() {
                let mut arr = vec![];
                if !text.is_empty() {
                    arr.push(json!({"text": text}));
                }
                for tc in &msg.tool_calls {
                    arr.push(json!({"functionCall": {"name": tc.name, "args": tc.args}}));
                }
                Value::Array(arr)
            } else if !text.is_empty() {
                json!([{"text": text}])
            } else {
                json!([])
            };
            if parts
                .as_array()
                .map(|items| items.is_empty())
                .unwrap_or(false)
            {
                continue;
            }
            contents.push(json!({"role": role, "parts": parts}));
        }
        req["contents"] = Value::Array(contents);

        if !tools.is_empty() {
            let decls: Vec<Value> = tools.iter().map(|t| {
                json!({"name": t.name, "description": t.description, "parameters": t.parameters})
            }).collect();
            req["tools"] = json!([{"function_declarations": decls}]);
        }
        req
    }

    fn parse_response(&self, body: &str) -> LlmResponse {
        let mut resp = LlmResponse::default();
        let json: Value = match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => {
                resp.error_message = format!("JSON parse error: {}", e);
                return resp;
            }
        };

        if let Some(parts) = json
            .pointer("/candidates/0/content/parts")
            .and_then(|v| v.as_array())
        {
            for part in parts {
                if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                    resp.text.push_str(text);
                }
                if let Some(fc) = part.get("functionCall") {
                    resp.tool_calls.push(LlmToolCall {
                        id: format!("call_{}", resp.tool_calls.len()),
                        name: fc["name"].as_str().unwrap_or("").into(),
                        args: fc.get("args").cloned().unwrap_or(json!({})),
                    });
                }
            }
        }
        if let Some(usage) = json.get("usageMetadata") {
            resp.prompt_tokens = usage["promptTokenCount"].as_i64().unwrap_or(0) as i32;
            resp.completion_tokens = usage["candidatesTokenCount"].as_i64().unwrap_or(0) as i32;
            resp.total_tokens = usage["totalTokenCount"].as_i64().unwrap_or(0) as i32;
            // cachedContentTokenCount shows how many tokens came from cache
            if let Some(cached_t) = usage["cachedContentTokenCount"].as_i64() {
                resp.cache_read_input_tokens = cached_t as i32;
                if cached_t > 0 {
                    log::debug!(
                        "[GeminiCache] Cache hit: {} cached tokens (saved ~{} prompt tokens)",
                        cached_t,
                        cached_t
                    );
                }
            }
        }
        resp.success = true;
        resp
    }

    /// Create or refresh a Gemini CachedContent for the given system prompt.
    ///
    /// On success, stores the cache resource name in `self.cached_content_name`
    /// and returns `true`. On failure, clears the cache name and returns `false`
    /// so that chat() falls back to inline system_instruction.
    pub async fn create_or_refresh_cache(&self, system_prompt: &str) -> bool {
        if self.api_key.is_empty() || system_prompt.is_empty() {
            return false;
        }

        // Gemini requires at least 32,768 tokens in the cached content to be
        // eligible for caching. For shorter prompts we skip the cache.
        // Rough heuristic: < 1,000 chars ≈ < 300 tokens — skip cache.
        if system_prompt.len() < 1_000 {
            log::debug!(
                "[GeminiCache] System prompt too short for caching ({} chars), skipping",
                system_prompt.len()
            );
            return false;
        }

        let url = format!("{}/cachedContents?key={}", self.endpoint, self.api_key);

        // TTL: 1 hour (3600 seconds). Gemini supports up to 1 hour by default.
        let body = json!({
            "model": format!("models/{}", self.model),
            "system_instruction": {
                "parts": [{"text": system_prompt}]
            },
            "ttl": "3600s"
        })
        .to_string();

        let http_resp = http_client::http_post(&url, &[], &body, 1, 30).await;

        if !http_resp.success {
            log::warn!(
                "[GeminiCache] Cache creation failed (HTTP {}): {}",
                http_resp.status_code,
                http_resp.error
            );
            if let Ok(mut guard) = self.cached_content_name.write() {
                *guard = None;
            }
            return false;
        }

        let parsed: Value = match serde_json::from_str(&http_resp.body) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("[GeminiCache] Cache response parse error: {}", e);
                return false;
            }
        };

        if let Some(name) = parsed["name"].as_str() {
            log::debug!(
                "[GeminiCache] Cache created: {} ({} chars prompt)",
                name,
                system_prompt.len()
            );
            if let Ok(mut guard) = self.cached_content_name.write() {
                *guard = Some(name.to_string());
            }
            true
        } else {
            log::warn!(
                "[GeminiCache] No 'name' field in cache response: {}",
                http_resp.body
            );
            false
        }
    }

    /// Clear the cached content reference (does NOT delete it server-side).
    pub fn clear_cache(&self) {
        if let Ok(mut guard) = self.cached_content_name.write() {
            *guard = None;
        }
    }
}

#[async_trait::async_trait]
impl LlmBackend for GeminiBackend {
    fn initialize(&mut self, config: &Value) -> bool {
        if let Some(k) = config["api_key"].as_str() {
            self.api_key = k.into();
        }
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
        if let Some(enabled) = config["prompt_cache"].as_bool() {
            self.prompt_cache_enabled = enabled;
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
        // Read cached content name (non-blocking shared read)
        let cached_name_opt: Option<String> =
            self.cached_content_name.read().ok().and_then(|g| g.clone());

        let body = self
            .build_request(
                messages,
                tools,
                system_prompt,
                cached_name_opt.as_deref(),
                max_tokens,
            )
            .to_string();

        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.endpoint, self.model, self.api_key
        );
        let http_resp = http_client::http_post(&url, &[], &body, 1, 60).await;
        let mut resp = if http_resp.success {
            self.parse_response(&http_resp.body)
        } else {
            let mut r = LlmResponse::default();
            r.error_message = http_resp.error;
            r
        };
        resp.http_status = http_resp.status_code;
        resp
    }

    async fn prepare_cache(&self, system_prompt: &str) -> bool {
        if self.prompt_cache_enabled {
            self.create_or_refresh_cache(system_prompt).await
        } else {
            false
        }
    }

    fn get_name(&self) -> &str {
        "gemini"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_request_inline_system_prompt() {
        let backend = GeminiBackend::new();
        let msgs = vec![LlmMessage::user("hello")];
        let req = backend.build_request(&msgs, &[], "You are TizenClaw.", None, None);
        assert!(req.get("system_instruction").is_some());
        assert!(req.get("cachedContent").is_none());
    }

    #[test]
    fn test_build_request_with_cached_name() {
        let backend = GeminiBackend::new();
        let msgs = vec![LlmMessage::user("hello")];
        let req = backend.build_request(
            &msgs,
            &[],
            "ignored prompt",
            Some("cachedContents/abc123"),
            None,
        );
        // cachedContent present, system_instruction must NOT be present
        assert!(req.get("cachedContent").is_some());
        assert!(req.get("system_instruction").is_none());
        assert_eq!(req["cachedContent"].as_str(), Some("cachedContents/abc123"));
    }

    #[test]
    fn test_clear_cache() {
        let backend = GeminiBackend::new();
        // Manually set the cache name
        {
            let mut g = backend.cached_content_name.write().unwrap();
            *g = Some("cachedContents/test".into());
        }
        backend.clear_cache();
        let guard = backend.cached_content_name.read().unwrap();
        assert!(guard.is_none());
    }
}
