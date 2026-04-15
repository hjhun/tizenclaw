use serde::{Deserialize, Serialize};

use crate::task_packet::{TaskFailure, TaskPriority};
use crate::trust_resolver::TrustResolution;
use crate::worker_boot::WorkerBootState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LaneEventKind {
    TaskQueued,
    TrustBlocked,
    WorkerAssigned,
    TaskStarted,
    TaskCompleted,
    TaskFailed,
    WorkerStateChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LaneEventPayload {
    TaskQueued {
        summary: String,
        priority: TaskPriority,
    },
    TrustBlocked {
        resolution: TrustResolution,
    },
    WorkerAssigned {
        worker_id: String,
    },
    TaskStarted {
        worker_id: Option<String>,
    },
    TaskCompleted {
        worker_id: Option<String>,
    },
    TaskFailed {
        failure: TaskFailure,
    },
    WorkerStateChanged {
        worker_id: String,
        state: WorkerBootState,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaneEvent {
    pub sequence: u64,
    pub lane_id: String,
    pub task_id: Option<String>,
    pub worker_id: Option<String>,
    pub kind: LaneEventKind,
    pub detail: String,
    pub payload: LaneEventPayload,
}

impl LaneEvent {
    pub fn new(
        sequence: u64,
        lane_id: impl Into<String>,
        task_id: Option<String>,
        worker_id: Option<String>,
        kind: LaneEventKind,
        detail: impl Into<String>,
        payload: LaneEventPayload,
    ) -> Self {
        Self {
            sequence,
            lane_id: lane_id.into(),
            task_id,
            worker_id,
            kind,
            detail: detail.into(),
            payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trust_resolver::{
        TrustDecision, TrustLevel, TrustRequirement, TrustResolution, TrustSubject,
        TrustSubjectKind,
    };

    #[test]
    fn lane_event_keeps_typed_payloads_serializable() {
        let event = LaneEvent::new(
            7,
            "lane-a",
            Some("task-1".to_string()),
            Some("worker-1".to_string()),
            LaneEventKind::TrustBlocked,
            "worker trust gate blocked start",
            LaneEventPayload::TrustBlocked {
                resolution: TrustResolution {
                    subject: TrustSubject::new(TrustSubjectKind::Task, "task-1"),
                    requirement: TrustRequirement::at_least(TrustLevel::Trusted),
                    actual_level: TrustLevel::Restricted,
                    decision: TrustDecision::Denied,
                    failure: Some(
                        crate::trust_resolver::TrustFailureReason::InsufficientLevel {
                            required: TrustLevel::Trusted,
                            actual: TrustLevel::Restricted,
                        },
                    ),
                    reason: "worker trust gate blocked start".to_string(),
                },
            },
        );

        let json = serde_json::to_string(&event).expect("serialize lane event");
        let restored: LaneEvent = serde_json::from_str(&json).expect("deserialize lane event");

        assert_eq!(restored.sequence, 7);
        assert_eq!(restored.kind, LaneEventKind::TrustBlocked);
        assert!(matches!(
            restored.payload,
            LaneEventPayload::TrustBlocked { .. }
        ));
    }
}
