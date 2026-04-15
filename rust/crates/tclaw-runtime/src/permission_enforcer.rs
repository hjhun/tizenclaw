use serde::{Deserialize, Serialize};

use crate::{
    bash_validation::BashValidationResult,
    config::RuntimeConfig,
    conversation::{ConversationRuntimeError, PermissionResolver},
    permissions::{
        PermissionDecision, PermissionDecisionSource, PermissionOutcome, PermissionPromptDecision,
        PermissionPromptRecord, PermissionPromptRequest, PermissionRequest,
    },
    policy_engine::{PolicyContext, PolicyEngine, PolicyEngineState},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PermissionEnforcerState {
    pub mode: crate::permissions::PermissionMode,
    pub last_decision: Option<PermissionDecision>,
    #[serde(default)]
    pub history: Vec<PermissionDecision>,
}

pub trait PermissionPrompter {
    fn prompt(
        &mut self,
        request: &PermissionPromptRequest,
    ) -> Result<PermissionPromptDecision, ConversationRuntimeError>;
}

#[derive(Debug, Default)]
pub struct DenyAllPrompter;

impl PermissionPrompter for DenyAllPrompter {
    fn prompt(
        &mut self,
        _request: &PermissionPromptRequest,
    ) -> Result<PermissionPromptDecision, ConversationRuntimeError> {
        Ok(PermissionPromptDecision::DenyOnce)
    }
}

#[derive(Debug, Default)]
pub struct RecordingPrompter {
    pub prompts: Vec<PermissionPromptRequest>,
    pub scripted_decisions: Vec<PermissionPromptDecision>,
}

impl RecordingPrompter {
    pub fn with_decisions(scripted_decisions: Vec<PermissionPromptDecision>) -> Self {
        Self {
            prompts: Vec::new(),
            scripted_decisions,
        }
    }
}

impl PermissionPrompter for RecordingPrompter {
    fn prompt(
        &mut self,
        request: &PermissionPromptRequest,
    ) -> Result<PermissionPromptDecision, ConversationRuntimeError> {
        self.prompts.push(request.clone());
        Ok(self
            .scripted_decisions
            .first()
            .cloned()
            .unwrap_or(PermissionPromptDecision::DenyOnce))
    }
}

pub struct PermissionEnforcer<P = DenyAllPrompter> {
    base_policy: PolicyEngineState,
    prompter: P,
    state: PermissionEnforcerState,
}

impl PermissionEnforcer<DenyAllPrompter> {
    pub fn new() -> Self {
        Self::with_prompter(DenyAllPrompter)
    }
}

impl Default for PermissionEnforcer<DenyAllPrompter> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P> PermissionEnforcer<P>
where
    P: PermissionPrompter,
{
    pub fn with_prompter(prompter: P) -> Self {
        Self {
            base_policy: PolicyEngineState::default(),
            prompter,
            state: PermissionEnforcerState::default(),
        }
    }

    pub fn with_policy(mut self, policy: PolicyEngineState) -> Self {
        self.base_policy = policy;
        self
    }

    pub fn state(&self) -> &PermissionEnforcerState {
        &self.state
    }

    pub fn into_parts(self) -> (PermissionEnforcerState, P) {
        (self.state, self.prompter)
    }

    fn effective_policy(&self, config: &RuntimeConfig) -> PolicyEngineState {
        self.base_policy.merged(&config.permission_policy)
    }

    fn decide_inner(
        &mut self,
        config: &RuntimeConfig,
        request: PermissionRequest,
    ) -> Result<PermissionDecision, ConversationRuntimeError> {
        self.state.mode = config.permission_mode.clone();

        if let Some(plan) = &request.bash_plan {
            let validation = BashValidationResult::from_plan(plan);
            if !validation.valid {
                let rationale = validation
                    .violations
                    .first()
                    .map(|violation| violation.message.clone())
                    .unwrap_or_else(|| "bash plan validation failed".to_string());
                let mut decision = PermissionDecision::deny(
                    request,
                    rationale,
                    PermissionDecisionSource::Validation,
                );
                decision.reasons = validation
                    .violations
                    .into_iter()
                    .map(|violation| {
                        format!(
                            "command {} validation error: {}",
                            violation.command_index, violation.message
                        )
                    })
                    .collect();
                return Ok(self.record(decision));
            }
        }

        let policy = self.effective_policy(config);
        let mut sandbox = config.sandbox_policy.clone();
        sandbox.enabled = sandbox.enabled && config.sandbox_enabled;
        let context = PolicyContext {
            mode: config.permission_mode.clone(),
            sandbox,
        };
        let evaluation = PolicyEngine::evaluate(&policy, &request, &context);
        let mut decision = match evaluation.outcome {
            PermissionOutcome::Allowed => PermissionDecision::allow(
                request.clone(),
                evaluation.rationale.clone(),
                if evaluation.matched_rule.is_some() {
                    PermissionDecisionSource::PolicyRule
                } else {
                    PermissionDecisionSource::Mode
                },
            ),
            PermissionOutcome::Denied => PermissionDecision::deny(
                request.clone(),
                evaluation.rationale.clone(),
                if evaluation.rationale.contains("sandbox profile") {
                    PermissionDecisionSource::Sandbox
                } else if evaluation.matched_rule.is_some() {
                    PermissionDecisionSource::PolicyRule
                } else {
                    PermissionDecisionSource::Mode
                },
            ),
            PermissionOutcome::Escalated => {
                let prompt = PermissionPromptRequest {
                    request: request.clone(),
                    message: format!(
                        "Permission required for {} on {}",
                        request
                            .tool_name
                            .clone()
                            .unwrap_or_else(|| request.scope_string()),
                        request.target
                    ),
                    reasons: evaluation.reasons.clone(),
                };
                let prompt_decision = self.prompter.prompt(&prompt)?;
                let allowed = matches!(
                    prompt_decision,
                    PermissionPromptDecision::AllowOnce | PermissionPromptDecision::AllowAlways
                );
                let mut decision = if allowed {
                    PermissionDecision::allow(
                        request.clone(),
                        "prompt approved the request",
                        PermissionDecisionSource::Prompt,
                    )
                } else {
                    PermissionDecision::deny(
                        request.clone(),
                        "prompt denied the request",
                        PermissionDecisionSource::Prompt,
                    )
                };
                decision.prompt = Some(PermissionPromptRecord {
                    prompt,
                    decision: prompt_decision,
                });
                decision
            }
        };

        decision.matched_rule = evaluation.matched_rule;
        decision.reasons = if decision.prompt.is_some() {
            let mut reasons = evaluation.reasons;
            reasons.push(decision.rationale.clone());
            reasons
        } else {
            evaluation.reasons
        };
        Ok(self.record(decision))
    }

    fn record(&mut self, decision: PermissionDecision) -> PermissionDecision {
        self.state.last_decision = Some(decision.clone());
        self.state.history.push(decision.clone());
        decision
    }
}

impl PermissionRequest {
    fn scope_string(&self) -> String {
        format!("{:?}", self.scope).to_lowercase()
    }
}

impl<P> PermissionResolver for PermissionEnforcer<P>
where
    P: PermissionPrompter,
{
    fn decide(
        &mut self,
        config: &RuntimeConfig,
        request: PermissionRequest,
    ) -> Result<PermissionDecision, ConversationRuntimeError> {
        self.decide_inner(config, request)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{
        bash::{BashCommand, BashExecutionPlan},
        config::RuntimeConfig,
        permissions::{PermissionLevel, PermissionMode, PermissionScope},
        policy_engine::{PolicyEffect, PolicyRule},
        sandbox::SandboxPolicy,
    };

    fn config(mode: PermissionMode) -> RuntimeConfig {
        RuntimeConfig {
            permission_mode: mode,
            sandbox_policy: SandboxPolicy {
                enabled: false,
                ..SandboxPolicy::default()
            },
            ..RuntimeConfig::default()
        }
    }

    fn request(scope: PermissionScope) -> PermissionRequest {
        PermissionRequest {
            scope,
            target: "workspace/file.txt".to_string(),
            reason: "exercise policy".to_string(),
            tool_name: Some("write_file".to_string()),
            minimum_level: PermissionLevel::Low,
            bash_plan: None,
            metadata: BTreeMap::new(),
        }
    }

    #[test]
    fn allows_requests_in_allow_all_mode() {
        let mut enforcer = PermissionEnforcer::new();

        let decision = enforcer
            .decide(
                &config(PermissionMode::AllowAll),
                request(PermissionScope::Read),
            )
            .expect("allow-all decision");

        assert!(decision.allowed);
        assert_eq!(decision.outcome, PermissionOutcome::Allowed);
        assert_eq!(decision.source, PermissionDecisionSource::Mode);
    }

    #[test]
    fn denies_requests_in_deny_all_mode() {
        let mut enforcer = PermissionEnforcer::new();

        let decision = enforcer
            .decide(
                &config(PermissionMode::DenyAll),
                request(PermissionScope::Read),
            )
            .expect("deny-all decision");

        assert!(!decision.allowed);
        assert_eq!(decision.outcome, PermissionOutcome::Denied);
        assert_eq!(decision.source, PermissionDecisionSource::Mode);
    }

    #[test]
    fn config_override_rule_can_allow_specific_tool() {
        let mut config = config(PermissionMode::DenyAll);
        config.permission_policy.active_rules.push(PolicyRule {
            rule_id: "allow-write-file".to_string(),
            tool_name: Some("write_file".to_string()),
            effect: PolicyEffect::Allow,
            rationale: "config override allows write_file".to_string(),
            ..PolicyRule::default()
        });
        let mut enforcer = PermissionEnforcer::new();

        let decision = enforcer
            .decide(&config, request(PermissionScope::Write))
            .expect("override decision");

        assert!(decision.allowed);
        assert_eq!(decision.matched_rule.as_deref(), Some("allow-write-file"));
        assert_eq!(decision.source, PermissionDecisionSource::PolicyRule);
    }

    #[test]
    fn shell_validation_deny_is_separate_from_policy_choice() {
        let mut enforcer = PermissionEnforcer::new();
        let invalid_request = PermissionRequest {
            bash_plan: Some(BashExecutionPlan {
                commands: vec![BashCommand {
                    program: String::new(),
                    args: Vec::new(),
                    working_dir: None,
                }],
                require_clean_environment: false,
            }),
            ..request(PermissionScope::Execute)
        };

        let decision = enforcer
            .decide(&config(PermissionMode::AllowAll), invalid_request)
            .expect("validation decision");

        assert!(!decision.allowed);
        assert_eq!(decision.source, PermissionDecisionSource::Validation);
        assert!(decision
            .reasons
            .iter()
            .any(|reason| reason.contains("validation error")));
    }

    #[test]
    fn prompting_records_prompt_and_result() {
        let mut config = config(PermissionMode::Ask);
        config
            .permission_policy
            .tool_minimum_levels
            .insert("write_file".to_string(), PermissionLevel::Sensitive);
        let prompter = RecordingPrompter::with_decisions(vec![PermissionPromptDecision::AllowOnce]);
        let mut enforcer = PermissionEnforcer::with_prompter(prompter);

        let decision = enforcer
            .decide(&config, request(PermissionScope::Write))
            .expect("prompt decision");
        let (state, prompter) = enforcer.into_parts();

        assert!(decision.allowed);
        assert_eq!(decision.source, PermissionDecisionSource::Prompt);
        assert!(decision.prompt.is_some());
        assert_eq!(state.history.len(), 1);
        assert_eq!(prompter.prompts.len(), 1);
        assert!(prompter.prompts[0].message.contains("write_file"));
    }
}
