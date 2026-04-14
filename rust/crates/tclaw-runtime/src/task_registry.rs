use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, RwLock};

use serde::{Deserialize, Serialize};

use crate::lane_events::{LaneEvent, LaneEventKind, LaneEventPayload};
use crate::task_packet::{TaskAssignment, TaskFailure, TaskPacket, TaskStatus};
use crate::trust_resolver::TrustResolution;
use crate::worker_boot::WorkerBootState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TaskRegistrySnapshot {
    pub active_tasks: Vec<TaskPacket>,
    pub completed_tasks: Vec<String>,
    pub failed_tasks: Vec<String>,
    pub lane_events: Vec<LaneEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskRegistryError {
    DuplicateTask {
        task_id: String,
    },
    UnknownTask {
        task_id: String,
    },
    MissingLaneAssignment {
        task_id: String,
    },
    InvalidStatusTransition {
        task_id: String,
        from: TaskStatus,
        to: TaskStatus,
    },
}

#[derive(Debug, Clone)]
struct TaskEntryHandle {
    inner: Arc<RwLock<TaskPacket>>,
}

impl TaskEntryHandle {
    fn new(packet: TaskPacket) -> Self {
        Self {
            inner: Arc::new(RwLock::new(packet)),
        }
    }

    fn snapshot(&self) -> TaskPacket {
        let task = self.inner.read().expect("task entry read lock");
        task.clone()
    }
}

#[derive(Debug, Default)]
struct LaneEventState {
    next_sequence: u64,
    events: Vec<LaneEvent>,
}

#[derive(Debug, Clone, Default)]
pub struct TaskRegistry {
    tasks: Arc<RwLock<BTreeMap<String, TaskEntryHandle>>>,
    lane_events: Arc<Mutex<LaneEventState>>,
}

impl TaskRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&self, mut packet: TaskPacket) -> Result<TaskPacket, TaskRegistryError> {
        packet.status = TaskStatus::Queued;
        let lane_id = packet
            .assignment
            .as_ref()
            .map(|assignment| assignment.lane_id.clone())
            .ok_or_else(|| TaskRegistryError::MissingLaneAssignment {
                task_id: packet.task_id.clone(),
            })?;

        let mut tasks = self.tasks.write().expect("task registry index write lock");
        if tasks.contains_key(&packet.task_id) {
            return Err(TaskRegistryError::DuplicateTask {
                task_id: packet.task_id,
            });
        }

        tasks.insert(packet.task_id.clone(), TaskEntryHandle::new(packet.clone()));
        drop(tasks);

        self.push_lane_event(
            lane_id,
            Some(packet.task_id.clone()),
            None,
            LaneEventKind::TaskQueued,
            format!("queued {}", packet.summary),
            LaneEventPayload::TaskQueued {
                summary: packet.summary.clone(),
                priority: packet.priority.clone(),
            },
        );

