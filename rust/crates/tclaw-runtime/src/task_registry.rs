use serde::{Deserialize, Serialize};

use crate::task_packet::TaskPacket;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TaskRegistrySnapshot {
    pub active_tasks: Vec<TaskPacket>,
    pub completed_tasks: Vec<String>,
}
