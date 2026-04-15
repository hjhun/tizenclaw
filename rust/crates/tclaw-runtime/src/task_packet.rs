use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::trust_resolver::{TrustRequirement, TrustResolution};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Queued,
    BlockedByTrust,
    Assigned,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskAssignment {
    pub lane_id: String,
    pub worker_id: Option<String>,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskFailure {
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskTrustGate {
    pub requirement: TrustRequirement,
    pub resolution: Option<TrustResolution>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskPacket {
    pub task_id: String,
    pub summary: String,
    pub priority: TaskPriority,
    pub labels: Vec<String>,
    pub status: TaskStatus,
    pub assignment: Option<TaskAssignment>,
    pub trust: Option<TaskTrustGate>,
    pub metadata: BTreeMap<String, String>,
    pub failure: Option<TaskFailure>,
}

impl TaskPacket {
    pub fn queued(
        task_id: impl Into<String>,
        summary: impl Into<String>,
        priority: TaskPriority,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            summary: summary.into(),
            priority,
            labels: Vec::new(),
            status: TaskStatus::Queued,
            assignment: None,
            trust: None,
            metadata: BTreeMap::new(),
            failure: None,
        }
    }

    pub fn with_lane(mut self, lane_id: impl Into<String>) -> Self {
        self.assignment = Some(TaskAssignment {
            lane_id: lane_id.into(),
            worker_id: None,
            session_id: None,
        });
        self
    }

    pub fn with_labels(mut self, labels: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.labels = labels.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_trust_requirement(mut self, requirement: TrustRequirement) -> Self {
        self.trust = Some(TaskTrustGate {
            requirement,
            resolution: None,
        });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trust_resolver::{TrustLevel, TrustSubject, TrustSubjectKind};

    #[test]
    fn queued_constructor_sets_explicit_defaults() {
        let packet = TaskPacket::queued("task-1", "inspect trust gate", TaskPriority::High)
            .with_lane("lane-a")
            .with_labels(["worker", "review"])
            .with_trust_requirement(TrustRequirement::at_least(TrustLevel::Trusted));

        assert_eq!(packet.status, TaskStatus::Queued);
        assert_eq!(
            packet.assignment.as_ref().map(|a| a.lane_id.as_str()),
            Some("lane-a")
        );
        assert_eq!(
            packet
                .trust
                .as_ref()
                .map(|gate| gate.requirement.minimum_level.clone()),
            Some(TrustLevel::Trusted)
        );
    }

    #[test]
    fn task_packet_serializes_nested_trust_gate() {
        let mut packet = TaskPacket::queued("task-2", "run delegated check", TaskPriority::Normal);
        packet.trust = Some(TaskTrustGate {
            requirement: TrustRequirement::at_least(TrustLevel::Restricted),
            resolution: Some(TrustResolution {
                subject: TrustSubject::new(TrustSubjectKind::Task, "task-2"),
                requirement: TrustRequirement::at_least(TrustLevel::Restricted),
                actual_level: TrustLevel::Trusted,
                decision: crate::trust_resolver::TrustDecision::Allowed,
                failure: None,
                reason: "pre-approved".to_string(),
            }),
        });

        let json = serde_json::to_string(&packet).expect("serialize packet");
        let restored: TaskPacket = serde_json::from_str(&json).expect("deserialize packet");

        assert_eq!(
            restored
                .trust
                .and_then(|gate| gate.resolution)
                .map(|resolution| resolution.reason),
            Some("pre-approved".to_string())
        );
    }
}
