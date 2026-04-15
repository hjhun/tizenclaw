use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::client::{EventStream, ProviderClient};
use crate::error::ApiError;
use crate::http_client::{HttpClient, HttpMethod, HttpRequest, HttpResponse};
use crate::prompt_cache::{PromptCacheMode, PromptCacheUsage};
use crate::providers::ProviderConfig;
use crate::sse::SseParser;
use crate::types::{
    ChatMessage, ChatRequest, ChatResponse, ContentBlock, ContentDelta, FinishReason, MessageRole,
    ProviderKind, ResponseMetadata, StreamEvent, Usage,
};

#[derive(Clone)]
pub struct AnthropicClient {
    config: ProviderConfig,
    http: Arc<dyn HttpClient>,
}

impl AnthropicClient {
    pub fn new(config: ProviderConfig, http: Arc<dyn HttpClient>) -> Self {
        Self { config, http }
    }

    fn endpoint(&self) -> String {
        format!("{}/v1/messages", self.config.api_base.trim_end_matches('/'))
    }

    fn build_request(&self, request: &ChatRequest, stream: bool) -> Result<HttpRequest, ApiError> {
        let mut body = json!({
            "model": request.model,
            "messages": request.messages.iter().map(message_to_anthropic).collect::<Vec<_>>(),
            "stream": stream,
            "max_tokens": request.max_output_tokens.unwrap_or(1024),
        });

        if let Some(system) = &request.system {
            body["system"] = Value::String(system.clone());
        }
        if let Some(temperature) = request.temperature {
            body["temperature"] = json!(temperature);
        }
        if !request.tools.is_empty() {
            body["tools"] = Value::Array(
                request
                    .tools
                    .iter()
                    .map(|tool| {
                        json!({
                            "name": tool.name,
                            "description": tool.description,
                            "input_schema": tool.input_schema,
                        })
                    })
                    .collect(),
            );
        }
        if let Some(prompt_cache) = &request.prompt_cache {
            let cache_type = match prompt_cache.mode {
                PromptCacheMode::Disabled => "disabled",
                PromptCacheMode::Ephemeral => "ephemeral",
                PromptCacheMode::Persistent => "persistent",
            };
            body["prompt_cache"] = json!({
                "type": cache_type,
                "ttl_seconds": prompt_cache.ttl_seconds,
                "breakpoint": prompt_cache.breakpoint,
            });
        }

        let mut http_request =
            HttpRequest::json(HttpMethod::Post, self.endpoint(), body).map_err(ApiError::Http)?;
        add_headers(
            &mut http_request,
            &self.config,
            "anthropic-version",
            "2023-06-01",
        );
        Ok(http_request)
    }

    fn handle_response(&self, response: HttpResponse) -> Result<ChatResponse, ApiError> {
        if response.status >= 400 {
            return Err(ApiError::Status {
                status: response.status,
                body: String::from_utf8_lossy(&response.body).into_owned(),
            });
        }

        let body: AnthropicMessageResponse = ApiError::decode_json(&response)?;
        Ok(ChatResponse {
            metadata: ResponseMetadata {
                provider: ProviderKind::Anthropic,
                model: body.model,
                id: Some(body.id),
            },
            content: body
                .content
                .into_iter()
                .filter_map(anthropic_block_to_content)
                .collect(),
            finish_reason: FinishReason::new(
                body.stop_reason.unwrap_or_else(|| "stop".to_string()),
            ),
            usage: body.usage.map(anthropic_usage_to_usage),
            stop_sequence: body.stop_sequence,
            raw: None,
        })
    }
}

impl ProviderClient for AnthropicClient {
    fn provider(&self) -> ProviderKind {
        ProviderKind::Anthropic
    }

    fn send(&self, request: &ChatRequest) -> Result<ChatResponse, ApiError> {
        let response = self.http.execute(self.build_request(request, false)?)?;
        self.handle_response(response)
    }

