use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionControlCommand {
    Resume {
        session_id: String,
    },
    Pause {
        session_id: String,
    },
    Stop {
        session_id: String,
    },
    AttachWorker {
        session_id: String,
        worker_id: String,
    },
    DetachWorker {
        session_id: String,
        worker_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionControlStatus {
    Accepted,
    Rejected,
    Deferred,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionControlResult {
    pub session_id: String,
    pub accepted: bool,
    pub message: String,
    pub status: SessionControlStatus,
    pub worker_id: Option<String>,
}

impl SessionControlResult {
    pub fn accepted(session_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            accepted: true,
            message: message.into(),
            status: SessionControlStatus::Accepted,
            worker_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepted_result_sets_explicit_status() {
        let result = SessionControlResult::accepted("session-1", "resume queued");

        assert!(result.accepted);
        assert_eq!(result.status, SessionControlStatus::Accepted);
        assert_eq!(result.worker_id, None);
    }
}
