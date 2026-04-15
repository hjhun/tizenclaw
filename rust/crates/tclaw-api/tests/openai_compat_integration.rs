use std::sync::Arc;

use serde_json::json;
use tclaw_api::{
    ChatMessage, ChatRequest, ContentBlock, ContentDelta, HttpMethod, OpenAiCompatClient,
    PromptCacheConfig, PromptCacheMode, ProviderClient, ProviderConfig, ResponseFormat,
    StaticHttpClient, StreamEvent, ToolDefinition,
};

fn sample_request() -> ChatRequest {
    ChatRequest {
        model: "gpt-4o-mini".to_string(),
        system: Some("act as verifier".to_string()),
        messages: vec![ChatMessage::user_text("hello")],
        tools: vec![ToolDefinition {
            name: "echo".to_string(),
            description: "Echo back".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string" }
                }
            }),
        }],
        stream: false,
        temperature: Some(0.2),
        max_output_tokens: Some(128),
        response_format: Some(ResponseFormat::JsonObject),
        prompt_cache: Some(PromptCacheConfig {
            mode: PromptCacheMode::Ephemeral,
            ttl_seconds: Some(60),
            breakpoint: Some("cli".to_string()),
        }),
        metadata: Some(json!({"suite": "openai_compat_integration"})),
    }
}

#[test]
fn send_builds_the_openai_compatible_request_and_decodes_response() {
    let http = Arc::new(StaticHttpClient::new());
    http.push_json_response(
        200,
        json!({
            "id": "chatcmpl-1",
            "model": "gpt-4o-mini",
            "choices": [
                {
                    "message": {
                        "role": "assistant",
                        "content": "done"
                    },
                    "finish_reason": "stop"
                }
            ],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }),
    );

    let client =
        OpenAiCompatClient::new(ProviderConfig::new("https://example.invalid"), http.clone());
    let response = client.send(&sample_request()).expect("send");
    let requests = http.take_requests();
    let request = requests.first().expect("request");
    let body: serde_json::Value = serde_json::from_slice(&request.body).expect("json");

    assert_eq!(request.method, HttpMethod::Post);
    assert_eq!(request.url, "https://example.invalid/v1/chat/completions");
    assert_eq!(body["response_format"]["type"], "json_object");
    assert_eq!(body["prompt_cache"]["mode"], "ephemeral");
    assert_eq!(body["metadata"]["suite"], "openai_compat_integration");
    assert_eq!(response.metadata.model, "gpt-4o-mini");
    assert_eq!(
        response.content,
        vec![ContentBlock::Text {
            text: "done".to_string()
        }]
    );
}

#[test]
fn stream_emits_typed_deltas_and_usage_events() {
    let http = Arc::new(StaticHttpClient::new());
    http.push_text_response(
        200,
        concat!(
            "data: {\"id\":\"chatcmpl-1\",\"model\":\"gpt-4o-mini\",\"choices\":[{\"delta\":{\"content\":\"hel\"}}]}\n\n",
            "data: {\"id\":\"chatcmpl-1\",\"model\":\"gpt-4o-mini\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"id\":\"call_1\",\"function\":{\"name\":\"echo\",\"arguments\":\"{\\\"text\\\":\\\"hi\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"total_tokens\":15}}\n\n",
            "data: [DONE]\n\n"
        ),
    );

    let client = OpenAiCompatClient::new(ProviderConfig::new("https://example.invalid"), http);
    let events = client
        .stream(&sample_request())
        .expect("stream")
        .collect::<Result<Vec<_>, _>>()
        .expect("events");

    assert!(matches!(
        events.first(),
        Some(StreamEvent::MessageStart { .. })
    ));
    assert!(events.iter().any(|event| matches!(
        event,
        StreamEvent::ContentBlockDelta {
            delta: ContentDelta::Text { text },
            ..
        } if text == "hel"
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        StreamEvent::ContentBlockDelta {
            delta: ContentDelta::ToolCall { .. },
            ..
        }
    )));
    assert!(events
        .iter()
        .any(|event| matches!(event, StreamEvent::Usage { .. })));
    assert!(events.iter().any(|event| matches!(
        event,
        StreamEvent::MessageStop { finish_reason } if finish_reason.0 == "tool_calls"
    )));
}
