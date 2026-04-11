use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HookPhase {
    PrePrompt,
    PreTool,
    PostTool,
    PostSession,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HookSpec {
    pub name: String,
    pub phase: HookPhase,
    pub command: String,
    pub enabled: bool,
}
