use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionControlCommand {
    Resume { session_id: String },
    Pause { session_id: String },
    Stop { session_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionControlResult {
    pub session_id: String,
    pub accepted: bool,
    pub message: String,
}
