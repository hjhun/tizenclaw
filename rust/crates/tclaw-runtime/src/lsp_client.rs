use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct LspClientSpec {
    pub language: String,
    pub command: String,
    pub root_uri: Option<String>,
}
