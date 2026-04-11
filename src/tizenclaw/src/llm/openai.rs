//! OpenAI-compatible LLM backend with OpenAI Codex OAuth/Responses support.

#![allow(clippy::all)]

use super::backend::*;
use crate::infra::http_client;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const CHATGPT_BACKEND_API: &str = "https://chatgpt.com/backend-api";
const OPENAI_RESPONSES_PATH: &str = "/responses";
const OPENAI_CODEX_RESPONSES_PATH: &str = "/codex/responses";
const OPENAI_CHAT_COMPLETIONS_PATH: &str = "/chat/completions";
const OPENAI_CODEX_OAUTH_ENDPOINT: &str = "https://auth.openai.com/oauth/token";
const OPENAI_CODEX_CLIENT_ID: &str = "managedreloadapp_EMoamEEZ73f0CkXaXp7hrann";
const OPENAI_CODEX_REFRESH_SKEW_SECS: i64 = 300;
const OPENAI_CODEX_USER_AGENT: &str = "CodexBar";
const OPENAI_CODEX_DEFAULT_INSTRUCTIONS: &str = "You are a helpful assistant.";

#[derive(Clone, Debug, PartialEq, Eq)]
enum OpenAiTransport {
    ChatCompletions,
    Responses,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CodexAuthSource {
    Config,
    CodexCli(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CodexAuthState {
    access_token: String,
    refresh_token: String,
    id_token: Option<String>,
    account_id: Option<String>,
    expires_at: Option<i64>,
    last_refresh: Option<String>,
    source: CodexAuthSource,
}

pub struct OpenAiBackend {
    api_key: String,
    model: String,
    endpoint: String,
    api_path: String,
    provider_name: String,
    transport: OpenAiTransport,
    service_tier: Option<String>,
    codex_auth: Mutex<Option<CodexAuthState>>,
}

impl OpenAiBackend {
    pub fn new(provider: &str) -> Self {
        let (endpoint, model, api_path, transport, service_tier) = match provider {
            "xai" => (
                "https://api.x.ai/v1",
                "grok-3-mini-fast",
                OPENAI_CHAT_COMPLETIONS_PATH,
                OpenAiTransport::ChatCompletions,
                None,
            ),
            "openai-codex" => (
                CHATGPT_BACKEND_API,
                "gpt-5.4",
                OPENAI_CODEX_RESPONSES_PATH,
                OpenAiTransport::Responses,
                None,
            ),
            _ => (
                "https://api.openai.com/v1",
                "gpt-4o",
                OPENAI_CHAT_COMPLETIONS_PATH,
                OpenAiTransport::ChatCompletions,
                None,
            ),
        };
        OpenAiBackend {
            api_key: String::new(),
            model: model.into(),
            endpoint: endpoint.into(),
            api_path: api_path.into(),
            provider_name: provider.into(),
            transport,
            service_tier,
            codex_auth: Mutex::new(None),
        }
    }

    fn trimmed_text(text: &str) -> String {
        text.trim().to_string()
    }

    fn resolve_responses_tool_name<'a>(
        tool_name: &'a str,
        valid_tools: &std::collections::HashSet<String>,
    ) -> Option<&'a str> {
        let has_tool = |candidate: &str| valid_tools.iter().any(|name| name == candidate);
        let trimmed = tool_name.trim();
        if trimmed.is_empty() {
            return None;
        }
        if has_tool(trimmed) {
            return Some(trimmed);
        }

        let canonical = match trimmed {
            "read_file" | "write_file" | "list_files" => "file_manager",
            other => other,
        };

        has_tool(canonical).then_some(canonical)
    }

    fn config_string<'a>(config: &'a Value, path: &[&str]) -> Option<&'a str> {
        let mut cursor = config;
        for segment in path {
            cursor = cursor.get(*segment)?;
        }
        cursor
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    fn config_i64(config: &Value, path: &[&str]) -> Option<i64> {
        let mut cursor = config;
        for segment in path {
            cursor = cursor.get(*segment)?;
        }
        cursor.as_i64()
    }

    fn positive_config_i64(config: &Value, path: &[&str]) -> Option<i64> {
        Self::config_i64(config, path).filter(|value| *value > 0)
    }

    fn json_string(value: &Value, key: &str) -> Option<String> {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    }

    fn codex_auth_string(value: &Value, key: &str) -> Option<String> {
        value
            .get("tokens")
            .and_then(Value::as_object)
            .and_then(|tokens| tokens.get(key))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(ToString::to_string)
            .or_else(|| Self::json_string(value, key))
    }

    fn codex_auth_i64(value: &Value, key: &str) -> Option<i64> {
        value
            .get("tokens")
            .and_then(Value::as_object)
            .and_then(|tokens| tokens.get(key))
            .and_then(Value::as_i64)
            .or_else(|| value.get(key).and_then(Value::as_i64))
            .filter(|entry| *entry > 0)
    }

    fn decode_jwt_payload(token: &str) -> Option<Value> {
        let payload = token.split('.').nth(1)?;
        let decoded = URL_SAFE_NO_PAD.decode(payload.as_bytes()).ok()?;
        serde_json::from_slice::<Value>(&decoded).ok()
    }

    fn jwt_exp(token: &str) -> Option<i64> {
        Self::decode_jwt_payload(token)?
            .get("exp")
            .and_then(Value::as_i64)
    }

