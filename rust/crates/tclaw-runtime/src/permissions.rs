use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PermissionScope {
    Read,
    Write,
    Execute,
    Network,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PermissionMode {
    Ask,
    AllowAll,
    DenyAll,
    RepoPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionRequest {
    pub scope: PermissionScope,
    pub target: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionDecision {
    pub request: PermissionRequest,
    pub allowed: bool,
    pub rationale: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_decision_round_trip_serializes_cleanly() {
        let decision = PermissionDecision {
            request: PermissionRequest {
                scope: PermissionScope::Write,
                target: "rust/crates/tclaw-runtime/src/lib.rs".to_string(),
                reason: "update public exports".to_string(),
            },
            allowed: true,
            rationale: "repo policy allows local edits".to_string(),
        };

        let json = serde_json::to_string(&decision).expect("serialize decision");
        let restored: PermissionDecision =
            serde_json::from_str(&json).expect("deserialize decision");

        assert!(restored.allowed);
        assert_eq!(restored.request.scope, PermissionScope::Write);
    }
}
