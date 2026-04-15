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
    ProviderKind, ResponseFormat, ResponseMetadata, StreamEvent, ToolCallDelta, Usage,
};

#[derive(Clone)]
pub struct OpenAiCompatClient {
    config: ProviderConfig,
    http: Arc<dyn HttpClient>,
}

impl OpenAiCompatClient {
    pub fn new(config: ProviderConfig, http: Arc<dyn HttpClient>) -> Self {
        Self { config, http }
    }

    fn endpoint(&self) -> String {
        format!(
            "{}/v1/chat/completions",
            self.config.api_base.trim_end_matches('/')
        )
    }

    fn build_request(&self, request: &ChatRequest, stream: bool) -> Result<HttpRequest, ApiError> {
        let mut body = json!({
            "model": request.model,
            "messages": request.messages.iter().map(message_to_openai).collect::<Vec<_>>(),
            "stream": stream,
        });
        if let Some(temperature) = request.temperature {
            body["temperature"] = json!(temperature);
        }
        if let Some(max_output_tokens) = request.max_output_tokens {
            body["max_tokens"] = json!(max_output_tokens);
        }
        if !request.tools.is_empty() {
            body["tools"] = Value::Array(
                request
                    .tools
                    .iter()
                    .map(|tool| {
                        json!({
                            "type": "function",
                            "function": {
                                "name": tool.name,
                                "description": tool.description,
                                "parameters": tool.input_schema,
                            }
                        })
                    })
                    .collect(),
            );
        }
        if let Some(system) = &request.system {
            let mut messages = body["messages"].as_array().cloned().unwrap_or_default();
            messages.insert(
                0,
                json!({
                    "role": "system",
                    "content": system,
                }),
            );
            body["messages"] = Value::Array(messages);
        }
        if matches!(request.response_format, Some(ResponseFormat::JsonObject)) {
            body["response_format"] = json!({ "type": "json_object" });
        }
        if let Some(prompt_cache) = &request.prompt_cache {
            let enabled = !matches!(prompt_cache.mode, PromptCacheMode::Disabled);
            body["prompt_cache"] = json!({
                "enabled": enabled,
                "mode": match prompt_cache.mode {
                    PromptCacheMode::Disabled => "disabled",
                    PromptCacheMode::Ephemeral => "ephemeral",
                    PromptCacheMode::Persistent => "persistent",
                },
                "ttl_seconds": prompt_cache.ttl_seconds,
                "breakpoint": prompt_cache.breakpoint,
            });
        }
        if let Some(metadata) = &request.metadata {
            body["metadata"] = metadata.clone();
        }

        let mut http_request =
            HttpRequest::json(HttpMethod::Post, self.endpoint(), body).map_err(ApiError::Http)?;
        http_request.headers.extend(
            self.config
                .default_headers
                .iter()
                .map(|(k, v)| (k.clone(), v.clone())),
        );
        if let Some(api_key) = &self.config.api_key {
            http_request
                .headers
                .insert("authorization".to_string(), format!("Bearer {}", api_key));
        }
        Ok(http_request)
    }

    fn handle_response(&self, response: HttpResponse) -> Result<ChatResponse, ApiError> {
        if response.status >= 400 {
            return Err(ApiError::Status {
                status: response.status,
                body: String::from_utf8_lossy(&response.body).into_owned(),
            });
        }
        let body: OpenAiResponse = ApiError::decode_json(&response)?;
        let choice = body
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| ApiError::InvalidResponse {
                message: "missing openai-compatible choice".to_string(),
            })?;

        Ok(ChatResponse {
            metadata: ResponseMetadata {
                provider: ProviderKind::OpenAiCompat,
                model: body.model,
                id: Some(body.id),
            },
            content: openai_message_to_content(choice.message),
            finish_reason: FinishReason::new(
                choice.finish_reason.unwrap_or_else(|| "stop".to_string()),
            ),
            usage: body.usage.map(openai_usage_to_usage),
            stop_sequence: None,
            raw: None,
        })
    }
}

