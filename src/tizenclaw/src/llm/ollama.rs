//! Ollama local LLM backend — uses serde_json + ureq.

#![allow(clippy::all)]

use super::backend::*;
use crate::infra::http_client;
use serde_json::{Value, json};
use std::sync::RwLock;

pub struct OllamaBackend {
    model: String,
    endpoint: String,
    default_max_tokens: Option<u32>,
    reachable: RwLock<Option<bool>>,
}

impl Default for OllamaBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl OllamaBackend {
    pub fn new() -> Self {
        OllamaBackend {
            model: "llama3.2".into(),
            endpoint: "http://localhost:11434".into(),
            default_max_tokens: None,
            reachable: RwLock::new(None),
        }
    }

    fn status_url(&self) -> String {
        format!("{}/api/version", self.endpoint.trim_end_matches('/'))
    }

    fn probe_endpoint(&self) -> bool {
        let response = http_client::http_get_sync(&self.status_url(), &[], 0, 2);
        let reachable = response.success;
        if let Ok(mut guard) = self.reachable.write() {
            *guard = Some(reachable);
        }
        reachable
    }

    fn build_request(
        &self,
        messages: &[LlmMessage],
        system_prompt: &str,
        max_tokens: Option<u32>,
    ) -> Value {
        let mut msgs = vec![];
        if !system_prompt.is_empty() {
            msgs.push(json!({"role": "system", "content": system_prompt}));
        }
        for msg in messages {
            msgs.push(json!({"role": msg.role, "content": msg.text}));
        }

        let mut req = json!({
            "model": self.model,
            "messages": msgs,
            "stream": false
        });
        if let Some(tokens) = max_tokens.or(self.default_max_tokens) {
            req["options"] = json!({
                "num_predict": tokens
            });
        }
        req
    }
}

#[async_trait::async_trait]
impl LlmBackend for OllamaBackend {
    fn initialize(&mut self, config: &Value) -> bool {
        if let Some(m) = config["model"].as_str() {
            self.model = m.into();
        }
        if let Some(e) = config["endpoint"].as_str() {
            self.endpoint = e.into();
        }
        if let Some(tokens) = config["max_tokens"].as_u64() {
            self.default_max_tokens = Some(tokens as u32);
        }
        self.probe_endpoint()
    }

    async fn chat(
        &self,
        messages: &[LlmMessage],
        _tools: &[LlmToolDecl],
        _on_chunk: Option<&(dyn Fn(&str) + Send + Sync)>,
        system_prompt: &str,
        max_tokens: Option<u32>,
    ) -> LlmResponse {
        let req = self.build_request(messages, system_prompt, max_tokens);

        let url = format!("{}/api/chat", self.endpoint);
        let http_resp = http_client::http_post(&url, &[], &req.to_string(), 1, 120).await;

        let mut resp = LlmResponse::default();
        resp.http_status = http_resp.status_code as i32;
        if !http_resp.success {
            resp.error_message = http_resp.error;
            return resp;
        }

        if let Ok(json) = serde_json::from_str::<Value>(&http_resp.body) {
            resp.text = json["message"]["content"].as_str().unwrap_or("").into();
            resp.success = true;
        }
        resp
    }

    fn get_name(&self) -> &str {
        "ollama"
    }

    fn is_configured(&self) -> bool {
        self.reachable
            .read()
            .ok()
            .and_then(|guard| *guard)
            .unwrap_or_else(|| self.probe_endpoint())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ollama_request_omits_num_predict_when_unset() {
        let backend = OllamaBackend::new();
        let req = backend.build_request(&[LlmMessage::user("hello")], "", None);

        assert!(req.get("options").is_none());
    }

    #[test]
    fn ollama_reports_unconfigured_when_endpoint_is_down() {
        let mut backend = OllamaBackend::new();
        let ok = backend.initialize(&json!({
            "endpoint": "http://127.0.0.1:9"
        }));

        assert!(!ok);
        assert!(!backend.is_configured());
    }
}
