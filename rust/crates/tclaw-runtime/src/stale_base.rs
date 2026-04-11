use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct StaleBaseReport {
    pub base_branch: String,
    pub commits_behind: usize,
}
