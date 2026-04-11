use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RemoteRuntimeSpec {
    pub host: String,
    pub transport: String,
    pub workspace_root: Option<String>,
}
