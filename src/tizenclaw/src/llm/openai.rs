//! OpenAI-compatible LLM backend — uses serde_json + ureq.

use serde_json::{json, Value};
use crate::infra::http_client;
use super::backend::*;

pub struct OpenAiBackend {
    api_key: String,
    model: String,
    endpoint: String,
    provider_name: String,
}

impl OpenAiBackend {
    pub fn new(provider: &str) -> Self {
        let (endpoint, model) = match provider {
            "xai" => ("https://api.x.ai/v1", "grok-3-mini-fast"),
            _ => ("https://api.openai.com/v1", "gpt-4o"),
        };
        OpenAiBackend { api_key: String::new(), model: model.into(), endpoint: endpoint.into(), provider_name: provider.into() }
    }
}

impl LlmBackend for OpenAiBackend {
    fn initialize(&mut self, config: &Value) -> bool {
        if let Some(k) = config["api_key"].as_str() { self.api_key = k.into(); }
        if let Some(m) = config["model"].as_str() { self.model = m.into(); }
        if let Some(e) = config["endpoint"].as_str() { self.endpoint = e.into(); }
        !self.api_key.is_empty()
    }

    fn chat(&self, messages: &[LlmMessage], tools: &[LlmToolDecl], _on_chunk: Option<&dyn Fn(&str)>, system_prompt: &str) -> LlmResponse {
        let mut msgs = vec![];
        if !system_prompt.is_empty() {
            msgs.push(json!({"role": "system", "content": system_prompt}));
        }
        for msg in messages {
            if msg.role == "tool" {
                msgs.push(json!({"role": "tool", "content": msg.tool_result.to_string(), "tool_call_id": msg.tool_call_id}));
            } else if !msg.tool_calls.is_empty() {
                let tcs: Vec<Value> = msg.tool_calls.iter().map(|tc| json!({
                    "id": tc.id, "type": "function",
                    "function": {"name": tc.name, "arguments": tc.args.to_string()}
                })).collect();
                let mut m = json!({"role": "assistant", "tool_calls": tcs});
                if !msg.text.is_empty() { m["content"] = json!(msg.text); }
                msgs.push(m);
            } else {
                msgs.push(json!({"role": msg.role, "content": msg.text}));
            }
        }
        let mut req = json!({"model": self.model, "messages": msgs});
        if !tools.is_empty() {
            let tool_arr: Vec<Value> = tools.iter().map(|t| json!({
                "type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.parameters}
            })).collect();
            req["tools"] = Value::Array(tool_arr);
        }

        let url = format!("{}/chat/completions", self.endpoint);
        let auth = format!("Bearer {}", self.api_key);
        let headers = [("Authorization", auth.as_str())];
        let http_resp = http_client::http_post(&url, &headers, &req.to_string(), 1, 60);

        let mut resp = LlmResponse::default();
        resp.http_status = http_resp.status_code;
        if !http_resp.success { resp.error_message = http_resp.error; return resp; }

        if let Ok(json) = serde_json::from_str::<Value>(&http_resp.body) {
            if let Some(msg) = json.pointer("/choices/0/message") {
                resp.text = msg["content"].as_str().unwrap_or("").into();
                if let Some(tcs) = msg["tool_calls"].as_array() {
                    for tc in tcs {
                        let args_str = tc["function"]["arguments"].as_str().unwrap_or("{}");
                        resp.tool_calls.push(LlmToolCall {
                            id: tc["id"].as_str().unwrap_or("").into(),
                            name: tc["function"]["name"].as_str().unwrap_or("").into(),
                            args: serde_json::from_str(args_str).unwrap_or(json!({})),
                        });
                    }
                }
            }
            if let Some(u) = json.get("usage") {
                resp.prompt_tokens = u["prompt_tokens"].as_i64().unwrap_or(0) as i32;
                resp.completion_tokens = u["completion_tokens"].as_i64().unwrap_or(0) as i32;
                resp.total_tokens = u["total_tokens"].as_i64().unwrap_or(0) as i32;
            }
            resp.success = true;
        }
        resp
    }

    fn get_name(&self) -> &str { &self.provider_name }
}
