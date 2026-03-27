//! Gemini LLM backend (Google AI) — uses serde_json + ureq.

use serde_json::{json, Value};
use crate::infra::http_client;
use super::backend::*;

pub struct GeminiBackend {
    api_key: String,
    model: String,
    endpoint: String,
}

impl GeminiBackend {
    pub fn new() -> Self {
        GeminiBackend {
            api_key: String::new(),
            model: "gemini-2.5-flash".into(),
            endpoint: "https://generativelanguage.googleapis.com/v1beta".into(),
        }
    }

    fn build_request(&self, messages: &[LlmMessage], tools: &[LlmToolDecl], system_prompt: &str) -> Value {
        let mut req = json!({});

        if !system_prompt.is_empty() {
            req["system_instruction"] = json!({
                "parts": [{"text": system_prompt}]
            });
        }

        let mut contents = vec![];
        for msg in messages {
            let role = match msg.role.as_str() {
                "assistant" => "model",
                "tool" => "function",
                _ => "user",
            };
            let parts = if msg.role == "tool" {
                json!([{"functionResponse": {"name": msg.tool_name, "response": msg.tool_result}}])
            } else if !msg.tool_calls.is_empty() {
                let calls: Vec<Value> = msg.tool_calls.iter().map(|tc| {
                    json!({"functionCall": {"name": tc.name, "args": tc.args}})
                }).collect();
                Value::Array(calls)
            } else {
                json!([{"text": msg.text}])
            };
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
            Err(e) => { resp.error_message = format!("JSON parse error: {}", e); return resp; }
        };

        if let Some(parts) = json.pointer("/candidates/0/content/parts").and_then(|v| v.as_array()) {
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
        }
        resp.success = true;
        resp
    }
}

impl LlmBackend for GeminiBackend {
    fn initialize(&mut self, config: &Value) -> bool {
        if let Some(k) = config["api_key"].as_str() { self.api_key = k.into(); }
        if let Some(m) = config["model"].as_str() { self.model = m.into(); }
        if let Some(e) = config["endpoint"].as_str() { self.endpoint = e.into(); }
        !self.api_key.is_empty()
    }

    fn chat(&self, messages: &[LlmMessage], tools: &[LlmToolDecl], _on_chunk: Option<&dyn Fn(&str)>, system_prompt: &str) -> LlmResponse {
        let body = self.build_request(messages, tools, system_prompt).to_string();
        let url = format!("{}/models/{}:generateContent?key={}", self.endpoint, self.model, self.api_key);
        let http_resp = http_client::http_post(&url, &[], &body, 1, 60);
        let mut resp = if http_resp.success { self.parse_response(&http_resp.body) } else {
            let mut r = LlmResponse::default(); r.error_message = http_resp.error; r
        };
        resp.http_status = http_resp.status_code;
        resp
    }

    fn get_name(&self) -> &str { "gemini" }
}
