use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CompactionPlan {
    pub target: String,
    pub max_items: usize,
    pub preserve_latest: usize,
}
