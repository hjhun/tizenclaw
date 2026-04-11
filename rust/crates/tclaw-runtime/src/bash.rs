use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BashCommand {
    pub program: String,
    pub args: Vec<String>,
    pub working_dir: Option<String>,
}

impl BashCommand {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            working_dir: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BashExecutionPlan {
    pub commands: Vec<BashCommand>,
    pub require_clean_environment: bool,
}