    fn jwt_account_id(token: &str) -> Option<String> {
        let payload = Self::decode_jwt_payload(token)?;
        let auth = payload.get("https://api.openai.com/auth")?;
        auth.get("chatgpt_account_id")
            .and_then(Value::as_str)
            .or_else(|| auth.get("chatgpt_account_user_id").and_then(Value::as_str))
            .or_else(|| auth.get("chatgpt_user_id").and_then(Value::as_str))
            .or_else(|| auth.get("user_id").and_then(Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    }

    fn codex_auth_path_from_home(home: &Path) -> PathBuf {
        home.join(".codex").join("auth.json")
    }

    fn codex_auth_path() -> Option<PathBuf> {
        let home = std::env::var("HOME").ok()?;
        if home.trim().is_empty() {
            return None;
        }
        Some(Self::codex_auth_path_from_home(Path::new(&home)))
    }

    fn codex_auth_path_from_config(config: &Value) -> Option<PathBuf> {
        Self::config_string(config, &["oauth", "auth_path"]).map(PathBuf::from)
    }

    fn parse_codex_auth_json(
        contents: &str,
        source: CodexAuthSource,
    ) -> Result<CodexAuthState, String> {
        let doc = serde_json::from_str::<Value>(contents)
            .map_err(|err| format!("Failed to parse Codex auth.json: {}", err))?;
        let access_token = Self::codex_auth_string(&doc, "access_token")
            .ok_or_else(|| "Codex auth.json is missing access_token".to_string())?
            .to_string();
        let refresh_token = Self::codex_auth_string(&doc, "refresh_token")
            .ok_or_else(|| "Codex auth.json is missing refresh_token".to_string())?
            .to_string();
        let id_token = Self::codex_auth_string(&doc, "id_token");
        let account_id = Self::codex_auth_string(&doc, "account_id")
            .or_else(|| Self::jwt_account_id(&access_token));
        let expires_at = Self::json_string(&doc, "expires_at")
            .and_then(|value| value.parse::<i64>().ok())
            .filter(|value| *value > 0)
            .or_else(|| Self::codex_auth_i64(&doc, "expires_at"))
            .or_else(|| Self::jwt_exp(&access_token));
        let last_refresh = doc
            .get("last_refresh")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);

        Ok(CodexAuthState {
            access_token,
            refresh_token,
            id_token,
            account_id,
            expires_at,
            last_refresh,
            source,
        })
    }

    fn load_codex_auth_from_path(path: &Path) -> Result<CodexAuthState, String> {
        let contents = std::fs::read_to_string(path)
            .map_err(|err| format!("Failed to read '{}': {}", path.display(), err))?;
        Self::parse_codex_auth_json(&contents, CodexAuthSource::CodexCli(path.to_path_buf()))
    }

    fn build_codex_auth_from_config(config: &Value) -> Option<CodexAuthState> {
        let access_token = Self::config_string(config, &["oauth", "access_token"])?.to_string();
        let refresh_token = Self::config_string(config, &["oauth", "refresh_token"])
            .unwrap_or("")
            .to_string();
        let id_token = Self::config_string(config, &["oauth", "id_token"]).map(ToString::to_string);
        let account_id = Self::config_string(config, &["oauth", "account_id"])
            .map(ToString::to_string)
            .or_else(|| Self::jwt_account_id(&access_token));
        // The default config template uses `expires_at=0` as a placeholder.
        // Treat non-positive values as "missing" so a valid JWT-backed
        // Codex session does not get forced into an unnecessary refresh
        // after reconnects or auth-file fallback.
        let expires_at = Self::positive_config_i64(config, &["oauth", "expires_at"])
            .or_else(|| Self::jwt_exp(&access_token));
        Some(CodexAuthState {
            access_token,
            refresh_token,
            id_token,
            account_id,
            expires_at,
            last_refresh: None,
            source: CodexAuthSource::Config,
        })
    }

    fn should_use_codex_cli_source(config: &Value) -> bool {
        let source = Self::config_string(config, &["oauth", "source"]).unwrap_or("codex_cli");
        source.eq_ignore_ascii_case("codex_cli")
            || source.eq_ignore_ascii_case("codex")
            || source.eq_ignore_ascii_case("external")
    }

    fn url_encode_component(value: &str) -> String {
        let mut encoded = String::with_capacity(value.len());
        for byte in value.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    encoded.push(byte as char)
                }
                b' ' => encoded.push('+'),
                _ => encoded.push_str(&format!("%{:02X}", byte)),
            }
        }
        encoded
    }

    fn should_refresh_codex_auth(state: &CodexAuthState) -> bool {
        let Some(expires_at) = state.expires_at else {
            return true;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0);
        expires_at <= now + OPENAI_CODEX_REFRESH_SKEW_SECS
    }

    fn save_codex_auth_state(path: &Path, state: &CodexAuthState) -> Result<(), String> {
        let mut root =
            if path.exists() {
                serde_json::from_str::<Value>(&std::fs::read_to_string(path).map_err(|err| {
                    format!("Failed to read existing '{}': {}", path.display(), err)
                })?)
                .unwrap_or_else(|_| json!({}))
            } else {
                json!({})
            };

        if !root.is_object() {
            root = json!({});
        }

        root["auth_mode"] = json!("chatgpt");
        root["last_refresh"] = json!(state.last_refresh.clone().unwrap_or_else(Self::utc_now_iso));

        if !root
            .get("tokens")
            .map(|value| value.is_object())
            .unwrap_or(false)
        {
            root["tokens"] = json!({});
        }

        root["tokens"]["access_token"] = json!(&state.access_token);
        root["tokens"]["refresh_token"] = json!(&state.refresh_token);
        if let Some(id_token) = &state.id_token {
            root["tokens"]["id_token"] = json!(id_token);
        }
        if let Some(account_id) = &state.account_id {
            root["tokens"]["account_id"] = json!(account_id);
        }
        if let Some(expires_at) = state.expires_at.filter(|value| *value > 0) {
            // Keep an explicit expiry hint beside the JWT so future
            // reconnect/import paths do not fall back to placeholder
            // values when the auth file is the only surviving source.
            root["expires_at"] = json!(expires_at);
            root["tokens"]["expires_at"] = json!(expires_at);
        }

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("Failed to create '{}': {}", parent.display(), err))?;
        }

        std::fs::write(
            path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(&root)
                    .map_err(|err| format!("Failed to serialize Codex auth.json: {}", err))?
            ),
        )
        .map_err(|err| format!("Failed to write '{}': {}", path.display(), err))
    }

    fn utc_now_iso() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0);
        format!("{}", secs)
    }

    async fn refresh_codex_auth(state: &CodexAuthState) -> Result<CodexAuthState, String> {
        if state.refresh_token.trim().is_empty() {
            return Err("OpenAI Codex OAuth refresh token is missing".into());
        }

        let body = format!(
            "grant_type=refresh_token&refresh_token={}&client_id={}",
            Self::url_encode_component(&state.refresh_token),
            Self::url_encode_component(OPENAI_CODEX_CLIENT_ID),
        );
        let response = http_client::http_post_with_content_type(
            OPENAI_CODEX_OAUTH_ENDPOINT,
            &[],
            &body,
            "application/x-www-form-urlencoded",
            1,
            30,
        )
        .await;

        if !response.success {
            let body_json = serde_json::from_str::<Value>(&response.body).unwrap_or(Value::Null);
            let code = body_json
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown_error");
            let desc = body_json
                .get("error_description")
                .and_then(Value::as_str)
                .or_else(|| body_json.get("message").and_then(Value::as_str))
                .unwrap_or("OAuth token exchange failed");
            return Err(format!("{}: {}", code, desc));
        }

        let doc = serde_json::from_str::<Value>(&response.body)
            .map_err(|err| format!("Failed to parse OAuth refresh response: {}", err))?;
        let access_token = Self::json_string(&doc, "access_token")
            .ok_or_else(|| "OAuth refresh response is missing access_token".to_string())?;
        let refresh_token =
            Self::json_string(&doc, "refresh_token").unwrap_or_else(|| state.refresh_token.clone());
        let id_token = Self::json_string(&doc, "id_token").or_else(|| state.id_token.clone());
        let account_id = Self::json_string(&doc, "account_id")
            .or_else(|| Self::jwt_account_id(&access_token))
            .or_else(|| state.account_id.clone());
        let expires_at = doc
            .get("expires_in")
            .and_then(Value::as_i64)
            .map(|expires_in| {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_secs() as i64)
                    .unwrap_or(0);
                now + expires_in
            })
            .or_else(|| Self::jwt_exp(&access_token));

        Ok(CodexAuthState {
            access_token,
            refresh_token,
            id_token,
            account_id,
            expires_at,
            last_refresh: Some(Self::utc_now_iso()),
            source: state.source.clone(),
        })
    }

    async fn resolved_codex_auth(&self) -> Result<CodexAuthState, String> {
        let mut current = self
            .codex_auth
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clone()
            .ok_or_else(|| "OpenAI Codex OAuth credentials are not configured".to_string())?;

        if let CodexAuthSource::CodexCli(path) = &current.source {
            if let Ok(fresh) = Self::load_codex_auth_from_path(path) {
                if fresh != current {
                    current = fresh.clone();
                    *self
                        .codex_auth
                        .lock()
                        .unwrap_or_else(|err| err.into_inner()) = Some(fresh);
                }
            }
        }

        if Self::should_refresh_codex_auth(&current) {
            let refreshed = Self::refresh_codex_auth(&current).await?;
            if let CodexAuthSource::CodexCli(path) = &refreshed.source {
                Self::save_codex_auth_state(path, &refreshed)?;
            }
            *self
                .codex_auth
                .lock()
                .unwrap_or_else(|err| err.into_inner()) = Some(refreshed.clone());
            current = refreshed;
        }

        Ok(current)
    }

    async fn auth_headers(&self) -> Result<Vec<(String, String)>, String> {
        if self.provider_name == "openai-codex" {
            let state = self.resolved_codex_auth().await?;
            let mut headers = vec![(
                "Authorization".to_string(),
                format!("Bearer {}", state.access_token),
            )];
            if let Some(account_id) = state.account_id {
                headers.push(("ChatGPT-Account-Id".to_string(), account_id));
            }
            return Ok(headers);
        }

        if self.api_key.trim().is_empty() {
            return Err(format!("Backend '{}' has no API key", self.provider_name));
        }
        Ok(vec![(
            "Authorization".to_string(),
            format!("Bearer {}", self.api_key),
        )])
    }

    fn build_chat_completions_messages(
        &self,
        messages: &[LlmMessage],
        tools: &[LlmToolDecl],
        system_prompt: &str,
    ) -> Vec<Value> {
        let mut valid_tools = std::collections::HashSet::new();
        for tool in tools {
            valid_tools.insert(tool.name.clone());
        }

        let mut msgs = Vec::new();
        if !system_prompt.is_empty() {
            msgs.push(json!({"role": "system", "content": system_prompt}));
        }

        for msg in messages {
            let text = Self::trimmed_text(&msg.text);
            let mut is_downgraded = false;
            if msg.role == "tool"
                && Self::resolve_responses_tool_name(msg.tool_name.as_str(), &valid_tools).is_none()
            {
                is_downgraded = true;
            }
            if !msg.tool_calls.is_empty()
                && msg
                    .tool_calls
                    .iter()
                    .any(|tool_call| {
                        Self::resolve_responses_tool_name(tool_call.name.as_str(), &valid_tools)
                            .is_none()
                    })
            {
                is_downgraded = true;
            }

            if is_downgraded {
                if msg.role == "tool" {
                    msgs.push(json!({
                        "role": "user",
                        "content": format!(
                            "[Historical Tool Result for '{}']: {}",
                            msg.tool_name,
                            msg.tool_result
                        )
                    }));
                } else if !msg.tool_calls.is_empty() {
                    let calls_text = msg
                        .tool_calls
                        .iter()
                        .map(|tool_call| {
                            format!(
                                "Called tool '{}' with args '{}'",
                                tool_call.name, tool_call.args
                            )
                        })
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
                msgs.push(json!({
                    "role": "tool",
                    "content": msg.tool_result.to_string(),
                    "tool_call_id": msg.tool_call_id
                }));
            } else if !msg.tool_calls.is_empty() {
                let tool_calls = msg
                    .tool_calls
                    .iter()
                    .map(|tool_call| {
                        json!({
                            "id": tool_call.id,
                            "type": "function",
                            "function": {
                                "name": tool_call.name,
                                "arguments": tool_call.args.to_string()
                            }
                        })
                    })
                    .collect::<Vec<_>>();
                let mut assistant_message = json!({
                    "role": "assistant",
                    "tool_calls": tool_calls
                });
                if !text.is_empty() {
                    assistant_message["content"] = json!(text);
                }
                msgs.push(assistant_message);
            } else if !text.is_empty() {
                msgs.push(json!({"role": msg.role, "content": text}));
            }
        }

        msgs
    }

    fn responses_text_part_type(role: &str) -> &'static str {
        match role {
            "assistant" => "output_text",
            _ => "input_text",
        }
    }

    fn responses_message(role: &str, text: &str) -> Value {
        json!({
            "type": "message",
            "role": role,
            "content": [
                {
                    "type": Self::responses_text_part_type(role),
                    "text": text
                }
            ]
        })
    }

    fn build_responses_input(&self, messages: &[LlmMessage], tools: &[LlmToolDecl]) -> Vec<Value> {
        let mut valid_tools = std::collections::HashSet::new();
        for tool in tools {
            valid_tools.insert(tool.name.clone());
        }

        let mut input = Vec::new();

        for msg in messages {
            let text = Self::trimmed_text(&msg.text);
            let mut is_downgraded = false;
            if msg.role == "tool"
                && Self::resolve_responses_tool_name(msg.tool_name.as_str(), &valid_tools).is_none()
            {
                is_downgraded = true;
            }
            if !msg.tool_calls.is_empty()
                && msg
                    .tool_calls
                    .iter()
                    .any(|tool_call| {
                        Self::resolve_responses_tool_name(tool_call.name.as_str(), &valid_tools)
                            .is_none()
                    })
            {
                is_downgraded = true;
            }

            if is_downgraded {
                if msg.role == "tool" {
                    input.push(Self::responses_message(
                        "user",
                        &format!(
                            "[Historical Tool Result for '{}']: {}",
                            msg.tool_name, msg.tool_result
                        ),
                    ));
                } else if !msg.tool_calls.is_empty() {
                    let calls_text = msg
                        .tool_calls
                        .iter()
                        .map(|tool_call| {
                            format!(
                                "Called tool '{}' with args '{}'",
                                tool_call.name, tool_call.args
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    let full_text = if text.is_empty() {
                        calls_text
                    } else {
                        format!("{}\n\n{}", text, calls_text)
                    };
                    input.push(Self::responses_message("assistant", &full_text));
                } else if !text.is_empty() {
                    input.push(Self::responses_message(&msg.role, &text));
                }
                continue;
            }

            match msg.role.as_str() {
                "tool" => {
                    let output = msg
                        .tool_result
                        .as_str()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| msg.tool_result.to_string());
                    input.push(json!({
                        "type": "function_call_output",
                        "call_id": msg.tool_call_id,
                        "output": output
                    }));
                }
                "assistant" if !msg.tool_calls.is_empty() => {
                    if !text.is_empty() {
                        input.push(Self::responses_message("assistant", &text));
                    }
                    for tool_call in &msg.tool_calls {
                        let resolved_name = Self::resolve_responses_tool_name(
                            tool_call.name.as_str(),
                            &valid_tools,
                        )
                        .unwrap_or(tool_call.name.as_str());
                        input.push(json!({
                            "type": "function_call",
                            "call_id": tool_call.id,
                            "name": resolved_name,
                            "arguments": tool_call.args.to_string()
                        }));
                    }
                }
                _ => {
                    if !text.is_empty() {
                        input.push(Self::responses_message(&msg.role, &text));
                    }
                }
            }
        }

        input
    }

    fn build_codex_input_and_instructions(
        &self,
        messages: &[LlmMessage],
        tools: &[LlmToolDecl],
        system_prompt: &str,
    ) -> (Vec<Value>, String) {
        let mut filtered_messages = Vec::with_capacity(messages.len());
        let mut instruction_parts = Vec::new();

        if !system_prompt.trim().is_empty() {
            instruction_parts.push(system_prompt.trim().to_string());
        }

        for msg in messages {
            if msg.role == "system" {
                let text = Self::trimmed_text(&msg.text);
                if !text.is_empty() {
                    instruction_parts.push(text);
                }
                continue;
            }
            filtered_messages.push(msg.clone());
        }

        let instructions = if instruction_parts.is_empty() {
            OPENAI_CODEX_DEFAULT_INSTRUCTIONS.to_string()
        } else {
            instruction_parts.join("\n\n")
        };

        (
            self.build_responses_input(&filtered_messages, tools),
            instructions,
        )
    }

    fn build_codex_responses_request(
        &self,
        messages: &[LlmMessage],
        tools: &[LlmToolDecl],
        system_prompt: &str,
    ) -> Value {
        let (input, instructions) =
            self.build_codex_input_and_instructions(messages, tools, system_prompt);
        let mut req = json!({
            "model": self.model,
            "instructions": instructions,
            "store": false,
            "stream": true,
            "input": input
        });
        if !tools.is_empty() {
            req["tools"] = Value::Array(
                tools
                    .iter()
                    .map(|tool| {
                        json!({
                            "type": "function",
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.parameters
                        })
                    })
                    .collect(),
            );
        }
        req
    }

    fn append_output_text_from_content(response: &mut LlmResponse, content: &Value) {
        let Some(items) = content.as_array() else {
            return;
        };
        for item in items {
            match item.get("type").and_then(Value::as_str).unwrap_or("") {
                "output_text" | "text" | "input_text" => {
                    if let Some(text) = item
                        .get("text")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                    {
                        if !response.text.is_empty() {
                            response.text.push('\n');
                        }
                        response.text.push_str(text);
                    }
                }
                _ => {}
            }
        }
    }

    fn append_reasoning_summary(response: &mut LlmResponse, item: &Value) {
        let Some(entries) = item.get("summary").and_then(Value::as_array) else {
            return;
        };
        for entry in entries {
            if let Some(text) = entry
                .get("text")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                if !response.reasoning_text.is_empty() {
                    response.reasoning_text.push('\n');
                }
                response.reasoning_text.push_str(text);
            }
        }
    }

    fn append_output_item(response: &mut LlmResponse, item: &Value) {
        match item.get("type").and_then(Value::as_str).unwrap_or("") {
            "message" => {
                if response.text.trim().is_empty() {
                    Self::append_output_text_from_content(response, &item["content"]);
                }
            }
            "function_call" => {
                let args_str = item
                    .get("arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("{}");
                let call_id = item
                    .get("call_id")
                    .and_then(Value::as_str)
                    .or_else(|| item.get("id").and_then(Value::as_str))
                    .unwrap_or("");
                response.tool_calls.push(LlmToolCall {
                    id: call_id.to_string(),
                    name: item
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    args: serde_json::from_str(args_str).unwrap_or_else(|_| json!({})),
                });
            }
            "reasoning" => Self::append_reasoning_summary(response, item),
            _ => {}
        }
    }

    fn apply_responses_usage(response: &mut LlmResponse, usage: &Value) {
        response.prompt_tokens = usage["input_tokens"].as_i64().unwrap_or(0) as i32;
        response.completion_tokens = usage["output_tokens"].as_i64().unwrap_or(0) as i32;
        response.total_tokens = usage["total_tokens"].as_i64().unwrap_or(0) as i32;
        response.cache_read_input_tokens = usage
            .get("input_tokens_details")
            .and_then(|details| details.get("cached_tokens"))
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32;
    }

    fn apply_codex_stream_event(
        response: &mut LlmResponse,
        event_type: &str,
        data: &str,
    ) -> Result<(), String> {
        if data.trim().is_empty() || data.trim() == "[DONE]" {
            return Ok(());
        }
        let json = serde_json::from_str::<Value>(data)
            .map_err(|err| format!("Failed to parse Codex SSE event '{}': {}", event_type, err))?;

        match event_type {
            "response.output_text.delta" => {
                if let Some(delta) = json.get("delta").and_then(Value::as_str) {
                    response.text.push_str(delta);
                }
            }
            "response.output_item.done" => {
                if let Some(item) = json.get("item") {
                    Self::append_output_item(response, item);
                }
            }
            "response.completed" => {
                if let Some(usage) = json.get("response").and_then(|value| value.get("usage")) {
                    Self::apply_responses_usage(response, usage);
                }
                response.success = true;
            }
            "response.failed" => {
                if let Some(message) = json
                    .get("response")
                    .and_then(|value| value.get("error"))
                    .and_then(|value| value.get("message"))
                    .and_then(Value::as_str)
                {
                    response.error_message = message.to_string();
                }
            }
            _ => {}
        }

        Ok(())
    }

    async fn chat_codex_responses(
        &self,
        messages: &[LlmMessage],
        tools: &[LlmToolDecl],
        system_prompt: &str,
    ) -> LlmResponse {
        let url = format!("{}{}", self.endpoint.trim_end_matches('/'), self.api_path);
        let auth_headers = match self.auth_headers().await {
            Ok(headers) => headers,
            Err(err) => {
                let mut response = LlmResponse::default();
                response.error_message = err;
                return response;
            }
        };
        let request = self.build_codex_responses_request(messages, tools, system_prompt);

        let client = http_client::default_client();
        let mut req = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .header("User-Agent", OPENAI_CODEX_USER_AGENT);
        for (key, value) in &auth_headers {
            req = req.header(key, value);
        }

        let http_response = match req.body(request.to_string()).send().await {
            Ok(resp) => resp,
            Err(err) => {
                let mut response = LlmResponse::default();
                response.error_message = format!("Connection Failed: {}", err);
                return response;
            }
        };

        let status = http_response.status();
        let mut response = LlmResponse {
            http_status: status.as_u16(),
            ..LlmResponse::default()
        };

        if !status.is_success() {
            let body = http_response.text().await.unwrap_or_default();
            response.error_message = format!("HTTP {}", status.as_u16());
            if let Ok(error_json) = serde_json::from_str::<Value>(&body) {
                if let Some(message) = error_json.get("detail").and_then(Value::as_str) {
                    response.error_message = message.to_string();
                } else if let Some(message) = error_json
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                {
                    response.error_message = message.to_string();
                } else if let Some(message) = error_json.get("message").and_then(Value::as_str) {
                    response.error_message = message.to_string();
                }
            }
            return response;
        }

        let mut event_type = String::new();
        let mut data_lines: Vec<String> = Vec::new();
        let mut pending = String::new();
        let mut stream = http_response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(err) => {
                    response.error_message = format!("Failed to read Codex SSE stream: {}", err);
                    return response;
                }
            };
            pending.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(newline_idx) = pending.find('\n') {
                let mut line = pending.drain(..=newline_idx).collect::<String>();
                if line.ends_with('\n') {
                    line.pop();
                }
                if line.ends_with('\r') {
                    line.pop();
                }

                if line.is_empty() {
                    let joined = data_lines.join("\n");
                    if !event_type.is_empty() || !joined.is_empty() {
                        if let Err(err) = Self::apply_codex_stream_event(
                            &mut response,
                            &event_type,
                            &joined,
                        ) {
                            response.error_message = err;
                            return response;
                        }
                    }
                    event_type.clear();
                    data_lines.clear();
                    continue;
                }

                if let Some(value) = line.strip_prefix("event:") {
                    event_type = value.trim().to_string();
                } else if let Some(value) = line.strip_prefix("data:") {
                    data_lines.push(value.trim_start().to_string());
                }
            }
        }

        if !pending.trim().is_empty() {
            if let Some(value) = pending.strip_prefix("data:") {
                data_lines.push(value.trim_start().to_string());
            }
        }
        if !event_type.is_empty() || !data_lines.is_empty() {
            let joined = data_lines.join("\n");
            if let Err(err) = Self::apply_codex_stream_event(&mut response, &event_type, &joined)
            {
                response.error_message = err;
                return response;
            }
        }

        if response.success && response.error_message.is_empty() {
            return response;
        }
        if response.text.is_empty() && response.tool_calls.is_empty() && response.error_message.is_empty() {
            response.error_message = "Codex stream completed without a final response".to_string();
        }
        response
    }

    fn parse_chat_completions_response(body: &str) -> LlmResponse {
        let mut response = LlmResponse::default();
        if let Ok(json) = serde_json::from_str::<Value>(body) {
            if let Some(message) = json.pointer("/choices/0/message") {
                response.text = message["content"].as_str().unwrap_or("").into();
                if let Some(tool_calls) = message["tool_calls"].as_array() {
                    for tool_call in tool_calls {
                        let args_str = tool_call["function"]["arguments"].as_str().unwrap_or("{}");
                        response.tool_calls.push(LlmToolCall {
                            id: tool_call["id"].as_str().unwrap_or("").into(),
                            name: tool_call["function"]["name"].as_str().unwrap_or("").into(),
                            args: serde_json::from_str(args_str).unwrap_or(json!({})),
                        });
                    }
                }
            }
            if let Some(usage) = json.get("usage") {
                response.prompt_tokens = usage["prompt_tokens"].as_i64().unwrap_or(0) as i32;
                response.completion_tokens =
                    usage["completion_tokens"].as_i64().unwrap_or(0) as i32;
                response.total_tokens = usage["total_tokens"].as_i64().unwrap_or(0) as i32;
            }
            response.success = true;
        }
        response
    }

    fn parse_responses_response(body: &str) -> LlmResponse {
        let mut response = LlmResponse::default();
        let Ok(json) = serde_json::from_str::<Value>(body) else {
            return response;
        };

        if let Some(output_text) = json.get("output_text").and_then(Value::as_str) {
            response.text = output_text.to_string();
        }

        if let Some(output) = json.get("output").and_then(Value::as_array) {
            for item in output {
                match item.get("type").and_then(Value::as_str).unwrap_or("") {
                    "message" => {
                        if let Some(content) = item.get("content").and_then(Value::as_array) {
                            for content_item in content {
                                match content_item
                                    .get("type")
                                    .and_then(Value::as_str)
                                    .unwrap_or("")
                                {
                                    "output_text" | "text" | "input_text" => {
                                        if let Some(text) = content_item
                                            .get("text")
                                            .and_then(Value::as_str)
                                            .map(str::trim)
                                            .filter(|value| !value.is_empty())
                                        {
                                            if !response.text.is_empty() {
                                                response.text.push('\n');
                                            }
                                            response.text.push_str(text);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    "function_call" => {
                        let args_str = item
                            .get("arguments")
                            .and_then(Value::as_str)
                            .unwrap_or("{}");
                        let call_id = item
                            .get("call_id")
                            .and_then(Value::as_str)
                            .or_else(|| item.get("id").and_then(Value::as_str))
                            .unwrap_or("");
                        response.tool_calls.push(LlmToolCall {
                            id: call_id.to_string(),
                            name: item
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string(),
                            args: serde_json::from_str(args_str).unwrap_or_else(|_| json!({})),
                        });
                    }
                    "reasoning" => {
                        if let Some(summary) = item.get("summary").and_then(Value::as_array) {
                            for entry in summary {
                                if let Some(text) = entry.get("text").and_then(Value::as_str) {
                                    if !response.reasoning_text.is_empty() {
                                        response.reasoning_text.push('\n');
                                    }
                                    response.reasoning_text.push_str(text);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if let Some(usage) = json.get("usage") {
            Self::apply_responses_usage(&mut response, usage);
        }

        response.success = true;
        response
    }
}

#[async_trait::async_trait]
impl LlmBackend for OpenAiBackend {
    fn initialize(&mut self, config: &Value) -> bool {
        if let Some(model) = Self::config_string(config, &["model"]) {
            self.model = model.into();
        }
        if let Some(endpoint) = Self::config_string(config, &["endpoint"]) {
            self.endpoint = endpoint.into();
        }
        if let Some(transport) = Self::config_string(config, &["transport"]) {
            self.transport = if transport.eq_ignore_ascii_case("responses") {
                OpenAiTransport::Responses
            } else {
                OpenAiTransport::ChatCompletions
            };
        }

        if self.provider_name == "openai-codex" {
            if let Some(path) = Self::config_string(config, &["api_path"]) {
                let normalized = if path.starts_with('/') {
                    path.to_string()
                } else {
                    format!("/{}", path)
                };
                self.api_path = if normalized == OPENAI_RESPONSES_PATH {
                    OPENAI_CODEX_RESPONSES_PATH.to_string()
                } else {
                    normalized
                };
            } else {
                self.api_path = OPENAI_CODEX_RESPONSES_PATH.to_string();
            }
            self.service_tier = None;
            let config_state = Self::build_codex_auth_from_config(config);

            if Self::should_use_codex_cli_source(config) {
                if let Some(path) =
                    Self::codex_auth_path_from_config(config).or_else(Self::codex_auth_path)
                {
                    match Self::load_codex_auth_from_path(&path) {
                        Ok(state) => {
                            *self
                                .codex_auth
                                .lock()
                                .unwrap_or_else(|err| err.into_inner()) = Some(state);
                            return true;
                        }
                        Err(err) => {
                            log::warn!(
                                "Failed to import Codex CLI auth for '{}': {}",
                                self.provider_name,
                                err
                            );
                        }
                    }
                }
            }

            if let Some(state) = config_state {
                *self
                    .codex_auth
                    .lock()
                    .unwrap_or_else(|err| err.into_inner()) = Some(state);
                return true;
            }

            log::warn!(
                "Skipping backend '{}' because no ChatGPT OAuth credentials were found",
                self.provider_name
            );
            return false;
        }

        if let Some(path) = Self::config_string(config, &["api_path"]) {
            self.api_path = if path.starts_with('/') {
                path.into()
            } else {
                format!("/{}", path)
            };
        }
        if let Some(service_tier) = Self::config_string(config, &["service_tier"]) {
            self.service_tier = Some(service_tier.to_string());
        }

        if let Some(key) = Self::config_string(config, &["api_key"]) {
            self.api_key = key.into();
        } else if let Some(key) = Self::config_string(config, &["oauth", "access_token"]) {
            self.api_key = key.into();
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
        if self.provider_name == "openai-codex" && matches!(self.transport, OpenAiTransport::Responses)
        {
            return self
                .chat_codex_responses(messages, tools, system_prompt)
                .await;
        }

        let mut request = match self.transport {
            OpenAiTransport::ChatCompletions => {
                let mut req = json!({
                    "model": self.model,
                    "messages": self.build_chat_completions_messages(messages, tools, system_prompt)
                });
                if let Some(tokens) = max_tokens {
                    req["max_tokens"] = json!(tokens);
                }
                if !tools.is_empty() {
                    req["tools"] = Value::Array(
                        tools
                            .iter()
                            .map(|tool| {
                                json!({
                                    "type": "function",
                                    "function": {
                                        "name": tool.name,
                                        "description": tool.description,
                                        "parameters": tool.parameters
                                    }
                                })
                            })
                            .collect(),
                    );
                }
                req
            }
            OpenAiTransport::Responses => {
                let mut req = json!({
                    "model": self.model,
                    "input": self.build_responses_input(messages, tools)
                });
                if let Some(tokens) = max_tokens {
                    req["max_output_tokens"] = json!(tokens);
                }
                if !system_prompt.trim().is_empty() {
                    req["instructions"] = json!(system_prompt);
                }
                if let Some(service_tier) = &self.service_tier {
                    req["service_tier"] = json!(service_tier);
                }
                if !tools.is_empty() {
                    req["tools"] = Value::Array(
                        tools
                            .iter()
                            .map(|tool| {
                                json!({
                                    "type": "function",
                                    "name": tool.name,
                                    "description": tool.description,
                                    "parameters": tool.parameters
                                })
                            })
                            .collect(),
                    );
                }
                req
            }
        };

        let url = format!("{}{}", self.endpoint.trim_end_matches('/'), self.api_path);
        let auth_headers = match self.auth_headers().await {
            Ok(headers) => headers,
            Err(err) => {
                let mut response = LlmResponse::default();
                response.error_message = err;
                return response;
            }
        };
        let header_refs = auth_headers
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect::<Vec<_>>();

        let http_response =
            http_client::http_post(&url, &header_refs, &request.to_string(), 1, 60).await;

        let mut response = LlmResponse::default();
        response.http_status = http_response.status_code;
        if !http_response.success {
            response.error_message = http_response.error;
            if let Ok(error_json) = serde_json::from_str::<Value>(&http_response.body) {
                if let Some(message) = error_json
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                {
                    response.error_message = message.to_string();
                } else if let Some(message) = error_json.get("message").and_then(Value::as_str) {
                    response.error_message = message.to_string();
                }
            }
            return response;
        }

        response = match self.transport {
            OpenAiTransport::ChatCompletions => {
                Self::parse_chat_completions_response(&http_response.body)
            }
            OpenAiTransport::Responses => Self::parse_responses_response(&http_response.body),
        };
        response.http_status = http_response.status_code;
        response
    }

    fn get_name(&self) -> &str {
        &self.provider_name
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CodexAuthSource, OpenAiBackend, OpenAiTransport, CHATGPT_BACKEND_API,
        OPENAI_CODEX_RESPONSES_PATH, OPENAI_RESPONSES_PATH,
    };
    use crate::llm::backend::{LlmBackend, LlmMessage, LlmToolCall, LlmToolDecl};
    use base64::Engine;
    use serde_json::{json, Value};
    use tempfile::tempdir;

    fn create_jwt(payload: Value) -> String {
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"alg":"none","typ":"JWT"}"#);
        let payload =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string().as_bytes());
        format!("{}.{}.sig", header, payload)
    }

    #[test]
    fn openai_codex_accepts_oauth_access_token() {
        let mut backend = OpenAiBackend::new("openai-codex");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let config = json!({
            "oauth": {
                "access_token": create_jwt(json!({
                    "exp": now + 3600,
                    "https://api.openai.com/auth": {
                        "chatgpt_account_id": "acct-123"
                    }
                })),
                "refresh_token": "refresh-token",
                "expires_at": now + 3600
            },
            "transport": "responses",
            "api_path": "/responses"
        });

        assert!(backend.initialize(&config));
    }

    #[test]
    fn openai_codex_imports_codex_cli_auth_json() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let auth_dir = home.join(".codex");
        std::fs::create_dir_all(&auth_dir).unwrap();
        std::fs::write(
            auth_dir.join("auth.json"),
            json!({
                "auth_mode": "chatgpt",
                "tokens": {
                    "access_token": create_jwt(json!({
                        "exp": 4_000_000_000i64,
                        "https://api.openai.com/auth": {
                            "chatgpt_account_id": "acct-xyz"
                        }
                    })),
                    "refresh_token": "refresh-token",
                    "id_token": "id-token",
                    "account_id": "acct-xyz"
                },
                "last_refresh": "2026-04-09T00:00:00Z"
            })
            .to_string(),
        )
        .unwrap();

        let previous_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", home);

        let mut backend = OpenAiBackend::new("openai-codex");
        let initialized = backend.initialize(&json!({}));

        if let Some(home) = previous_home {
            std::env::set_var("HOME", home);
        }

        assert!(initialized);
    }

    #[test]
    fn openai_codex_imports_flat_codex_auth_json() {
        let dir = tempdir().unwrap();
        let home = dir.path();
        let auth_dir = home.join(".codex");
        std::fs::create_dir_all(&auth_dir).unwrap();
        std::fs::write(
            auth_dir.join("auth.json"),
            json!({
                "auth_mode": "chatgpt",
                "access_token": create_jwt(json!({
                    "exp": 4_000_000_000i64,
                    "https://api.openai.com/auth": {
                        "chatgpt_account_id": "acct-flat"
                    }
                })),
                "refresh_token": "refresh-token",
                "id_token": "id-token",
                "last_refresh": "2026-04-11T00:00:00Z"
            })
            .to_string(),
        )
        .unwrap();

        let previous_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", home);

        let mut backend = OpenAiBackend::new("openai-codex");
        let initialized = backend.initialize(&json!({}));
        let account_id = backend
            .codex_auth
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|state| state.account_id.clone());

        if let Some(home) = previous_home {
            std::env::set_var("HOME", home);
        }

        assert!(initialized);
        assert_eq!(account_id.as_deref(), Some("acct-flat"));
    }

    #[test]
    fn openai_codex_uses_explicit_auth_path_from_config() {
        let dir = tempdir().unwrap();
        let auth_path = dir.path().join("codex-auth.json");
        std::fs::write(
            &auth_path,
            json!({
                "auth_mode": "chatgpt",
                "tokens": {
                    "access_token": create_jwt(json!({
                        "exp": 4_000_000_000i64,
                        "https://api.openai.com/auth": {
                            "chatgpt_account_id": "acct-path"
                        }
                    })),
                    "refresh_token": "refresh-token",
                    "account_id": "acct-path"
                }
            })
            .to_string(),
        )
        .unwrap();

        let mut backend = OpenAiBackend::new("openai-codex");
        let initialized = backend.initialize(&json!({
            "oauth": {
                "source": "codex_cli",
                "auth_path": auth_path.display().to_string()
            }
        }));

        assert!(initialized);
    }

    #[test]
    fn openai_codex_falls_back_to_config_tokens_when_auth_file_is_unavailable() {
        let mut backend = OpenAiBackend::new("openai-codex");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let initialized = backend.initialize(&json!({
            "oauth": {
                "source": "codex_cli",
                "auth_path": "/tmp/does-not-exist-auth.json",
                "access_token": create_jwt(json!({
                    "exp": now + 3600,
                    "https://api.openai.com/auth": {
                        "chatgpt_account_id": "acct-fallback"
                    }
                })),
                "refresh_token": "refresh-token"
            }
        }));

        assert!(initialized);
    }

    #[test]
    fn openai_codex_ignores_placeholder_config_expiry() {
        let mut backend = OpenAiBackend::new("openai-codex");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let access_token = create_jwt(json!({
            "exp": now + 3600,
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "acct-expiry"
            }
        }));
        let initialized = backend.initialize(&json!({
            "oauth": {
                "source": "config",
                "access_token": access_token,
                "refresh_token": "refresh-token",
                "expires_at": 0
            }
        }));
        let expires_at = backend
            .codex_auth
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|state| state.expires_at);

        assert!(initialized);
        assert_eq!(expires_at, Some(now + 3600));
    }

    #[test]
    fn openai_codex_defaults_to_responses_transport() {
        let backend = OpenAiBackend::new("openai-codex");
        assert_eq!(backend.endpoint, CHATGPT_BACKEND_API);
        assert_eq!(backend.api_path, OPENAI_CODEX_RESPONSES_PATH);
        assert!(matches!(backend.transport, OpenAiTransport::Responses));
    }

    #[test]
    fn openai_codex_request_matches_codex_route_contract() {
        let backend = OpenAiBackend::new("openai-codex");
        let request =
            backend.build_codex_responses_request(&[LlmMessage::user("안녕")], &[], "");

        assert_eq!(request["instructions"], json!("You are a helpful assistant."));
        assert_eq!(request["store"], json!(false));
        assert_eq!(request["stream"], json!(true));
        assert!(request.get("max_output_tokens").is_none());
        assert!(request.get("service_tier").is_none());
    }

    #[test]
    fn openai_codex_moves_system_messages_into_instructions() {
        let backend = OpenAiBackend::new("openai-codex");
        let request = backend.build_codex_responses_request(
            &[
                LlmMessage {
                    role: "system".to_string(),
                    text: "추가 시스템 규칙".to_string(),
                    ..Default::default()
                },
                LlmMessage::user("파일을 정리해줘"),
            ],
            &[],
            "기본 시스템 프롬프트",
        );

        assert_eq!(
            request["instructions"],
            json!("기본 시스템 프롬프트\n\n추가 시스템 규칙")
        );
        let input = request["input"].as_array().unwrap();
        assert_eq!(input.len(), 1);
        assert_eq!(input[0]["role"], json!("user"));
    }

    #[test]
    fn responses_input_uses_output_text_for_assistant_history() {
        let backend = OpenAiBackend::new("openai-codex");
        let input = backend.build_responses_input(
            &[
                LlmMessage::user("질문"),
                LlmMessage::assistant("답변"),
                LlmMessage::user("후속 질문"),
            ],
            &[],
        );

        assert_eq!(input[0]["content"][0]["type"], json!("input_text"));
        assert_eq!(input[1]["content"][0]["type"], json!("output_text"));
        assert_eq!(input[2]["content"][0]["type"], json!("input_text"));
    }

    #[test]
    fn responses_input_preserves_file_manager_alias_history_as_tool_io() {
        let backend = OpenAiBackend::new("openai-codex");
        let input = backend.build_responses_input(
            &[
                LlmMessage {
                    role: "assistant".to_string(),
                    tool_calls: vec![LlmToolCall {
                        id: "call_1".to_string(),
                        name: "file_manager".to_string(),
                        args: json!({"operation": "list", "path": "/tmp"}),
                    }],
                    ..Default::default()
                },
                LlmMessage::tool_result(
                    "call_1",
                    "list_files",
                    json!({"entries": [{"path": "/tmp/demo.txt"}]}),
                ),
            ],
            &[LlmToolDecl {
                name: "file_manager".to_string(),
                description: "Manage files".to_string(),
                parameters: json!({}),
            }],
        );

        assert_eq!(input.len(), 2);
        assert_eq!(input[0]["type"], json!("function_call"));
        assert_eq!(input[0]["name"], json!("file_manager"));
        assert_eq!(input[1]["type"], json!("function_call_output"));
        assert_eq!(input[1]["call_id"], json!("call_1"));
    }

    #[test]
    fn openai_chat_completions_requests_do_not_force_default_max_tokens() {
        let backend = OpenAiBackend::new("openai");
        let request = json!({
            "model": backend.model,
            "messages": backend.build_chat_completions_messages(
                &[LlmMessage::user("질문")],
                &[],
                ""
            )
        });

        assert!(request.get("max_tokens").is_none());
    }

    #[test]
    fn openai_responses_requests_do_not_force_default_output_tokens() {
        let backend = OpenAiBackend::new("openai");
        let mut request = json!({
            "model": backend.model,
            "input": backend.build_responses_input(&[LlmMessage::user("질문")], &[])
        });
        request["instructions"] = json!("시스템");

        assert!(request.get("max_output_tokens").is_none());
    }

    #[test]
    fn parse_responses_response_extracts_text_and_tool_calls() {
        let response = OpenAiBackend::parse_responses_response(
            &json!({
                "output": [
                    {
                        "type": "reasoning",
                        "summary": [
                            { "text": "Need weather info." }
                        ]
                    },
                    {
                        "type": "function_call",
                        "call_id": "call_1",
                        "name": "get_weather",
                        "arguments": "{\"location\":\"Seoul\"}"
                    },
                    {
                        "type": "message",
                        "content": [
                            { "type": "output_text", "text": "It is sunny." }
                        ]
                    }
                ],
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 5,
                    "total_tokens": 15,
                    "input_tokens_details": {
                        "cached_tokens": 3
                    }
                }
            })
            .to_string(),
        );

        assert!(response.success);
        assert_eq!(response.text, "It is sunny.");
        assert_eq!(response.reasoning_text, "Need weather info.");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "call_1");
        assert_eq!(response.tool_calls[0].name, "get_weather");
        assert_eq!(response.prompt_tokens, 10);
        assert_eq!(response.completion_tokens, 5);
        assert_eq!(response.total_tokens, 15);
        assert_eq!(response.cache_read_input_tokens, 3);
    }
}
