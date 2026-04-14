use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    config::RuntimeConfig,
    hooks::{HookPhase, HookSpec},
    permissions::{PermissionDecision, PermissionRequest},
    prompt::PromptAssembly,
    session::{SessionRecord, SessionState},
};

use super::{
    ApiRequest, AssistantEvent, ConversationEvent, ConversationTurnResult, HookContext,
    HookOutcome, HookRunner, HookRuntimeError, ModelResponseEvent, ToolCallRequest,
    ToolExecutor, ToolFailure, TurnSummary, TurnUsageReport,
};
use super::helpers::{
    apply_compaction, assistant_message, build_summary_text, emit, normalize_summary_text,
    tool_error_message, tool_permission_level, tool_permission_scope, tool_success_message,
};

#[derive(Debug, Error, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ModelError {
    #[error("model transport failed: {message}")]
    Transport { message: String },
}

#[derive(Debug, Error, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConversationRuntimeError {
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error(transparent)]
    Hook(#[from] HookRuntimeError),
    #[error("conversation exceeded the configured request limit of {max_model_requests}")]
    MaxModelRequestsExceeded { max_model_requests: usize },
    #[error("permission resolution failed: {message}")]
    Permission { message: String },
    #[error("conversation invariant violated: {message}")]
    Invariant { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationEngineOptions {
    pub max_model_requests: usize,
}

impl Default for ConversationEngineOptions {
    fn default() -> Self {
        Self {
            max_model_requests: 8,
        }
    }
}

pub trait ModelTransport {
    fn stream(&mut self, request: &ApiRequest) -> Result<Vec<ModelResponseEvent>, ModelError>;
}

pub trait PermissionResolver {
    fn decide(
        &mut self,
        config: &RuntimeConfig,
        request: PermissionRequest,
    ) -> Result<PermissionDecision, ConversationRuntimeError>;
}

pub struct ConversationEngine<M, T, P, H> {
    config: RuntimeConfig,
    hooks: Vec<HookSpec>,
    options: ConversationEngineOptions,
    model: M,
    tools: T,
    permissions: P,
    hook_runner: H,
}

impl<M, T, P, H> ConversationEngine<M, T, P, H>
where
    M: ModelTransport,
    T: ToolExecutor,
    P: PermissionResolver,
    H: HookRunner,
{
    pub fn new(
        config: RuntimeConfig,
        hooks: Vec<HookSpec>,
        model: M,
        tools: T,
        permissions: P,
        hook_runner: H,
    ) -> Self {
        Self {
            config,
            hooks,
            options: ConversationEngineOptions::default(),
            model,
            tools,
            permissions,
            hook_runner,
        }
    }

    pub fn with_options(mut self, options: ConversationEngineOptions) -> Self {
        self.options = options;
        self
    }

    pub fn prepare_request(
        &self,
        session: &SessionRecord,
        prompt: &PromptAssembly,
        request_index: usize,
    ) -> ApiRequest {
        ApiRequest {
            session_id: session.session_id.clone(),
            request_index,
            prompt: prompt.clone(),
            prompt_text: prompt.render(),
            messages: session.messages.clone(),
            available_tools: self.tools.definitions(),
            metadata: BTreeMap::from([
                (
                    "profile".to_string(),
                    format!("{:?}", self.config.profile).to_lowercase(),
                ),
                (
                    "permission_mode".to_string(),
                    format!("{:?}", self.config.permission_mode).to_lowercase(),
                ),
                (
                    "hooks_enabled".to_string(),
                    self.config.hooks_enabled.to_string(),
                ),
            ]),
        }
    }

    pub fn run_turn<F>(
        &mut self,
        session: &mut SessionRecord,
        prompt: &PromptAssembly,
        mut observer: F,
    ) -> Result<ConversationTurnResult, ConversationRuntimeError>
    where
        F: FnMut(&ConversationEvent),
    {
        let mut events = Vec::new();
        let mut usage = TurnUsageReport::default();
        let mut tool_names = Vec::new();
        let mut request_count = 0usize;
        let mut compacted = false;
        let final_assistant_text = loop {
            session.set_state(SessionState::Active);
            if request_count >= self.options.max_model_requests {
                return Err(ConversationRuntimeError::MaxModelRequestsExceeded {
                    max_model_requests: self.options.max_model_requests,
                });
            }

            let mut request = self.prepare_request(session, prompt, request_count);
            self.run_hooks(
                HookPhase::PrePrompt,
                request_count,
                session,
                None,
                None,
                Some(&mut request.metadata),
                &mut events,
                &mut observer,
            )?;

            emit(
                &mut events,
                &mut observer,
                ConversationEvent::RequestPrepared {
                    request: request.clone(),
                },
            );

            let model_events = self.model.stream(&request)?;
            let (assistant_text, tool_calls) =
                self.consume_model_events(&model_events, &mut usage, &mut events, &mut observer);

            if !assistant_text.is_empty() || !tool_calls.is_empty() {
                let assistant_message =
                    assistant_message(assistant_text.clone(), &tool_calls, usage.latest().cloned());
                session.push_message(assistant_message.clone());
                emit(
                    &mut events,
                    &mut observer,
                    ConversationEvent::Assistant {
                        event: AssistantEvent::Completed {
                            message: assistant_message,
                        },
                    },
                );
            }

            if tool_calls.is_empty() {
                break assistant_text;
            }

            let available_tools = &request.available_tools;
            for call in tool_calls {
                tool_names.push(call.name.clone());
                let permission = PermissionRequest {
                    scope: tool_permission_scope(available_tools, &call.name),
                    target: call.name.clone(),
                    reason: format!("execute tool call {}", call.id),
                    tool_name: Some(call.name.clone()),
                    minimum_level: tool_permission_level(available_tools, &call.name),
                    bash_plan: None,
                    metadata: BTreeMap::new(),
                };
                let decision =
                    self.permissions
                        .decide(&self.config, permission)
                        .map_err(|error| ConversationRuntimeError::Permission {
                            message: error.to_string(),
                        })?;
                session.permission_history.push(decision.clone());
                emit(
                    &mut events,
                    &mut observer,
                    ConversationEvent::PermissionResolved {
                        decision: decision.clone(),
                    },
                );

                if !decision.allowed {
                    let failure = ToolFailure {
                        tool_call_id: call.id.clone(),
                        name: call.name.clone(),
                        message: decision.rationale.clone(),
                        recoverable: true,
                    };
                    session.push_message(tool_error_message(&failure));
                    emit(
                        &mut events,
                        &mut observer,
                        ConversationEvent::ToolExecutionFailed { failure },
                    );
                    continue;
                }

                self.run_hooks(
                    HookPhase::PreTool,
                    request_count,
                    session,
                    Some(call.clone()),
                    None,
                    None,
                    &mut events,
                    &mut observer,
                )?;

                emit(
                    &mut events,
                    &mut observer,
                    ConversationEvent::ToolExecutionStarted { call: call.clone() },
                );

                match self.tools.execute(&call) {
                    Ok(result) => {
                        session.push_message(tool_success_message(&call, &result));
                        emit(
                            &mut events,
                            &mut observer,
                            ConversationEvent::ToolExecutionFinished {
                                result: result.clone(),
                            },
                        );
                        self.run_hooks(
                            HookPhase::PostTool,
                            request_count,
                            session,
                            Some(call),
                            None,
                            None,
                            &mut events,
                            &mut observer,
                        )?;
                    }
                    Err(error) => {
                        let failure = ToolFailure {
                            tool_call_id: call.id.clone(),
                            name: call.name.clone(),
                            message: error.to_string(),
                            recoverable: true,
                        };
                        session.push_message(tool_error_message(&failure));
                        emit(
                            &mut events,
                            &mut observer,
                            ConversationEvent::ToolExecutionFailed { failure },
                        );
                        self.run_hooks(
                            HookPhase::PostTool,
                            request_count,
                            session,
                            Some(call),
                            None,
                            None,
                            &mut events,
                            &mut observer,
                        )?;
                    }
                }
            }

            request_count += 1;
        };

        request_count += 1;

        let mut summary = TurnSummary {
            session_id: session.session_id.clone(),
            request_count,
            tool_call_count: tool_names.len(),
            tool_names,
            assistant_text: normalize_summary_text(&final_assistant_text),
            final_message_count: session.messages.len(),
            compacted: false,
            summary: String::new(),
            usage,
        };
        summary.summary = build_summary_text(&summary);

        let hook_outcomes = self.run_hooks_collect(
            HookPhase::PostSession,
            request_count.saturating_sub(1),
            session,
            None,
            Some(summary.clone()),
            &mut events,
            &mut observer,
        )?;
        for outcome in hook_outcomes {
            if let Some(summary_override) = outcome.summary_override {
                summary.summary = normalize_summary_text(&summary_override);
            }
            if let Some(plan) = outcome.compaction {
                if let Some(metadata) = apply_compaction(session, &summary.summary, &plan) {
                    compacted = true;
                    summary.compacted = true;
                    summary.final_message_count = session.messages.len();
                    emit(
                        &mut events,
                        &mut observer,
                        ConversationEvent::CompactionApplied { metadata },
                    );
                }
            }
        }

        if !compacted {
            summary.compacted = false;
        }
        session.set_summary(summary.summary.clone());
        session.set_state(SessionState::Completed);

        emit(
            &mut events,
            &mut observer,
            ConversationEvent::SummaryUpdated {
                summary: summary.clone(),
            },
        );
        emit(
            &mut events,
            &mut observer,
            ConversationEvent::TurnCompleted {
                summary: summary.clone(),
            },
        );

        Ok(ConversationTurnResult { summary, events })
    }

    fn consume_model_events<F>(
        &mut self,
        model_events: &[ModelResponseEvent],
        usage: &mut TurnUsageReport,
        events: &mut Vec<ConversationEvent>,
        observer: &mut F,
    ) -> (String, Vec<ToolCallRequest>)
    where
        F: FnMut(&ConversationEvent),
    {
        let mut assistant_text = String::new();
        let mut tool_calls = Vec::new();

        for event in model_events {
            match event {
                ModelResponseEvent::TextDelta { text } => {
                    assistant_text.push_str(text);
                    emit(
                        events,
                        observer,
                        ConversationEvent::Assistant {
                            event: AssistantEvent::Delta { text: text.clone() },
                        },
                    );
                }
                ModelResponseEvent::ToolCall { call } => {
                    tool_calls.push(call.clone());
                    emit(
                        events,
                        observer,
                        ConversationEvent::Assistant {
                            event: AssistantEvent::ToolCall { call: call.clone() },
                        },
                    );
                }
                ModelResponseEvent::Usage { usage: snapshot } => {
                    usage.record(snapshot.clone());
                    emit(
                        events,
                        observer,
                        ConversationEvent::Assistant {
                            event: AssistantEvent::Usage {
                                usage: snapshot.clone(),
                            },
                        },
                    );
                }
                ModelResponseEvent::Completed => {}
            }
        }

        (assistant_text, tool_calls)
    }

    fn run_hooks<F>(
        &mut self,
        phase: HookPhase,
        request_index: usize,
        session: &SessionRecord,
        tool_call: Option<ToolCallRequest>,
        summary: Option<TurnSummary>,
        request_metadata: Option<&mut BTreeMap<String, String>>,
        events: &mut Vec<ConversationEvent>,
        observer: &mut F,
    ) -> Result<(), ConversationRuntimeError>
    where
        F: FnMut(&ConversationEvent),
    {
        if !self.config.hooks_enabled {
            return Ok(());
        }

        let context = HookContext {
            phase: phase.clone(),
            session_id: session.session_id.clone(),
            request_index,
            tool_call,
            summary,
            message_count: session.messages.len(),
        };
        let mut metadata_target = request_metadata;

        for hook in self
            .hooks
            .iter()
            .filter(|hook| hook.enabled && hook.phase == phase)
        {
            emit(
                events,
                observer,
                ConversationEvent::HookStarted {
                    name: hook.name.clone(),
                    phase: phase.clone(),
                },
            );
            let outcome = self.hook_runner.run(hook, &context)?;
            if let Some(target) = metadata_target.as_deref_mut() {
                for (key, value) in &outcome.metadata {
                    target.insert(key.clone(), value.clone());
                }
            }
            emit(
                events,
                observer,
                ConversationEvent::HookCompleted {
                    name: hook.name.clone(),
                    phase: phase.clone(),
                    outcome,
                },
            );
        }

        Ok(())
    }

    fn run_hooks_collect<F>(
        &mut self,
        phase: HookPhase,
        request_index: usize,
        session: &SessionRecord,
        tool_call: Option<ToolCallRequest>,
        summary: Option<TurnSummary>,
        events: &mut Vec<ConversationEvent>,
        observer: &mut F,
    ) -> Result<Vec<HookOutcome>, ConversationRuntimeError>
    where
        F: FnMut(&ConversationEvent),
    {
        if !self.config.hooks_enabled {
            return Ok(Vec::new());
        }

        let context = HookContext {
            phase: phase.clone(),
            session_id: session.session_id.clone(),
            request_index,
            tool_call,
            summary,
            message_count: session.messages.len(),
        };
        let mut outcomes = Vec::new();

        for hook in self
            .hooks
            .iter()
            .filter(|hook| hook.enabled && hook.phase == phase)
        {
            emit(
                events,
                observer,
                ConversationEvent::HookStarted {
                    name: hook.name.clone(),
                    phase: phase.clone(),
                },
            );
            let outcome = self.hook_runner.run(hook, &context)?;
            emit(
                events,
                observer,
                ConversationEvent::HookCompleted {
                    name: hook.name.clone(),
                    phase: phase.clone(),
                    outcome: outcome.clone(),
                },
            );
            outcomes.push(outcome);
        }

        Ok(outcomes)
    }
}