        Ok(packet)
    }

    pub fn get(&self, task_id: &str) -> Option<TaskPacket> {
        let tasks = self.tasks.read().expect("task registry index read lock");
        tasks.get(task_id).map(TaskEntryHandle::snapshot)
    }

    pub fn lane_events(&self) -> Vec<LaneEvent> {
        let state = self
            .lane_events
            .lock()
            .expect("task registry lane event lock");
        state.events.clone()
    }

    pub fn snapshot(&self) -> TaskRegistrySnapshot {
        let entries = {
            let tasks = self.tasks.read().expect("task registry index read lock");
            tasks.values().cloned().collect::<Vec<_>>()
        };
        let lane_events = self.lane_events();

        let mut active_tasks = Vec::new();
        let mut completed_tasks = BTreeSet::new();
        let mut failed_tasks = BTreeSet::new();

        for entry in entries {
            let task = entry.snapshot();
            match task.status {
                TaskStatus::Completed => {
                    completed_tasks.insert(task.task_id);
                }
                TaskStatus::Failed => {
                    failed_tasks.insert(task.task_id);
                }
                TaskStatus::Cancelled => {}
                _ => active_tasks.push(task),
            }
        }

        TaskRegistrySnapshot {
            active_tasks,
            completed_tasks: completed_tasks.into_iter().collect(),
            failed_tasks: failed_tasks.into_iter().collect(),
            lane_events,
        }
    }

    pub fn record_trust_resolution(
        &self,
        task_id: &str,
        resolution: TrustResolution,
    ) -> Result<TaskPacket, TaskRegistryError> {
        let entry = self.entry(task_id)?;
        let (task_snapshot, lane_event) = {
            let mut task = entry.inner.write().expect("task entry write lock");
            let lane_id = task_lane_id(&task)?;

            match &mut task.trust {
                Some(gate) => gate.resolution = Some(resolution.clone()),
                None => {
                    return Err(TaskRegistryError::InvalidStatusTransition {
                        task_id: task_id.to_string(),
                        from: task.status.clone(),
                        to: TaskStatus::BlockedByTrust,
                    });
                }
            }

            let lane_event = if resolution.is_allowed() {
                task.status = TaskStatus::Queued;
                None
            } else {
                task.status = TaskStatus::BlockedByTrust;
                Some((
                    lane_id,
                    Some(task.task_id.clone()),
                    task.assignment
                        .as_ref()
                        .and_then(|assignment| assignment.worker_id.clone()),
                    resolution.reason.clone(),
                    LaneEventPayload::TrustBlocked {
                        resolution: resolution.clone(),
                    },
                ))
            };

            (task.clone(), lane_event)
        };

        if let Some((lane_id, task_id, worker_id, detail, payload)) = lane_event {
            self.push_lane_event(
                lane_id,
                task_id,
                worker_id,
                LaneEventKind::TrustBlocked,
                detail,
                payload,
            );
        }

        Ok(task_snapshot)
    }

    pub fn assign_worker(
        &self,
        task_id: &str,
        worker_id: impl Into<String>,
        session_id: Option<String>,
    ) -> Result<TaskPacket, TaskRegistryError> {
        let worker_id = worker_id.into();
        let entry = self.entry(task_id)?;
        let (task_snapshot, lane_id, event_task_id) = {
            let mut task = entry.inner.write().expect("task entry write lock");
            ensure_transition(&task, TaskStatus::Assigned)?;

            let lane_id = task_lane_id(&task)?;
            let assignment = task
                .assignment
                .get_or_insert_with(|| TaskAssignment {
                    lane_id: lane_id.clone(),
                    worker_id: None,
                    session_id: None,
                });
            assignment.worker_id = Some(worker_id.clone());
            assignment.session_id = session_id;
            task.status = TaskStatus::Assigned;

            (task.clone(), lane_id, task.task_id.clone())
        };

        self.push_lane_event(
            lane_id,
            Some(event_task_id),
            Some(worker_id.clone()),
            LaneEventKind::WorkerAssigned,
            format!("assigned to {}", worker_id),
            LaneEventPayload::WorkerAssigned { worker_id },
        );

        Ok(task_snapshot)
    }

    pub fn start_task(&self, task_id: &str) -> Result<TaskPacket, TaskRegistryError> {
        let entry = self.entry(task_id)?;
        let (task_snapshot, lane_id, event_task_id, worker_id) = {
            let mut task = entry.inner.write().expect("task entry write lock");
            ensure_transition(&task, TaskStatus::Running)?;
            task.status = TaskStatus::Running;
            let lane_id = task_lane_id(&task)?;
            let worker_id = task
                .assignment
                .as_ref()
                .and_then(|assignment| assignment.worker_id.clone());

            (task.clone(), lane_id, task.task_id.clone(), worker_id)
        };

        self.push_lane_event(
            lane_id,
            Some(event_task_id),
            worker_id.clone(),
            LaneEventKind::TaskStarted,
            "task started",
            LaneEventPayload::TaskStarted { worker_id },
        );

        Ok(task_snapshot)
    }

    pub fn complete_task(&self, task_id: &str) -> Result<TaskPacket, TaskRegistryError> {
        let entry = self.entry(task_id)?;
        let (task_snapshot, lane_id, event_task_id, worker_id) = {
            let mut task = entry.inner.write().expect("task entry write lock");
            ensure_transition(&task, TaskStatus::Completed)?;
            task.status = TaskStatus::Completed;
            task.failure = None;
            let lane_id = task_lane_id(&task)?;
            let worker_id = task
                .assignment
                .as_ref()
                .and_then(|assignment| assignment.worker_id.clone());

            (task.clone(), lane_id, task.task_id.clone(), worker_id)
        };

        self.push_lane_event(
            lane_id,
            Some(event_task_id),
            worker_id.clone(),
            LaneEventKind::TaskCompleted,
            "task completed",
            LaneEventPayload::TaskCompleted { worker_id },
        );
        Ok(task_snapshot)
    }

    pub fn fail_task(
        &self,
        task_id: &str,
        failure: TaskFailure,
    ) -> Result<TaskPacket, TaskRegistryError> {
        let entry = self.entry(task_id)?;
        let (task_snapshot, lane_id, event_task_id, worker_id) = {
            let mut task = entry.inner.write().expect("task entry write lock");
            ensure_transition(&task, TaskStatus::Failed)?;
            task.status = TaskStatus::Failed;
            task.failure = Some(failure.clone());
            let lane_id = task_lane_id(&task)?;
            let worker_id = task
                .assignment
                .as_ref()
                .and_then(|assignment| assignment.worker_id.clone());

            (task.clone(), lane_id, task.task_id.clone(), worker_id)
        };

        self.push_lane_event(
            lane_id,
            Some(event_task_id),
            worker_id,
            LaneEventKind::TaskFailed,
            failure.message.clone(),
            LaneEventPayload::TaskFailed { failure },
        );
        Ok(task_snapshot)
    }

    pub fn record_worker_state(
        &self,
        lane_id: impl Into<String>,
        worker_id: impl Into<String>,
        state_name: WorkerBootState,
    ) {
        let lane_id = lane_id.into();
        let worker_id = worker_id.into();
        self.push_lane_event(
            lane_id,
            None,
            Some(worker_id.clone()),
            LaneEventKind::WorkerStateChanged,
            format!("worker changed state to {:?}", state_name),
            LaneEventPayload::WorkerStateChanged {
                worker_id,
                state: state_name,
            },
        );
    }

    fn entry(&self, task_id: &str) -> Result<TaskEntryHandle, TaskRegistryError> {
        let tasks = self.tasks.read().expect("task registry index read lock");
        tasks
            .get(task_id)
            .cloned()
            .ok_or_else(|| TaskRegistryError::UnknownTask {
                task_id: task_id.to_string(),
            })
    }

    fn push_lane_event(
        &self,
        lane_id: String,
        task_id: Option<String>,
        worker_id: Option<String>,
        kind: LaneEventKind,
        detail: impl Into<String>,
        payload: LaneEventPayload,
    ) {
        let mut state = self
            .lane_events
            .lock()
            .expect("task registry lane event lock");
        let sequence = state.next_sequence;
        state.events.push(LaneEvent::new(
            sequence,
            lane_id,
            task_id,
            worker_id,
            kind,
            detail,
            payload,
        ));
        state.next_sequence += 1;
    }
}

