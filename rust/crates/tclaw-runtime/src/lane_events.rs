use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LaneEventKind {
    Started,
    Progress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaneEvent {
    pub lane_id: String,
    pub kind: LaneEventKind,
    pub detail: String,
}
