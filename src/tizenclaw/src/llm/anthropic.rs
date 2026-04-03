//! Anthropic LLM backend (Claude) — uses serde_json + ureq.

#![allow(clippy::all)]

use serde_json::{json, Value};
use crate::infra::http_client;
use super::backend::*;

pub struct AnthropicBackend {
    api_key: String,
    model: String,
    endpoint: String,
}

impl Default for AnthropicBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AnthropicBackend {
    pub fn new() -> Self {
        AnthropicBackend { api_key: String::new(), model: "claude-sonnet-4-20250514".into(), endpoint: "https://api.anthropic.com/v1".into() }
    }
}

#[async_trait::async_trait]
impl LlmBackend for AnthropicBackend {
    fn initialize(&mut self, config: &Value) -> bool {
        if let Some(k) = config["api_key"].as_str() { self.api_key = k.into(); }
        if let Some(m) = config["model"].as_str() { self.model = m.into(); }
        if let Some(e) = config["endpoint"].as_str() { self.endpoint = e.into(); }
        !self.api_key.is_empty()
    }

    async fn chat(&self, messages: &[LlmMessage], tools: &[LlmToolDecl], _on_chunk: Option<&(dyn Fn(&str) + Send + Sync)>, system_prompt: &str, max_tokens: Option<u32>) -> LlmResponse {
        let mut req = json!({"model": self.model, "max_tokens": max_tokens.unwrap_or(4096)});
        if !system_prompt.is_empty() { 
            req["system"] = json!([{
                "type": "text", 
                "text": system_prompt, 
                "cache_control": {"type": "ephemeral"}
            }]); 
        }

        let mut msgs = vec![];
        for msg in messages {
            if msg.role == "tool" {
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
                if !msg.text.is_empty() {
                    content.push(json!({"type": "text", "text": msg.text}));
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
            } else {
                msgs.push(json!({"role": msg.role, "content": msg.text}));
            }
        }
        req["messages"] = Value::Array(msgs);
        if !tools.is_empty() {
            let arr: Vec<Value> = tools.iter().map(|t| json!({
                "name": t.name, "description": t.description, "input_schema": t.parameters
            })).collect();
            req["tools"] = Value::Array(arr);
        }

        let url = format!("{}/messages", self.endpoint);
        let headers = [
            ("x-api-key", self.api_key.as_str()), 
            ("anthropic-version", "2023-06-01"),
            ("anthropic-beta", "prompt-caching-2024-07-31")
        ];
        let http_resp = http_client::http_post(&url, &headers, &req.to_string(), 1, 60).await;

        let mut resp = LlmResponse::default();
        resp.http_status = http_resp.status_code;
        if !http_resp.success { resp.error_message = http_resp.error; return resp; }

        if let Ok(json) = serde_json::from_str::<Value>(&http_resp.body) {
            if let Some(content) = json["content"].as_array() {
                for block in content {
                    match block["type"].as_str() {
                        Some("text") => { resp.text.push_str(block["text"].as_str().unwrap_or("")); }
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
                resp.prompt_tokens = u["input_tokens"].as_i64().unwrap_or(0) as i32;
                resp.completion_tokens = u["output_tokens"].as_i64().unwrap_or(0) as i32;
                resp.total_tokens = resp.prompt_tokens + resp.completion_tokens;
            }
            resp.success = true;
        }
        resp
    }

    fn get_name(&self) -> &str { "anthropic" }
}
