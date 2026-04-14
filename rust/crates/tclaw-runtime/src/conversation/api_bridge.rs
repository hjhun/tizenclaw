use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{prompt::PromptAssembly, session::ConversationMessage};

use super::ToolDefinition;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ApiRequest {
    pub session_id: String,
    pub request_index: usize,
    pub prompt: PromptAssembly,
    pub prompt_text: String,
    #[serde(default)]
    pub messages: Vec<ConversationMessage>,
    #[serde(default)]
    pub available_tools: Vec<ToolDefinition>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}
