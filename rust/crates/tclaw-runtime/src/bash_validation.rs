use serde::{Deserialize, Serialize};

use crate::bash::BashExecutionPlan;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BashValidationViolation {
    pub command_index: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BashValidationResult {
    pub valid: bool,
    pub violations: Vec<BashValidationViolation>,
}

impl BashValidationResult {
    pub fn from_plan(plan: &BashExecutionPlan) -> Self {
        let violations = plan
            .commands
            .iter()
            .enumerate()
            .filter(|(_, command)| command.program.trim().is_empty())
            .map(|(command_index, _)| BashValidationViolation {
                command_index,
                message: "bash program must not be empty".to_string(),
            })
            .collect::<Vec<_>>();

        Self {
            valid: violations.is_empty(),
            violations,
        }
    }
}