    fn stream(&self, request: &ChatRequest) -> Result<EventStream, ApiError> {
        let response = self.http.execute(self.build_request(request, true)?)?;
        if response.status >= 400 {
            return Err(ApiError::Status {
                status: response.status,
                body: String::from_utf8_lossy(&response.body).into_owned(),
            });
        }

        let body = String::from_utf8_lossy(&response.body);
        let mut events_out = Vec::new();
        for event in SseParser::parse(&body)? {
            if event.data == "[DONE]" {
                events_out.push(Ok(StreamEvent::MessageStop {
                    finish_reason: FinishReason::stop(),
                }));
                continue;
            }

            let payload: Value =
                serde_json::from_str(&event.data).map_err(|source| ApiError::Decode {
                    source,
                    body: event.data.clone(),
                })?;

            match payload.get("type").and_then(Value::as_str) {
                Some("message_start") => {
                    let message = payload.get("message").cloned().ok_or_else(|| {
                        ApiError::InvalidResponse {
                            message: "missing message_start.message".to_string(),
                        }
                    })?;
                    let response: AnthropicMessageStart =
                        serde_json::from_value(message).map_err(|source| ApiError::Decode {
                            source,
                            body: payload.to_string(),
                        })?;
                    events_out.push(Ok(StreamEvent::MessageStart {
                        metadata: ResponseMetadata {
                            provider: ProviderKind::Anthropic,
                            model: response.model,
                            id: Some(response.id),
                        },
                    }));
                }
                Some("content_block_start") => {
                    let index = payload.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                    let block = payload.get("content_block").cloned().ok_or_else(|| {
                        ApiError::InvalidResponse {
                            message: "missing content_block_start.content_block".to_string(),
                        }
                    })?;
                    let content: AnthropicContentBlock =
                        serde_json::from_value(block).map_err(|source| ApiError::Decode {
                            source,
                            body: payload.to_string(),
                        })?;
                    if let Some(mapped) = anthropic_block_to_content(content) {
                        events_out.push(Ok(StreamEvent::ContentBlockStart {
                            index,
                            block: mapped,
                        }));
                    }
                }
                Some("content_block_delta") => {
                    let index = payload.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                    let delta_type = payload
                        .get("delta")
                        .and_then(|delta| delta.get("type"))
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    match delta_type {
                        "text_delta" => {
                            if let Some(text) = payload
                                .get("delta")
                                .and_then(|delta| delta.get("text"))
                                .and_then(Value::as_str)
                            {
                                events_out.push(Ok(StreamEvent::ContentBlockDelta {
                                    index,
                                    delta: ContentDelta::Text {
                                        text: text.to_string(),
                                    },
                                }));
                            }
                        }
                        "input_json_delta" => {
                            if let Some(partial_json) = payload
                                .get("delta")
                                .and_then(|delta| delta.get("partial_json"))
                            {
                                events_out.push(Ok(StreamEvent::ContentBlockDelta {
                                    index,
                                    delta: ContentDelta::Json {
                                        value: partial_json.clone(),
                                    },
                                }));
                            }
                        }
                        _ => {
                            events_out.push(Ok(StreamEvent::RawProviderEvent {
                                provider: ProviderKind::Anthropic,
                                event: event.event.clone(),
                                payload: payload.clone(),
                            }));
                        }
                    }
                }
                Some("content_block_stop") => {
                    let index = payload.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                    events_out.push(Ok(StreamEvent::ContentBlockStop { index }));
                }
                Some("message_delta") => {
                    if let Some(stop_reason) = payload
                        .get("delta")
                        .and_then(|delta| delta.get("stop_reason"))
                        .and_then(Value::as_str)
                    {
                        events_out.push(Ok(StreamEvent::MessageStop {
                            finish_reason: FinishReason::new(stop_reason),
                        }));
                    }
                    if let Some(usage) = payload.get("usage") {
                        let usage: AnthropicUsage =
                            serde_json::from_value(usage.clone()).map_err(|source| {
                                ApiError::Decode {
                                    source,
                                    body: payload.to_string(),
                                }
                            })?;
                        events_out.push(Ok(StreamEvent::Usage {
                            usage: anthropic_usage_to_usage(usage),
                        }));
                    }
                }
                Some("message_stop") => {
                    events_out.push(Ok(StreamEvent::MessageStop {
                        finish_reason: FinishReason::stop(),
                    }));
                }
                _ => {
                    events_out.push(Ok(StreamEvent::RawProviderEvent {
                        provider: ProviderKind::Anthropic,
                        event: event.event.clone(),
                        payload,
                    }));
                }
            }
        }

        Ok(Box::new(events_out.into_iter()))
    }
}

