use serde::{Deserialize, Serialize};

use crate::permissions::{PermissionDecision, PermissionMode};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionEnforcerState {
    pub mode: PermissionMode,
    pub last_decision: Option<PermissionDecision>,
}