fn task_lane_id(task: &TaskPacket) -> Result<String, TaskRegistryError> {
    task.assignment
        .as_ref()
        .map(|assignment| assignment.lane_id.clone())
        .ok_or_else(|| TaskRegistryError::MissingLaneAssignment {
            task_id: task.task_id.clone(),
        })
}

fn ensure_transition(task: &TaskPacket, next: TaskStatus) -> Result<(), TaskRegistryError> {
    if is_valid_transition(&task.status, &next) {
        Ok(())
    } else {
        Err(TaskRegistryError::InvalidStatusTransition {
            task_id: task.task_id.clone(),
            from: task.status.clone(),
            to: next,
        })
    }
}

fn is_valid_transition(current: &TaskStatus, next: &TaskStatus) -> bool {
    if current == next {
        return true;
    }

    match current {
        TaskStatus::Queued => matches!(
            next,
            TaskStatus::BlockedByTrust
                | TaskStatus::Assigned
                | TaskStatus::Cancelled
                | TaskStatus::Failed
        ),
        TaskStatus::BlockedByTrust => matches!(
            next,
            TaskStatus::Queued | TaskStatus::Cancelled | TaskStatus::Failed
        ),
        TaskStatus::Assigned => matches!(
            next,
            TaskStatus::Running | TaskStatus::Cancelled | TaskStatus::Failed
        ),
        TaskStatus::Running => matches!(
            next,
            TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::Failed
        ),
        TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled => false,
    }
}

#[cfg(test)]
mod tests;
