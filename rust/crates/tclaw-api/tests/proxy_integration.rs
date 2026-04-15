use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::json;
use tclaw_api::{
    ChatMessage, ChatRequest, HttpMethod, OpenAiCompatClient, ProviderClient, ProviderConfig,
    StaticHttpClient,
};

#[test]
fn provider_config_headers_and_auth_are_forwarded_to_the_http_layer() {
    let http = Arc::new(StaticHttpClient::new());
    http.push_json_response(
        200,
        json!({
            "id": "chatcmpl-2",
            "model": "proxy-model",
            "choices": [
                {
                    "message": { "role": "assistant", "content": "proxied" },
                    "finish_reason": "stop"
                }
            ]
        }),
    );

    let mut config = ProviderConfig::new("https://proxy.invalid/base");
    config.api_key = Some("secret-token".to_string());
    config.default_headers = BTreeMap::from([
        ("x-proxy-route".to_string(), "team-a".to_string()),
        ("x-trace-id".to_string(), "trace-123".to_string()),
    ]);

    let client = OpenAiCompatClient::new(config, http.clone());
    client
        .send(&ChatRequest {
            model: "proxy-model".to_string(),
            system: None,
            messages: vec![ChatMessage::user_text("hello proxy")],
            tools: vec![],
            stream: false,
            temperature: None,
            max_output_tokens: None,
            response_format: None,
            prompt_cache: None,
            metadata: None,
        })
        .expect("send");

    let request = http.take_requests().pop().expect("request");
    assert_eq!(request.method, HttpMethod::Post);
    assert_eq!(
        request.url,
        "https://proxy.invalid/base/v1/chat/completions"
    );
    assert_eq!(
        request.headers.get("authorization").map(String::as_str),
        Some("Bearer secret-token")
    );
    assert_eq!(
        request.headers.get("x-proxy-route").map(String::as_str),
        Some("team-a")
    );
    assert_eq!(
        request.headers.get("x-trace-id").map(String::as_str),
        Some("trace-123")
    );
}
