use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct StaleBranchReport {
    pub branch_name: String,
    pub commits_behind_base: usize,
    pub diverged: bool,
}