impl ProviderClient for OpenAiCompatClient {
    fn provider(&self) -> ProviderKind {
        ProviderKind::OpenAiCompat
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
        let mut started = false;

        for event in SseParser::parse(&body)? {
            if event.data == "[DONE]" {
                events_out.push(Ok(StreamEvent::MessageStop {
                    finish_reason: FinishReason::stop(),
                }));
                continue;
            }

            let payload: OpenAiStreamChunk =
                serde_json::from_str(&event.data).map_err(|source| ApiError::Decode {
                    source,
                    body: event.data.clone(),
                })?;

            if !started {
                events_out.push(Ok(StreamEvent::MessageStart {
                    metadata: ResponseMetadata {
                        provider: ProviderKind::OpenAiCompat,
                        model: payload.model.clone(),
                        id: Some(payload.id.clone()),
                    },
                }));
                started = true;
            }

            let usage = payload.usage.map(openai_usage_to_usage);

            for choice in payload.choices {
                if let Some(content) = choice.delta.content {
                    events_out.push(Ok(StreamEvent::ContentBlockDelta {
                        index: 0,
                        delta: ContentDelta::Text { text: content },
                    }));
                }

                for (index, tool_call) in choice.delta.tool_calls.into_iter().enumerate() {
                    let function = tool_call.function.unwrap_or_default();
                    events_out.push(Ok(StreamEvent::ContentBlockDelta {
                        index,
                        delta: ContentDelta::ToolCall {
                            delta: ToolCallDelta {
                                id: tool_call.id,
                                name: function.name,
                                arguments: function.arguments,
                            },
                        },
                    }));
                }

                if let Some(finish_reason) = choice.finish_reason {
                    events_out.push(Ok(StreamEvent::MessageStop {
                        finish_reason: FinishReason::new(finish_reason),
                    }));
                }
            }

            if let Some(usage) = usage {
                events_out.push(Ok(StreamEvent::Usage { usage }));
            }
        }

        Ok(Box::new(events_out.into_iter()))
    }
}

fn message_to_openai(message: &ChatMessage) -> Value {
    let role = match message.role {
        MessageRole::System => "system",
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::Tool => "tool",
    };

    let mut object = json!({
        "role": role,
    });

    match message.role {
        MessageRole::Tool => {
            let (tool_call_id, output) = first_tool_result(&message.content);
            object["tool_call_id"] = Value::String(tool_call_id.unwrap_or_default());
            object["content"] = output.unwrap_or_else(|| Value::String(String::new()));
        }
        _ => {
            let mut content = Vec::new();
            let mut tool_calls = Vec::new();
            for block in &message.content {
                match block {
                    ContentBlock::Text { text } => {
                        content.push(json!({ "type": "text", "text": text }));
                    }
                    ContentBlock::Json { value } => {
                        content.push(json!({ "type": "json", "json": value }));
                    }
                    ContentBlock::ToolCall { id, name, input } => {
                        tool_calls.push(json!({
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": input.to_string(),
                            }
                        }));
                    }
                    ContentBlock::ToolResult { .. } => {}
                }
            }

            if content.len() == 1
                && content
                    .first()
                    .and_then(|part| part.get("type"))
                    .and_then(Value::as_str)
                    == Some("text")
            {
                object["content"] = content
                    .first()
                    .and_then(|part| part.get("text"))
                    .cloned()
                    .unwrap_or(Value::String(String::new()));
            } else if !content.is_empty() {
                object["content"] = Value::Array(content);
            } else {
                object["content"] = Value::Null;
            }

            if !tool_calls.is_empty() {
                object["tool_calls"] = Value::Array(tool_calls);
            }
        }
    }

    object
}

fn first_tool_result(content: &[ContentBlock]) -> (Option<String>, Option<Value>) {
    for block in content {
        if let ContentBlock::ToolResult {
            tool_call_id,
            output,
        } = block
        {
            return (Some(tool_call_id.clone()), Some(output.clone()));
        }
    }
    (None, None)
}

fn openai_message_to_content(message: OpenAiMessage) -> Vec<ContentBlock> {
    let mut content = Vec::new();

    match message.content {
        Some(OpenAiMessageContent::Text(text)) => {
            content.push(ContentBlock::Text { text });
        }
        Some(OpenAiMessageContent::Parts(parts)) => {
            for part in parts {
                match part.kind.as_deref() {
                    Some("text") | None => content.push(ContentBlock::Text {
                        text: part.text.unwrap_or_default(),
                    }),
                    Some("json") => content.push(ContentBlock::Json {
                        value: part.json.unwrap_or(Value::Null),
                    }),
                    _ => {}
                }
            }
        }
        None => {}
    }

    for tool_call in message.tool_calls {
        let function = tool_call.function.unwrap_or_default();
        let input = function
            .arguments
            .and_then(|arguments| serde_json::from_str(&arguments).ok())
            .unwrap_or(Value::Null);
        content.push(ContentBlock::ToolCall {
            id: tool_call.id.unwrap_or_default(),
            name: function.name.unwrap_or_default(),
            input,
        });
    }

    content
}

