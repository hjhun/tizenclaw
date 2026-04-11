//! LLM Backend abstraction layer.
//!
//! Uses serde/serde_json for all data serialization.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A tool call requested by the LLM.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LlmToolCall {
    pub id: String,
    pub name: String,
    pub args: Value,
}

/// A message in a conversation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: String,
    pub text: String,
    #[serde(default)]
    pub reasoning_text: String,
    #[serde(default)]
    pub tool_calls: Vec<LlmToolCall>,
    #[serde(default)]
    pub tool_name: String,
    #[serde(default)]
    pub tool_call_id: String,
    #[serde(default)]
    pub tool_result: Value,
}

impl LlmMessage {
    pub fn user(text: &str) -> Self {
        LlmMessage {
            role: "user".into(),
            text: text.into(),
            reasoning_text: String::new(),
            tool_calls: vec![],
            tool_name: String::new(),
            tool_call_id: String::new(),
            tool_result: Value::Null,
        }
    }
    pub fn assistant(text: &str) -> Self {
        LlmMessage {
            role: "assistant".into(),
            text: text.into(),
            reasoning_text: String::new(),
            tool_calls: vec![],
            tool_name: String::new(),
            tool_call_id: String::new(),
            tool_result: Value::Null,
        }
    }
    pub fn tool_result(call_id: &str, name: &str, result: Value) -> Self {
        LlmMessage {
            role: "tool".into(),
            text: String::new(),
            reasoning_text: String::new(),
            tool_calls: vec![],
            tool_name: name.into(),
            tool_call_id: call_id.into(),
            tool_result: result,
        }
    }
}

impl Default for LlmMessage {
    fn default() -> Self {
        LlmMessage {
            role: String::new(),
            text: String::new(),
            reasoning_text: String::new(),
            tool_calls: vec![],
            tool_name: String::new(),
            tool_call_id: String::new(),
            tool_result: Value::Null,
        }
    }
}

/// Response from the LLM.
#[derive(Clone, Debug, Default)]
pub struct LlmResponse {
    pub success: bool,
    pub text: String,
    pub reasoning_text: String,
    pub error_message: String,
    pub tool_calls: Vec<LlmToolCall>,
    pub prompt_tokens: i32,
    pub completion_tokens: i32,
    pub total_tokens: i32,
    pub cache_creation_input_tokens: i32,
    pub cache_read_input_tokens: i32,
    pub http_status: i32,
}

impl LlmResponse {
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}

/// Tool declaration for function calling.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmToolDecl {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// Abstract LLM backend interface.
#[async_trait::async_trait]
pub trait LlmBackend: Send + Sync {
    /// Compatibility entry point used by AgentCore today.
    fn initialize(&mut self, config: &Value) -> bool;

    /// Compatibility entry point used by AgentCore today.
    async fn chat(
        &self,
        messages: &[LlmMessage],
        tools: &[LlmToolDecl],
        on_chunk: Option<&(dyn Fn(&str) + Send + Sync)>,
        system_prompt: &str,
        max_tokens: Option<u32>,
    ) -> LlmResponse;

    /// Compatibility entry point used by AgentCore today.
    fn get_name(&self) -> &str;
    fn shutdown(&mut self) {}

    /// Prompt-compatible metadata helper.
    fn name(&self) -> &str {
        self.get_name()
    }

    /// Prompt-compatible configuration helper.
    fn configure(&mut self, config: &Value) -> Result<(), String> {
        if self.initialize(config) {
            Ok(())
        } else {
            Err(format!("Backend '{}' is not configured", self.get_name()))
        }
    }

    /// Prompt-compatible readiness helper. Backends with stronger
    /// reachability checks should override this.
    fn is_configured(&self) -> bool {
        true
    }

    /// Optional backend priority hook for future selection logic.
    fn priority(&self) -> i64 {
        0
    }

    /// Optionally pre-cache the system prompt server-side before the first
    /// `chat()` call. Returns `true` if a cache was successfully prepared
    /// (subsequent `chat()` calls may then omit the inline system prompt).
    ///
    /// Default: no-op returning `false`. Only backends that support
    /// server-side prompt caching (e.g. Gemini) should override this.
    async fn prepare_cache(&self, _system_prompt: &str) -> bool {
        false
    }
}

/// Create an LLM backend by name.
pub fn create_backend(name: &str) -> Option<Box<dyn LlmBackend>> {
    match name {
        "gemini" => Some(Box::new(super::gemini::GeminiBackend::new())),
        "openai" | "openai-codex" | "xai" => {
            Some(Box::new(super::openai::OpenAiBackend::new(name)))
        }
        "anthropic" => Some(Box::new(super::anthropic::AnthropicBackend::new())),
        "ollama" => Some(Box::new(super::ollama::OllamaBackend::new())),
        _ => None,
    }
}
