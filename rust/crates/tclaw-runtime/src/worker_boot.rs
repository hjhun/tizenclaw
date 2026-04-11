use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkerKind {
    Default,
    Explorer,
    Worker,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkerBootState {
    Requested,
    Booting,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerIdentity {
    pub worker_id: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerBootSpec {
    pub identity: WorkerIdentity,
    pub kind: WorkerKind,
    pub state: WorkerBootState,
    pub inherited_session_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_boot_spec_serializes_state() {
        let spec = WorkerBootSpec {
            identity: WorkerIdentity {
                worker_id: "worker-1".to_string(),
                display_name: Some("Agent".to_string()),
            },
            kind: WorkerKind::Worker,
            state: WorkerBootState::Ready,
            inherited_session_id: Some("session-1".to_string()),
        };

        let json = serde_json::to_string(&spec).expect("serialize worker spec");
        let restored: WorkerBootSpec =
            serde_json::from_str(&json).expect("deserialize worker spec");

        assert_eq!(restored.state, WorkerBootState::Ready);
        assert_eq!(restored.kind, WorkerKind::Worker);
    }
}
