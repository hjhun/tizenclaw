use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginLifecyclePhase {
    Discovered,
    Loaded,
    Active,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PluginLifecycleState {
    pub plugin_name: String,
    pub phase: Option<PluginLifecyclePhase>,
    pub last_error: Option<String>,
}
