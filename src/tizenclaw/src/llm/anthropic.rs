//! Anthropic LLM backend (Claude) — uses serde_json + ureq.

use serde_json::{json, Value};
use crate::infra::http_client;
use super::backend::*;

pub struct AnthropicBackend {
    api_key: String,
    model: String,
    endpoint: String,
}

impl AnthropicBackend {
    pub fn new() -> Self {
        AnthropicBackend { api_key: String::new(), model: "claude-sonnet-4-20250514".into(), endpoint: "https://api.anthropic.com/v1".into() }
    }
}

impl LlmBackend for AnthropicBackend {
    fn initialize(&mut self, config: &Value) -> bool {
        if let Some(k) = config["api_key"].as_str() { self.api_key = k.into(); }
        if let Some(m) = config["model"].as_str() { self.model = m.into(); }
        if let Some(e) = config["endpoint"].as_str() { self.endpoint = e.into(); }
        !self.api_key.is_empty()
    }

    fn chat(&self, messages: &[LlmMessage], tools: &[LlmToolDecl], _on_chunk: Option<&dyn Fn(&str)>, system_prompt: &str) -> LlmResponse {
        let mut req = json!({"model": self.model, "max_tokens": 4096});
        if !system_prompt.is_empty() { req["system"] = json!(system_prompt); }

        let mut msgs = vec![];
        for msg in messages {
            if msg.role == "tool" {
                msgs.push(json!({"role": "user", "content": [{
                    "type": "tool_result", "tool_use_id": msg.tool_call_id,
                    "content": msg.tool_result.to_string()
                }]}));
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
        let headers = [("x-api-key", self.api_key.as_str()), ("anthropic-version", "2023-06-01")];
        let http_resp = http_client::http_post(&url, &headers, &req.to_string(), 1, 60);

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
