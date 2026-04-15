use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use crate::trust_resolver::{TrustDecision, TrustResolution};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkerKind {
    Default,
    Explorer,
    Worker,
    Supervisor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkerBootState {
    Requested,
    TrustPending,
    Booting,
    Ready,
    Busy,
    Paused,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerIdentity {
    pub worker_id: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerTaskBinding {
    pub task_id: String,
    pub lane_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkerFailureReason {
    TrustDenied { reason: String },
    BootFailed { reason: String },
    TaskFailed { task_id: String, reason: String },
    Stopped { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorkerEventPayload {
    Registered,
    TrustCheckRequested,
    TrustResolved { resolution: TrustResolution },
    BootStarted,
    Ready,
    TaskAssigned { binding: WorkerTaskBinding },
    TaskReleased { task_id: String },
    Paused { reason: String },
    Resumed,
    Stopped { reason: String },
    Failed { reason: WorkerFailureReason },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerEvent {
    pub sequence: u64,
    pub worker_id: String,
    pub state: WorkerBootState,
    pub payload: WorkerEventPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerBootSpec {
    pub identity: WorkerIdentity,
    pub kind: WorkerKind,
    pub state: WorkerBootState,
    pub inherited_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerRecord {
    pub spec: WorkerBootSpec,
    pub trust: Option<TrustResolution>,
    pub current_task: Option<WorkerTaskBinding>,
    pub last_failure: Option<WorkerFailureReason>,
    pub event_count: usize,
}

impl WorkerRecord {
    pub fn worker_id(&self) -> &str {
        &self.spec.identity.worker_id
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorkerRegistrySnapshot {
    pub workers: Vec<WorkerRecord>,
    pub events: Vec<WorkerEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerRegistryError {
    DuplicateWorker {
        worker_id: String,
    },
    UnknownWorker {
        worker_id: String,
    },
    InvalidTransition {
        worker_id: String,
        from: WorkerBootState,
        to: WorkerBootState,
    },
}

#[derive(Debug, Default)]
struct WorkerRegistryState {
    next_sequence: u64,
    workers: BTreeMap<String, WorkerRecord>,
    events: Vec<WorkerEvent>,
}

#[derive(Debug, Clone, Default)]
pub struct WorkerRegistry {
    inner: Arc<RwLock<WorkerRegistryState>>,
}

impl WorkerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, spec: WorkerBootSpec) -> Result<WorkerRecord, WorkerRegistryError> {
        let worker_id = spec.identity.worker_id.clone();
        let mut state = self.inner.write().expect("worker registry write lock");
        if state.workers.contains_key(&worker_id) {
            return Err(WorkerRegistryError::DuplicateWorker { worker_id });
        }

        let record = WorkerRecord {
            spec,
            trust: None,
            current_task: None,
            last_failure: None,
            event_count: 0,
        };
        state.workers.insert(worker_id.clone(), record);
        let event = push_event(
            &mut state,
            worker_id.clone(),
            WorkerBootState::Requested,
            WorkerEventPayload::Registered,
        );
        let record = state
            .workers
            .get_mut(&worker_id)
            .expect("registered worker record");
        record.event_count += 1;
        record.spec.state = event.state.clone();
        Ok(record.clone())
    }

    pub fn list(&self) -> Vec<WorkerRecord> {
        let state = self.inner.read().expect("worker registry read lock");
        state.workers.values().cloned().collect()
    }

    pub fn get(&self, worker_id: &str) -> Option<WorkerRecord> {
        let state = self.inner.read().expect("worker registry read lock");
        state.workers.get(worker_id).cloned()
    }

    pub fn events(&self) -> Vec<WorkerEvent> {
        let state = self.inner.read().expect("worker registry read lock");
        state.events.clone()
    }

    pub fn snapshot(&self) -> WorkerRegistrySnapshot {
        let state = self.inner.read().expect("worker registry read lock");
        WorkerRegistrySnapshot {
            workers: state.workers.values().cloned().collect(),
            events: state.events.clone(),
        }
    }

    pub fn request_trust_check(
        &self,
        worker_id: &str,
    ) -> Result<WorkerRecord, WorkerRegistryError> {
        self.transition(worker_id, WorkerBootState::TrustPending, |record| {
            record.last_failure = None;
            WorkerEventPayload::TrustCheckRequested
        })
    }

    pub fn record_trust_resolution(
        &self,
        worker_id: &str,
        resolution: TrustResolution,
    ) -> Result<WorkerRecord, WorkerRegistryError> {
        let next_state = if resolution.decision == TrustDecision::Allowed {
            WorkerBootState::Booting
        } else {
            WorkerBootState::Failed
        };

        self.transition(worker_id, next_state, move |record| {
            record.trust = Some(resolution.clone());
            if resolution.decision != TrustDecision::Allowed {
                record.last_failure = Some(WorkerFailureReason::TrustDenied {
                    reason: resolution.reason.clone(),
                });
            } else {
                record.last_failure = None;
            }
            WorkerEventPayload::TrustResolved {
                resolution: resolution.clone(),
            }
        })
    }

    pub fn mark_boot_started(&self, worker_id: &str) -> Result<WorkerRecord, WorkerRegistryError> {
        self.transition(worker_id, WorkerBootState::Booting, |_| {
            WorkerEventPayload::BootStarted
        })
    }

    pub fn mark_ready(&self, worker_id: &str) -> Result<WorkerRecord, WorkerRegistryError> {
        self.transition(worker_id, WorkerBootState::Ready, |record| {
            record.last_failure = None;
            WorkerEventPayload::Ready
        })
    }

    pub fn assign_task(
        &self,
        worker_id: &str,
        binding: WorkerTaskBinding,
    ) -> Result<WorkerRecord, WorkerRegistryError> {
        self.transition(worker_id, WorkerBootState::Busy, move |record| {
            record.current_task = Some(binding.clone());
            WorkerEventPayload::TaskAssigned {
                binding: binding.clone(),
            }
        })
    }

    pub fn release_task(
        &self,
        worker_id: &str,
        task_id: &str,
    ) -> Result<WorkerRecord, WorkerRegistryError> {
        let task_id = task_id.to_string();
        self.transition(worker_id, WorkerBootState::Ready, move |record| {
            record.current_task = None;
            WorkerEventPayload::TaskReleased {
                task_id: task_id.clone(),
            }
        })
    }

    pub fn pause(
        &self,
        worker_id: &str,
        reason: impl Into<String>,
    ) -> Result<WorkerRecord, WorkerRegistryError> {
        let reason = reason.into();
        self.transition(worker_id, WorkerBootState::Paused, move |_| {
            WorkerEventPayload::Paused {
                reason: reason.clone(),
            }
        })
    }

    pub fn resume(&self, worker_id: &str) -> Result<WorkerRecord, WorkerRegistryError> {
        self.transition(worker_id, WorkerBootState::Ready, |_| {
            WorkerEventPayload::Resumed
        })
    }

    pub fn fail(
        &self,
        worker_id: &str,
        reason: WorkerFailureReason,
    ) -> Result<WorkerRecord, WorkerRegistryError> {
        self.transition(worker_id, WorkerBootState::Failed, move |record| {
            record.current_task = None;
            record.last_failure = Some(reason.clone());
            WorkerEventPayload::Failed {
                reason: reason.clone(),
            }
        })
    }

    pub fn stop(
        &self,
        worker_id: &str,
        reason: impl Into<String>,
    ) -> Result<WorkerRecord, WorkerRegistryError> {
        let reason = reason.into();
        self.transition(worker_id, WorkerBootState::Stopped, move |record| {
            record.current_task = None;
            WorkerEventPayload::Stopped {
                reason: reason.clone(),
            }
        })
    }

    fn transition<F>(
        &self,
        worker_id: &str,
        next_state: WorkerBootState,
        update: F,
    ) -> Result<WorkerRecord, WorkerRegistryError>
    where
        F: FnOnce(&mut WorkerRecord) -> WorkerEventPayload,
    {
        let mut state = self.inner.write().expect("worker registry write lock");
        let record =
            state
                .workers
                .get_mut(worker_id)
                .ok_or_else(|| WorkerRegistryError::UnknownWorker {
                    worker_id: worker_id.to_string(),
                })?;

        if !is_valid_transition(&record.spec.state, &next_state) {
            return Err(WorkerRegistryError::InvalidTransition {
                worker_id: worker_id.to_string(),
                from: record.spec.state.clone(),
                to: next_state,
            });
        }

        let payload = update(record);
        let event = push_event(
            &mut state,
            worker_id.to_string(),
            next_state.clone(),
            payload,
        );
        let record = state
            .workers
            .get_mut(worker_id)
            .expect("existing worker record");
        record.spec.state = event.state.clone();
        record.event_count += 1;
        Ok(record.clone())
    }
}

fn push_event(
    state: &mut WorkerRegistryState,
    worker_id: String,
    worker_state: WorkerBootState,
    payload: WorkerEventPayload,
) -> WorkerEvent {
    let event = WorkerEvent {
        sequence: state.next_sequence,
        worker_id,
        state: worker_state,
        payload,
    };
    state.next_sequence += 1;
    state.events.push(event.clone());
    event
}

fn is_valid_transition(current: &WorkerBootState, next: &WorkerBootState) -> bool {
    if current == next {
        return true;
    }

    match current {
        WorkerBootState::Requested => matches!(
            next,
            WorkerBootState::TrustPending
                | WorkerBootState::Booting
                | WorkerBootState::Stopped
                | WorkerBootState::Failed
        ),
        WorkerBootState::TrustPending => matches!(
            next,
            WorkerBootState::Booting | WorkerBootState::Stopped | WorkerBootState::Failed
        ),
        WorkerBootState::Booting => matches!(
            next,
            WorkerBootState::Ready | WorkerBootState::Stopped | WorkerBootState::Failed
        ),
        WorkerBootState::Ready => matches!(
            next,
            WorkerBootState::Busy
                | WorkerBootState::Paused
                | WorkerBootState::Stopped
                | WorkerBootState::Failed
        ),
        WorkerBootState::Busy => matches!(
            next,
            WorkerBootState::Ready
                | WorkerBootState::Paused
                | WorkerBootState::Stopped
                | WorkerBootState::Failed
        ),
        WorkerBootState::Paused => matches!(
            next,
            WorkerBootState::Ready | WorkerBootState::Stopped | WorkerBootState::Failed
        ),
        WorkerBootState::Stopped | WorkerBootState::Failed => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trust_resolver::{
        TrustFailureReason, TrustLevel, TrustRequirement, TrustResolver, TrustSubject,
        TrustSubjectKind,
    };

    fn worker_spec(worker_id: &str) -> WorkerBootSpec {
        WorkerBootSpec {
            identity: WorkerIdentity {
                worker_id: worker_id.to_string(),
                display_name: Some("Planner".to_string()),
            },
            kind: WorkerKind::Worker,
            state: WorkerBootState::Requested,
            inherited_session_id: Some("session-1".to_string()),
        }
    }

    #[test]
    fn worker_boot_spec_serializes_state() {
        let spec = worker_spec("worker-1");

        let json = serde_json::to_string(&spec).expect("serialize worker spec");
        let restored: WorkerBootSpec =
            serde_json::from_str(&json).expect("deserialize worker spec");

        assert_eq!(restored.state, WorkerBootState::Requested);
        assert_eq!(restored.kind, WorkerKind::Worker);
    }

    #[test]
    fn worker_registry_records_lifecycle_events_in_order() {
        let registry = WorkerRegistry::new();
        registry
            .register(worker_spec("worker-1"))
            .expect("register worker");
        registry
            .request_trust_check("worker-1")
            .expect("request trust check");

        let resolution = TrustResolver::resolve(
            TrustSubject::new(TrustSubjectKind::Worker, "worker-1"),
            TrustRequirement::at_least(TrustLevel::Trusted),
            TrustLevel::Trusted,
        );
        registry
            .record_trust_resolution("worker-1", resolution)
            .expect("record trust");
        registry
            .mark_ready("worker-1")
            .expect("worker becomes ready");
        registry
            .assign_task(
                "worker-1",
                WorkerTaskBinding {
                    task_id: "task-1".to_string(),
                    lane_id: "lane-a".to_string(),
                },
            )
            .expect("assign task");
        registry
            .release_task("worker-1", "task-1")
            .expect("release task");

        let events = registry.events();
        assert_eq!(events.len(), 6);
        assert_eq!(events[0].sequence, 0);
        assert_eq!(events[0].payload, WorkerEventPayload::Registered);
        assert_eq!(events[1].payload, WorkerEventPayload::TrustCheckRequested);
        assert!(matches!(
            events[2].payload,
            WorkerEventPayload::TrustResolved { .. }
        ));
        assert_eq!(events[3].payload, WorkerEventPayload::Ready);
        assert!(matches!(
            events[4].payload,
            WorkerEventPayload::TaskAssigned { .. }
        ));
        assert!(matches!(
            events[5].payload,
            WorkerEventPayload::TaskReleased { .. }
        ));
    }

    #[test]
    fn worker_registry_tracks_trust_failures_explicitly() {
        let registry = WorkerRegistry::new();
        registry
            .register(worker_spec("worker-2"))
            .expect("register worker");
        registry
            .request_trust_check("worker-2")
            .expect("request trust check");

        let resolution = TrustResolver::resolve(
            TrustSubject::new(TrustSubjectKind::Worker, "worker-2"),
            TrustRequirement::at_least(TrustLevel::Trusted),
            TrustLevel::Restricted,
        );

        let record = registry
            .record_trust_resolution("worker-2", resolution.clone())
            .expect("record trust failure");
        assert_eq!(record.spec.state, WorkerBootState::Failed);
        assert_eq!(
            record.last_failure,
            Some(WorkerFailureReason::TrustDenied {
                reason: resolution.reason.clone()
            })
        );
        assert_eq!(record.trust, Some(resolution));
    }

    #[test]
    fn worker_registry_rejects_invalid_transitions() {
        let registry = WorkerRegistry::new();
        registry
            .register(worker_spec("worker-3"))
            .expect("register worker");

        let error = registry
            .mark_ready("worker-3")
            .expect_err("ready should fail");
        assert_eq!(
            error,
            WorkerRegistryError::InvalidTransition {
                worker_id: "worker-3".to_string(),
                from: WorkerBootState::Requested,
                to: WorkerBootState::Ready,
            }
        );
    }

    #[test]
    fn worker_registry_can_record_runtime_failures() {
        let registry = WorkerRegistry::new();
        registry
            .register(worker_spec("worker-4"))
            .expect("register worker");
        registry
            .mark_boot_started("worker-4")
            .expect("boot starts directly");

        let failed = registry
            .fail(
                "worker-4",
                WorkerFailureReason::TaskFailed {
                    task_id: "task-x".to_string(),
                    reason: "panic in delegated command".to_string(),
                },
            )
            .expect("mark failed");

        assert_eq!(failed.spec.state, WorkerBootState::Failed);
        assert!(matches!(
            failed.last_failure,
            Some(WorkerFailureReason::TaskFailed { .. })
        ));
        assert!(matches!(
            registry.events().last().map(|event| &event.payload),
            Some(WorkerEventPayload::Failed { .. })
        ));
    }

    #[test]
    fn trust_failure_event_payload_keeps_reason_serializable() {
        let resolution = TrustResolver::deny(
            TrustSubject::new(TrustSubjectKind::Worker, "worker-5"),
            TrustRequirement::at_least(TrustLevel::Trusted),
            TrustLevel::Unknown,
            TrustFailureReason::InsufficientLevel {
                required: TrustLevel::Trusted,
                actual: TrustLevel::Unknown,
            },
            "worker boot requires trusted execution",
        );

        let payload = WorkerEventPayload::TrustResolved { resolution };
        let json = serde_json::to_string(&payload).expect("serialize payload");
        assert!(json.contains("worker boot requires trusted execution"));
    }
}
