use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeamCronEntry {
    pub schedule: String,
    pub task_name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TeamCronRegistry {
    pub entries: Vec<TeamCronEntry>,
}
