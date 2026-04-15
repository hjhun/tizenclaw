use super::*;
use std::thread;

use crate::task_packet::{TaskPacket, TaskPriority};
use crate::trust_resolver::{
    TrustFailureReason, TrustLevel, TrustRequirement, TrustResolver, TrustSubject, TrustSubjectKind,
};

fn queued_task(task_id: &str) -> TaskPacket {
    TaskPacket::queued(task_id, "review worker output", TaskPriority::High).with_lane("lane-a")
}

#[test]
fn registry_records_lane_events_for_happy_path() {
    let registry = TaskRegistry::new();
    registry
        .register(queued_task("task-1"))
        .expect("register task");
    registry
        .assign_worker("task-1", "worker-1", Some("session-1".to_string()))
        .expect("assign worker");
    registry.start_task("task-1").expect("start task");
    registry.complete_task("task-1").expect("complete task");

    let snapshot = registry.snapshot();
    assert!(snapshot.active_tasks.is_empty());
    assert_eq!(snapshot.completed_tasks, vec!["task-1".to_string()]);
    assert_eq!(snapshot.lane_events.len(), 4);
    assert_eq!(snapshot.lane_events[0].kind, LaneEventKind::TaskQueued);
    assert_eq!(snapshot.lane_events[1].kind, LaneEventKind::WorkerAssigned);
    assert_eq!(snapshot.lane_events[2].kind, LaneEventKind::TaskStarted);
    assert_eq!(snapshot.lane_events[3].kind, LaneEventKind::TaskCompleted);
}

#[test]
fn registry_blocks_tasks_when_trust_resolution_denies_execution() {
    let registry = TaskRegistry::new();
    registry
        .register(
            queued_task("task-2")
                .with_trust_requirement(TrustRequirement::at_least(TrustLevel::Trusted)),
        )
        .expect("register gated task");

    let resolution = TrustResolver::deny(
        TrustSubject::new(TrustSubjectKind::Task, "task-2"),
        TrustRequirement::at_least(TrustLevel::Trusted),
        TrustLevel::Restricted,
        TrustFailureReason::InsufficientLevel {
            required: TrustLevel::Trusted,
            actual: TrustLevel::Restricted,
        },
        "worker trust gate denied execution",
    );
    let task = registry
        .record_trust_resolution("task-2", resolution)
        .expect("record trust failure");

    assert_eq!(task.status, TaskStatus::BlockedByTrust);
    assert_eq!(
        registry
            .lane_events()
            .last()
            .map(|event| event.kind.clone()),
        Some(LaneEventKind::TrustBlocked)
    );
}

#[test]
fn registry_records_failure_paths() {
    let registry = TaskRegistry::new();
    registry
        .register(queued_task("task-3"))
        .expect("register task");
    registry
        .assign_worker("task-3", "worker-3", None)
        .expect("assign");
    registry.start_task("task-3").expect("start");
    let task = registry
        .fail_task(
            "task-3",
            TaskFailure {
                message: "delegate crashed".to_string(),
                retryable: true,
            },
        )
        .expect("fail task");

    assert_eq!(task.status, TaskStatus::Failed);
    assert_eq!(registry.snapshot().failed_tasks, vec!["task-3".to_string()]);
}

#[test]
fn registry_rejects_invalid_status_transition() {
    let registry = TaskRegistry::new();
    registry
        .register(queued_task("task-4"))
        .expect("register task");

    let error = registry
        .complete_task("task-4")
        .expect_err("complete should fail");
    assert_eq!(
        error,
        TaskRegistryError::InvalidStatusTransition {
            task_id: "task-4".to_string(),
            from: TaskStatus::Queued,
            to: TaskStatus::Completed,
        }
    );
}

#[test]
fn registry_supports_concurrent_registration() {
    let registry = TaskRegistry::new();
    let handles = (0..16)
        .map(|index| {
            let registry = registry.clone();
            thread::spawn(move || {
                registry
                    .register(queued_task(&format!("task-{index}")))
                    .expect("register concurrent task");
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle.join().expect("concurrent registration thread");
    }

    let snapshot = registry.snapshot();
    assert_eq!(snapshot.active_tasks.len(), 16);
    assert_eq!(snapshot.lane_events.len(), 16);
    assert!(snapshot
        .lane_events
        .iter()
        .enumerate()
        .all(|(index, event)| event.sequence == index as u64));
}

#[test]
fn registry_supports_concurrent_task_completion() {
    let registry = TaskRegistry::new();
    for index in 0..8 {
        registry
            .register(queued_task(&format!("task-complete-{index}")))
            .expect("register completion task");
    }

    let handles = (0..8)
        .map(|index| {
            let registry = registry.clone();
            thread::spawn(move || {
                let task_id = format!("task-complete-{index}");
                let worker_id = format!("worker-{index}");
                registry
                    .assign_worker(&task_id, worker_id.clone(), None)
                    .expect("assign concurrent task");
                registry
                    .start_task(&task_id)
                    .expect("start concurrent task");
                registry
                    .complete_task(&task_id)
                    .expect("complete concurrent task");
            })
        })
        .collect::<Vec<_>>();

    for handle in handles {
        handle.join().expect("concurrent completion thread");
    }

    let snapshot = registry.snapshot();
    assert!(snapshot.active_tasks.is_empty());
    assert_eq!(snapshot.completed_tasks.len(), 8);
    assert_eq!(snapshot.lane_events.len(), 32);
    assert!(snapshot
        .lane_events
        .iter()
        .enumerate()
        .all(|(index, event)| event.sequence == index as u64));
}
