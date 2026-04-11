use serde::{Deserialize, Serialize};

use crate::config::RuntimeConfig;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfigValidationIssue {
    pub field: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ConfigValidationReport {
    pub valid: bool,
    pub issues: Vec<ConfigValidationIssue>,
}

impl ConfigValidationReport {
    pub fn from_config(config: &RuntimeConfig) -> Self {
        let mut issues = Vec::new();

        if config.paths.session_dir.trim().is_empty() {
            issues.push(ConfigValidationIssue {
                field: "paths.session_dir".to_string(),
                message: "session dir must not be empty".to_string(),
            });
        }

        Self {
            valid: issues.is_empty(),
            issues,
        }
    }
}
