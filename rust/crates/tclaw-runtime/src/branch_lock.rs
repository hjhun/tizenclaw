use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BranchLockState {
    pub branch_name: String,
    pub locked_by_session: Option<String>,
    pub reason: Option<String>,
}