fn openai_usage_to_usage(usage: OpenAiUsage) -> Usage {
    Usage {
        input_tokens: usage.prompt_tokens,
        output_tokens: usage.completion_tokens,
        total_tokens: usage.total_tokens,
        prompt_cache: Some(PromptCacheUsage {
            cache_creation_input_tokens: usage
                .prompt_tokens_details
                .as_ref()
                .and_then(|details| details.cached_tokens)
                .unwrap_or(0),
            cache_read_input_tokens: usage
                .prompt_tokens_details
                .as_ref()
                .and_then(|details| details.cached_tokens)
                .unwrap_or(0),
            hit: usage
                .prompt_tokens_details
                .as_ref()
                .and_then(|details| details.cached_tokens)
                .unwrap_or(0)
                > 0,
        }),
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    id: String,
    model: String,
    choices: Vec<OpenAiChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiMessage {
    #[serde(default)]
    content: Option<OpenAiMessageContent>,
    #[serde(default)]
    tool_calls: Vec<OpenAiToolCall>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OpenAiMessageContent {
    Text(String),
    Parts(Vec<OpenAiContentPart>),
}

#[derive(Debug, Deserialize)]
struct OpenAiContentPart {
    #[serde(rename = "type")]
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    json: Option<Value>,
}

#[derive(Debug, Deserialize, Default)]
struct OpenAiToolCall {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<OpenAiToolFunction>,
}

#[derive(Debug, Deserialize, Default)]
struct OpenAiToolFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
    #[serde(default)]
    prompt_tokens_details: Option<OpenAiPromptTokenDetails>,
}

#[derive(Debug, Deserialize)]
struct OpenAiPromptTokenDetails {
    #[serde(default)]
    cached_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChunk {
    id: String,
    model: String,
    #[serde(default)]
    choices: Vec<OpenAiStreamChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChoice {
    delta: OpenAiStreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct OpenAiStreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<OpenAiToolCall>,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use crate::client::ProviderClient;
    use crate::error::ApiError;
    use crate::http_client::StaticHttpClient;
    use crate::providers::{OpenAiCompatClient, ProviderConfig};
    use crate::types::{ChatMessage, ChatRequest, ContentDelta, ProviderKind, StreamEvent};

    #[test]
    fn openai_streaming_path_emits_typed_events() {
        let http = Arc::new(StaticHttpClient::new());
        http.push_text_response(
            200,
            concat!(
                "data: {\"id\":\"resp_1\",\"model\":\"gpt-test\",\"choices\":[{\"delta\":{\"content\":\"Hel\"},\"finish_reason\":null}]}\n\n",
                "data: {\"id\":\"resp_1\",\"model\":\"gpt-test\",\"choices\":[{\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":4,\"completion_tokens\":2,\"total_tokens\":6}}\n\n",
                "data: [DONE]\n\n",
            ),
        );

        let client = OpenAiCompatClient::new(ProviderConfig::new("https://example.invalid"), http);
        let request = ChatRequest {
            model: "gpt-test".to_string(),
            system: None,
            messages: vec![ChatMessage::user_text("hello")],
            tools: Vec::new(),
            stream: true,
            temperature: None,
            max_output_tokens: None,
            response_format: None,
            prompt_cache: None,
            metadata: None,
        };

        let events: Vec<_> = client
            .stream(&request)
            .expect("stream should build")
            .collect::<Result<Vec<_>, _>>()
            .expect("stream should decode");

        assert!(matches!(
            &events[0],
            StreamEvent::MessageStart { metadata }
                if metadata.provider == ProviderKind::OpenAiCompat
                && metadata.model == "gpt-test"
        ));
        assert!(matches!(
            &events[1],
            StreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::Text { text }
            } if text == "Hel"
        ));
        assert!(matches!(
            &events[2],
            StreamEvent::ContentBlockDelta {
                index: 0,
                delta: ContentDelta::Text { text }
            } if text == "lo"
        ));
        assert!(events
            .iter()
            .any(|event| matches!(event, StreamEvent::Usage { .. })));
    }

    #[test]
    fn openai_send_reports_decode_errors() {
        let http = Arc::new(StaticHttpClient::new());
        http.push_json_response(200, json!({ "malformed": true }));

        let client = OpenAiCompatClient::new(ProviderConfig::new("https://example.invalid"), http);
        let request = ChatRequest {
            model: "gpt-test".to_string(),
            system: None,
            messages: vec![ChatMessage::user_text("hello")],
            tools: Vec::new(),
            stream: false,
            temperature: None,
            max_output_tokens: None,
            response_format: None,
            prompt_cache: None,
            metadata: None,
        };

        let err = client.send(&request).expect_err("send should fail");
        assert!(matches!(
            err,
            ApiError::Decode { .. } | ApiError::InvalidResponse { .. }
        ));
    }
}
