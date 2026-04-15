mod client;
mod error;
mod http_client;
mod prompt_cache;
mod providers;
mod sse;
mod types;

pub use client::{ApiClient, EventStream, ProviderClient};
pub use error::ApiError;
pub use http_client::{
    HttpClient, HttpClientError, HttpMethod, HttpRequest, HttpResponse, StaticHttpClient,
};
pub use prompt_cache::{PromptCacheConfig, PromptCacheMode, PromptCacheUsage};
pub use providers::{AnthropicClient, OpenAiCompatClient, ProviderConfig};
pub use sse::{SseEvent, SseParser};
pub use types::{
    ChatMessage, ChatRequest, ChatResponse, ContentBlock, ContentDelta, FinishReason, MessageRole,
    ProviderKind, ResponseFormat, ResponseMetadata, StreamEvent, SurfaceDescriptor, ToolCallDelta,
    ToolDefinition, Usage,
};

pub fn canonical_surfaces() -> Vec<SurfaceDescriptor> {
    vec![
        SurfaceDescriptor {
            name: "cli".into(),
            role: "operator entrypoint".into(),
        },
        SurfaceDescriptor {
            name: "runtime".into(),
            role: "canonical daemon implementation".into(),
        },
        SurfaceDescriptor {
            name: "tools".into(),
            role: "tool integration boundary".into(),
        },
        SurfaceDescriptor {
            name: "plugins".into(),
            role: "plugin integration boundary".into(),
        },
        SurfaceDescriptor {
            name: "api".into(),
            role: "provider abstraction and streaming boundary".into(),
        },
        SurfaceDescriptor {
            name: "commands".into(),
            role: "slash-command registry and parsing boundary".into(),
        },
    ]
}