fn add_headers(
    request: &mut HttpRequest,
    config: &ProviderConfig,
    header_name: &str,
    header_value: &str,
) {
    request.headers.extend(
        config
            .default_headers
            .iter()
            .map(|(k, v)| (k.clone(), v.clone())),
    );
    request
        .headers
        .insert(header_name.to_string(), header_value.to_string());
    if let Some(api_key) = &config.api_key {
        request
            .headers
            .insert("x-api-key".to_string(), api_key.clone());
    }
}

fn message_to_anthropic(message: &ChatMessage) -> Value {
    let role = match message.role {
        MessageRole::User | MessageRole::Tool => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::System => "user",
    };
    json!({
        "role": role,
        "content": message.content.iter().map(content_to_anthropic).collect::<Vec<_>>(),
    })
}

fn content_to_anthropic(block: &ContentBlock) -> Value {
    match block {
        ContentBlock::Text { text } => json!({ "type": "text", "text": text }),
        ContentBlock::ToolCall { id, name, input } => {
            json!({ "type": "tool_use", "id": id, "name": name, "input": input })
        }
        ContentBlock::ToolResult {
            tool_call_id,
            output,
        } => {
            json!({ "type": "tool_result", "tool_use_id": tool_call_id, "content": output })
        }
        ContentBlock::Json { value } => json!({ "type": "json", "value": value }),
    }
}

fn anthropic_block_to_content(block: AnthropicContentBlock) -> Option<ContentBlock> {
    match block.kind.as_str() {
        "text" => Some(ContentBlock::Text {
            text: block.text.unwrap_or_default(),
        }),
        "tool_use" => Some(ContentBlock::ToolCall {
            id: block.id.unwrap_or_default(),
            name: block.name.unwrap_or_default(),
            input: block.input.unwrap_or(Value::Null),
        }),
        "tool_result" => Some(ContentBlock::ToolResult {
            tool_call_id: block.tool_use_id.unwrap_or_default(),
            output: block.content.unwrap_or(Value::Null),
        }),
        _ => None,
    }
}

fn anthropic_usage_to_usage(usage: AnthropicUsage) -> Usage {
    let input_tokens = usage.input_tokens.unwrap_or(0);
    let output_tokens = usage.output_tokens.unwrap_or(0);
    Usage {
        input_tokens,
        output_tokens,
        total_tokens: input_tokens + output_tokens,
        prompt_cache: Some(PromptCacheUsage {
            cache_creation_input_tokens: usage.cache_creation_input_tokens.unwrap_or(0),
            cache_read_input_tokens: usage.cache_read_input_tokens.unwrap_or(0),
            hit: usage.cache_read_input_tokens.unwrap_or(0) > 0,
        }),
    }
}

#[derive(Debug, Deserialize)]
struct AnthropicMessageResponse {
    id: String,
    model: String,
    #[serde(default)]
    content: Vec<AnthropicContentBlock>,
    #[serde(default)]
    stop_reason: Option<String>,
    #[serde(default)]
    stop_sequence: Option<String>,
    #[serde(default)]
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessageStart {
    id: String,
    model: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    input: Option<Value>,
    #[serde(default)]
    tool_use_id: Option<String>,
    #[serde(default)]
    content: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: Option<u32>,
    #[serde(default)]
    output_tokens: Option<u32>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u32>,
    #[serde(default)]
    cache_read_input_tokens: Option<u32>,
}
