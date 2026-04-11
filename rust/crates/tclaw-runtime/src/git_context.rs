use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct GitContextSnapshot {
    pub repository_root: String,
    pub current_branch: String,
    pub head_commit: Option<String>,
    pub has_uncommitted_changes: bool,
}
