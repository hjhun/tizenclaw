use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    permissions::{
        PermissionLevel, PermissionMode, PermissionOutcome, PermissionRequest, PermissionScope,
    },
    sandbox::SandboxPolicy,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyEffect {
    Allow,
    Deny,
    Ask,
}

impl Default for PolicyEffect {
    fn default() -> Self {
        Self::Ask
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PolicyRule {
    pub rule_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<PermissionScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_program: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_level: Option<PermissionLevel>,
    pub effect: PolicyEffect,
    #[serde(default)]
    pub rationale: String,
}

impl PolicyRule {
    pub fn matches(&self, request: &PermissionRequest, effective_level: PermissionLevel) -> bool {
        if let Some(scope) = &self.scope {
            if scope != &request.scope {
                return false;
            }
        }

        if let Some(tool_name) = &self.tool_name {
            if request.tool_name.as_deref() != Some(tool_name.as_str()) {
                return false;
            }
        }

        if let Some(target_prefix) = &self.target_prefix {
            if !request.target.starts_with(target_prefix) {
                return false;
            }
        }

        if let Some(command_program) = &self.command_program {
            let matches_program = request
                .bash_plan
                .as_ref()
                .map(|plan| {
                    plan.commands
                        .iter()
                        .any(|command| command.program == *command_program)
                })
                .unwrap_or(false);
            if !matches_program {
                return false;
            }
        }

        if let Some(min_level) = self.min_level {
            if effective_level < min_level {
                return false;
            }
        }

        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PolicyEngineState {
    #[serde(default)]
    pub active_rules: Vec<PolicyRule>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tool_minimum_levels: BTreeMap<String, PermissionLevel>,
}

impl PolicyEngineState {
    pub fn merged(&self, overrides: &PolicyEngineState) -> Self {
        let mut tool_minimum_levels = self.tool_minimum_levels.clone();
        for (tool_name, level) in &overrides.tool_minimum_levels {
            tool_minimum_levels.insert(tool_name.clone(), *level);
        }

        let mut active_rules = self.active_rules.clone();
        active_rules.extend(overrides.active_rules.clone());

        Self {
            active_rules,
            tool_minimum_levels,
        }
    }

    pub fn minimum_level_for(&self, request: &PermissionRequest) -> PermissionLevel {
        request
            .tool_name
            .as_ref()
            .and_then(|tool_name| self.tool_minimum_levels.get(tool_name))
            .copied()
            .unwrap_or(request.minimum_level)
            .max(request.minimum_level)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyEvaluation {
    pub outcome: PermissionOutcome,
    pub rationale: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_rule: Option<String>,
    pub effective_level: PermissionLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicyContext {
    pub mode: PermissionMode,
    pub sandbox: SandboxPolicy,
}

pub struct PolicyEngine;

impl PolicyEngine {
    pub fn evaluate(
        policy: &PolicyEngineState,
        request: &PermissionRequest,
        context: &PolicyContext,
    ) -> PolicyEvaluation {
        let effective_level = policy.minimum_level_for(request);

        if let Some(evaluation) = evaluate_sandbox(request, effective_level, context) {
            return evaluation;
        }

        if let Some(rule) = policy
            .active_rules
            .iter()
            .find(|rule| rule.matches(request, effective_level))
        {
            let rationale = if rule.rationale.trim().is_empty() {
                format!("matched policy rule {}", rule.rule_id)
            } else {
                rule.rationale.clone()
            };
            let outcome = match rule.effect {
                PolicyEffect::Allow => PermissionOutcome::Allowed,
                PolicyEffect::Deny => PermissionOutcome::Denied,
                PolicyEffect::Ask => PermissionOutcome::Escalated,
            };
            return PolicyEvaluation {
                outcome,
                rationale: rationale.clone(),
                reasons: vec![
                    format!("matched rule {}", rule.rule_id),
                    format!("effective permission level is {effective_level:?}").to_lowercase(),
                    rationale,
                ],
                matched_rule: Some(rule.rule_id.clone()),
                effective_level,
            };
        }

        let (outcome, rationale) = match context.mode {
            PermissionMode::AllowAll => (
                PermissionOutcome::Allowed,
                "permission mode allow_all automatically approves requests".to_string(),
            ),
            PermissionMode::DenyAll => (
                PermissionOutcome::Denied,
                "permission mode deny_all blocks all guarded requests".to_string(),
            ),
            PermissionMode::Ask => (
                PermissionOutcome::Escalated,
                "permission mode ask requires a prompt decision".to_string(),
            ),
            PermissionMode::RepoPolicy => match effective_level {
                PermissionLevel::Low => (
                    PermissionOutcome::Allowed,
                    "repo policy auto-allows low-risk requests".to_string(),
                ),
                PermissionLevel::Standard | PermissionLevel::Sensitive => (
                    PermissionOutcome::Escalated,
                    "repo policy escalates standard or sensitive requests".to_string(),
                ),
            },
        };

        PolicyEvaluation {
            outcome,
            reasons: vec![
                format!("permission mode is {:?}", context.mode).to_lowercase(),
                format!("effective permission level is {effective_level:?}").to_lowercase(),
                rationale.clone(),
            ],
            rationale,
            matched_rule: None,
            effective_level,
        }
    }
}

fn evaluate_sandbox(
    request: &PermissionRequest,
    effective_level: PermissionLevel,
    context: &PolicyContext,
) -> Option<PolicyEvaluation> {
    if !context.sandbox.enabled {
        return None;
    }

    if request.scope == PermissionScope::Network && !context.sandbox.network_access {
        let rationale = format!(
            "sandbox profile {} blocks network access",
            context.sandbox.profile_name
        );
        return Some(PolicyEvaluation {
            outcome: PermissionOutcome::Denied,
            reasons: vec![
                rationale.clone(),
                format!("effective permission level is {effective_level:?}").to_lowercase(),
            ],
            rationale,
            matched_rule: None,
            effective_level,
        });
    }

    if request.scope == PermissionScope::Write
        && !context.sandbox.writable_roots.is_empty()
        && !target_is_within_writable_roots(&request.target, &context.sandbox.writable_roots)
    {
        let rationale = format!(
            "sandbox profile {} does not allow writes to {}",
            context.sandbox.profile_name, request.target
        );
        return Some(PolicyEvaluation {
            outcome: PermissionOutcome::Denied,
            reasons: vec![
                rationale.clone(),
                format!(
                    "allowed writable roots: {}",
                    context.sandbox.writable_roots.join(", ")
                ),
            ],
            rationale,
            matched_rule: None,
            effective_level,
        });
    }

    None
}

fn target_is_within_writable_roots(target: &str, roots: &[String]) -> bool {
    roots.iter().any(|root| {
        if root == "." {
            return !Path::new(target).is_absolute();
        }

        target == root
            || target.starts_with(&format!("{root}/"))
            || target.starts_with(&format!("{root}\\"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> PermissionRequest {
        PermissionRequest {
            scope: PermissionScope::Execute,
            target: "shell.exec".to_string(),
            reason: "run a shell tool".to_string(),
            tool_name: Some("shell".to_string()),
            minimum_level: PermissionLevel::Low,
            bash_plan: None,
            metadata: BTreeMap::new(),
        }
    }

    fn context(mode: PermissionMode) -> PolicyContext {
        PolicyContext {
            mode,
            sandbox: SandboxPolicy::default(),
        }
    }

    #[test]
    fn matches_override_rule_before_mode_defaults() {
        let policy = PolicyEngineState {
            active_rules: vec![PolicyRule {
                rule_id: "deny-shell".to_string(),
                tool_name: Some("shell".to_string()),
                effect: PolicyEffect::Deny,
                rationale: "shell tool is disabled by repo override".to_string(),
                ..PolicyRule::default()
            }],
            ..PolicyEngineState::default()
        };

        let evaluation =
            PolicyEngine::evaluate(&policy, &request(), &context(PermissionMode::AllowAll));

        assert_eq!(evaluation.outcome, PermissionOutcome::Denied);
        assert_eq!(evaluation.matched_rule.as_deref(), Some("deny-shell"));
        assert!(evaluation.rationale.contains("disabled by repo override"));
    }

    #[test]
    fn tool_specific_minimum_levels_raise_effective_level() {
        let mut policy = PolicyEngineState::default();
        policy
            .tool_minimum_levels
            .insert("shell".to_string(), PermissionLevel::Sensitive);

        let evaluation =
            PolicyEngine::evaluate(&policy, &request(), &context(PermissionMode::RepoPolicy));

        assert_eq!(evaluation.effective_level, PermissionLevel::Sensitive);
        assert_eq!(evaluation.outcome, PermissionOutcome::Escalated);
    }

    #[test]
    fn sandbox_policy_denies_writes_outside_writable_roots() {
        let request = PermissionRequest {
            scope: PermissionScope::Write,
            target: "/tmp/outside.txt".to_string(),
            reason: "write to tmp".to_string(),
            tool_name: Some("write_file".to_string()),
            minimum_level: PermissionLevel::Standard,
            bash_plan: None,
            metadata: BTreeMap::new(),
        };
        let context = PolicyContext {
            mode: PermissionMode::AllowAll,
            sandbox: SandboxPolicy {
                enabled: true,
                profile_name: "workspace-write".to_string(),
                writable_roots: vec!["workspace".to_string()],
                network_access: false,
            },
        };

        let evaluation = PolicyEngine::evaluate(&PolicyEngineState::default(), &request, &context);

        assert_eq!(evaluation.outcome, PermissionOutcome::Denied);
        assert!(evaluation.rationale.contains("does not allow writes"));
    }
}
