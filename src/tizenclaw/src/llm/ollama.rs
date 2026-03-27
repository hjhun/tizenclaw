//! Ollama local LLM backend — uses serde_json + ureq.

use serde_json::{json, Value};
use crate::infra::http_client;
use super::backend::*;

pub struct OllamaBackend {
    model: String,
    endpoint: String,
}

impl OllamaBackend {
    pub fn new() -> Self {
        OllamaBackend { model: "llama3".into(), endpoint: "http://localhost:11434".into() }
    }
}

impl LlmBackend for OllamaBackend {
    fn initialize(&mut self, config: &Value) -> bool {
        if let Some(m) = config["model"].as_str() { self.model = m.into(); }
        if let Some(e) = config["endpoint"].as_str() { self.endpoint = e.into(); }
        true
    }

    fn chat(&self, messages: &[LlmMessage], _tools: &[LlmToolDecl], _on_chunk: Option<&dyn Fn(&str)>, system_prompt: &str) -> LlmResponse {
        let mut msgs = vec![];
        if !system_prompt.is_empty() {
            msgs.push(json!({"role": "system", "content": system_prompt}));
        }
        for msg in messages {
            msgs.push(json!({"role": msg.role, "content": msg.text}));
        }
        let req = json!({"model": self.model, "messages": msgs, "stream": false});

        let url = format!("{}/api/chat", self.endpoint);
        let http_resp = http_client::http_post(&url, &[], &req.to_string(), 1, 120);

        let mut resp = LlmResponse::default();
        resp.http_status = http_resp.status_code;
        if !http_resp.success { resp.error_message = http_resp.error; return resp; }

        if let Ok(json) = serde_json::from_str::<Value>(&http_resp.body) {
            resp.text = json["message"]["content"].as_str().unwrap_or("").into();
            resp.success = true;
        }
        resp
    }

    fn get_name(&self) -> &str { "ollama" }
}
