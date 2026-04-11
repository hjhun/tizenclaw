//! Agent Loop State — 15-phase autonomous agent execution state machine.
//!
//! Each active session carries an `AgentLoopState` that tracks the current
//! phase, planning context, token budget utilization, and self-inspection
//! metrics. This state machine drives `AgentCore::process_prompt()`.
//!
//! ## Phases
//! 1.  GoalParsing        — Interpret user intent, extract entities
//! 2.  ContextLoading     — Load session history + memory retrieval
//! 3.  Planning           — Decompose goal into actionable plan steps
//! 4.  DecisionMaking     — Select next tool/skill with confidence score
//! 5.  ToolDispatching    — Execute selected tool or skill
//! 6.  ObservationCollect — Normalize and record tool results
//! 7.  Evaluating         — Assess progress: achieved / partial / stuck
//! 8.  RePlanning         — Revise plan if goal not met
//! 9.  TerminationCheck   — Evaluate loop exit conditions
//! 10. ErrorRecovery      — Handle tool or LLM failures with retries
//! 11. SafetyCheck        — Permission & policy gate before execution
//! 12. StateTracking      — Persist loop metadata to session store
//! 13. SelfInspection     — Token budget & loop health monitoring
//! 14. ResultReporting    — Format and finalize agent output
//! 15. Complete           — Loop exited; session archived

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Instant;

/// The 15 phases of the TizenClaw autonomous agent loop.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentPhase {
    // Pre-loop phases
    GoalParsing,
    ContextLoading,
    Planning,
    // Main loop phases
    DecisionMaking,
    SafetyCheck,
    ToolDispatching,
    ObservationCollect,
    Evaluating,
    RePlanning,
    ErrorRecovery,
    StateTracking,
    SelfInspection,
    TerminationCheck,
    // Exit phases
    ResultReporting,
    Complete,
}

impl AgentPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentPhase::GoalParsing => "GoalParsing",
            AgentPhase::ContextLoading => "ContextLoading",
            AgentPhase::Planning => "Planning",
            AgentPhase::DecisionMaking => "DecisionMaking",
            AgentPhase::SafetyCheck => "SafetyCheck",
            AgentPhase::ToolDispatching => "ToolDispatching",
            AgentPhase::ObservationCollect => "ObservationCollect",
            AgentPhase::Evaluating => "Evaluating",
            AgentPhase::RePlanning => "RePlanning",
            AgentPhase::ErrorRecovery => "ErrorRecovery",
            AgentPhase::StateTracking => "StateTracking",
            AgentPhase::SelfInspection => "SelfInspection",
            AgentPhase::TerminationCheck => "TerminationCheck",
            AgentPhase::ResultReporting => "ResultReporting",
            AgentPhase::Complete => "Complete",
        }
    }
}

/// Evaluation verdict after assessing LLM response progress.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalVerdict {
    NotStarted,
    GoalAchieved,    // LLM claims task done, no pending tool calls
    PartialProgress, // Tool calls executed, goal not yet confirmed
    Stuck,           // Same output repeated N times (idle loop)
    Failed,          // Unrecoverable error
}

impl EvalVerdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            EvalVerdict::NotStarted => "NotStarted",
            EvalVerdict::GoalAchieved => "GoalAchieved",
            EvalVerdict::PartialProgress => "PartialProgress",
            EvalVerdict::Stuck => "Stuck",
            EvalVerdict::Failed => "Failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoopTransitionReason {
    LoopInitialized,
    ToolCallsRequested,
    IdleRecovery,
    WorkflowStepAdvance,
    FileActionRequired,
    FileTargetsMissing,
    PersistedOutputsMissing,
    GoalAchieved,
    NoBackendConfigured,
    StuckLoopAbort,
    RoundLimitReached,
}

impl LoopTransitionReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            LoopTransitionReason::LoopInitialized => "loop_initialized",
            LoopTransitionReason::ToolCallsRequested => "tool_calls_requested",
            LoopTransitionReason::IdleRecovery => "idle_recovery",
            LoopTransitionReason::WorkflowStepAdvance => "workflow_step_advance",
            LoopTransitionReason::FileActionRequired => "file_action_required",
            LoopTransitionReason::FileTargetsMissing => "file_targets_missing",
            LoopTransitionReason::PersistedOutputsMissing => "persisted_outputs_missing",
            LoopTransitionReason::GoalAchieved => "goal_achieved",
            LoopTransitionReason::NoBackendConfigured => "no_backend_configured",
            LoopTransitionReason::StuckLoopAbort => "stuck_loop_abort",
            LoopTransitionReason::RoundLimitReached => "round_limit_reached",
        }
    }
}

/// Per-session state carried across all agent loop iterations.
///
/// `Send + Sync`: all fields are owned values or standard primitives.
/// Stored per session_id in `AgentCore::loop_states`.
pub struct AgentLoopState {
    pub session_id: String,
    pub phase: AgentPhase,
    pub original_goal: String,

    // Planning
    pub plan_steps: Vec<String>,
    pub current_step: usize,

    // Loop counters
    pub round: usize,
    pub error_count: usize,
    pub tool_retry_count: usize,
    pub max_tool_rounds: usize, // 0 disables the round cap

    // Workflow execution mode
    pub active_workflow_id: Option<String>,
    pub current_workflow_step: usize,
    pub workflow_vars: std::collections::HashMap<String, Value>,

    // Evaluation
    pub last_eval_verdict: EvalVerdict,
    pub recent_outputs: Vec<String>, // for idle/stuck detection (window=3)

    // Token budget (size-based compaction)
    pub token_budget: usize, // 0 disables automatic compaction
    pub token_used: usize,
    pub compact_threshold: f32, // 0.90 default when budget > 0

    // Observation
    pub last_observation: Option<Value>,
    pub needs_follow_up: bool,
    pub last_transition_reason: String,
    pub last_transition_detail: String,
    pub last_prefetch_memory: Option<String>,
    pub last_prefetch_skills: Vec<String>,

    // Error recovery
    pub last_error: Option<String>,

    // Self-inspection telemetry
    pub started_at: Instant,
    pub total_tool_calls: usize,

    // Fallback strategy telemetry
    pub stuck_retry_count: usize,
    pub tool_budget_events: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentLoopSnapshot {
    pub session_id: String,
    pub phase: String,
    pub original_goal: String,
    pub plan_step_count: usize,
    pub current_step: usize,
    pub round: usize,
    pub error_count: usize,
    pub tool_retry_count: usize,
    pub max_tool_rounds: usize,
    pub last_eval_verdict: String,
    pub needs_follow_up: bool,
    pub last_transition_reason: String,
    pub last_transition_detail: String,
    pub last_error: Option<String>,
    pub total_tool_calls: usize,
    pub stuck_retry_count: usize,
    pub tool_budget_events: usize,
    pub active_workflow_id: Option<String>,
    pub current_workflow_step: usize,
}

impl AgentLoopState {
    pub const DEFAULT_TOKEN_BUDGET: usize = 0;
    pub const DEFAULT_COMPACT_THRESHOLD: f32 = 0.90;
    pub const DEFAULT_MAX_TOOL_ROUNDS: usize = 0;
    /// Idle detection window: if last N outputs are identical → Stuck
    pub const IDLE_WINDOW: usize = 3;

    pub fn new(session_id: &str, goal: &str) -> Self {
        AgentLoopState {
            session_id: session_id.to_string(),
            phase: AgentPhase::GoalParsing,
            original_goal: goal.to_string(),
            plan_steps: Vec::new(),
            current_step: 0,
            round: 0,
            error_count: 0,
            tool_retry_count: 0,
            max_tool_rounds: Self::DEFAULT_MAX_TOOL_ROUNDS,
            last_eval_verdict: EvalVerdict::NotStarted,
            recent_outputs: Vec::new(),
            token_budget: Self::DEFAULT_TOKEN_BUDGET,
            token_used: 0,
            compact_threshold: Self::DEFAULT_COMPACT_THRESHOLD,
            last_observation: None,
            needs_follow_up: false,
            last_transition_reason: LoopTransitionReason::LoopInitialized.as_str().to_string(),
            last_transition_detail: String::new(),
            last_prefetch_memory: None,
            last_prefetch_skills: Vec::new(),
            last_error: None,
            started_at: Instant::now(),
            total_tool_calls: 0,
            stuck_retry_count: 0,
            tool_budget_events: 0,
            active_workflow_id: None,
            current_workflow_step: 0,
            workflow_vars: std::collections::HashMap::new(),
        }
    }

    /// Override token budget and threshold from config.
    pub fn with_budget(mut self, budget: usize, threshold: f32) -> Self {
        self.token_budget = budget;
        self.compact_threshold = threshold;
        self
    }

    /// Transition to a new phase and log the transition via dlog.
    pub fn transition(&mut self, next: AgentPhase) {
        log::debug!(
            "[AgentLoop] Session '{}' round {} | {} → {}",
            self.session_id,
            self.round,
            self.phase.as_str(),
            next.as_str()
        );
        self.phase = next;
    }

    pub fn set_follow_up(&mut self, value: bool) {
        self.needs_follow_up = value;
    }

    pub fn record_transition(
        &mut self,
        reason: LoopTransitionReason,
        detail: impl Into<String>,
    ) {
        self.last_transition_reason = reason.as_str().to_string();
        self.last_transition_detail = detail.into();
    }

    pub fn mark_follow_up(
        &mut self,
        reason: LoopTransitionReason,
        detail: impl Into<String>,
    ) {
        self.needs_follow_up = true;
        self.record_transition(reason, detail);
    }

    pub fn mark_terminal(
        &mut self,
        reason: LoopTransitionReason,
        detail: impl Into<String>,
    ) {
        self.needs_follow_up = false;
        self.record_transition(reason, detail);
    }

    pub fn record_prefetch_memory(&mut self, preview: Option<String>) {
        self.last_prefetch_memory = preview;
    }

    pub fn record_prefetch_skills(&mut self, skills: Vec<String>) {
        self.last_prefetch_skills = skills;
    }

    pub fn record_budget_events(&mut self, count: usize) {
        self.tool_budget_events += count;
    }

    pub fn snapshot(&self) -> AgentLoopSnapshot {
        AgentLoopSnapshot {
            session_id: self.session_id.clone(),
            phase: self.phase.as_str().to_string(),
            original_goal: self.original_goal.clone(),
            plan_step_count: self.plan_steps.len(),
            current_step: self.current_step,
            round: self.round,
            error_count: self.error_count,
            tool_retry_count: self.tool_retry_count,
            max_tool_rounds: self.max_tool_rounds,
            last_eval_verdict: self.last_eval_verdict.as_str().to_string(),
            needs_follow_up: self.needs_follow_up,
            last_transition_reason: self.last_transition_reason.clone(),
            last_transition_detail: self.last_transition_detail.clone(),
            last_error: self.last_error.clone(),
            total_tool_calls: self.total_tool_calls,
            stuck_retry_count: self.stuck_retry_count,
            tool_budget_events: self.tool_budget_events,
            active_workflow_id: self.active_workflow_id.clone(),
            current_workflow_step: self.current_workflow_step,
        }
    }

    /// Returns true if the token budget is at or above the compaction threshold.
    pub fn needs_compaction(&self) -> bool {
        if self.token_budget == 0 {
            return false;
        }
        let ratio = self.token_used as f32 / self.token_budget as f32;
        ratio >= self.compact_threshold
    }

    /// Returns true if the loop has reached the maximum allowed rounds.
    pub fn is_round_limit_reached(&self) -> bool {
        if self.max_tool_rounds == 0 {
            return false;
        }
        self.round >= self.max_tool_rounds
    }

    /// Record a new output for idle/stuck detection.
    /// Returns `EvalVerdict::Stuck` if the last IDLE_WINDOW outputs are identical.
    pub fn observe_output(&mut self, output: &str) -> EvalVerdict {
        self.recent_outputs.push(output.to_string());
        if self.recent_outputs.len() > Self::IDLE_WINDOW {
            self.recent_outputs.remove(0);
        }
        if self.recent_outputs.len() == Self::IDLE_WINDOW {
            let first = &self.recent_outputs[0];
            if self.recent_outputs.iter().all(|o| o == first) {
                self.last_eval_verdict = EvalVerdict::Stuck;
                return EvalVerdict::Stuck;
            }
        }
        EvalVerdict::PartialProgress
    }

    /// Log self-inspection telemetry via dlog.
    pub fn log_self_inspection(&self) {
        let elapsed = self.started_at.elapsed().as_secs();
        let token_pct = if self.token_budget > 0 {
            (self.token_used as f64 / self.token_budget as f64 * 100.0) as u32
        } else {
            0
        };
        log::debug!(
            "[SelfInspection] session='{}' round={} phase={} \
             tokens={}/{} ({}%) tools={} errors={} follow_up={} \
             transition={} prefetched_skills={} memory_prefetched={} budgeted_results={} \
             elapsed={}s",
            self.session_id,
            self.round,
            self.phase.as_str(),
            self.token_used,
            self.token_budget,
            token_pct,
            self.total_tool_calls,
            self.error_count,
            self.needs_follow_up,
            self.last_transition_reason,
            self.last_prefetch_skills.len(),
            self.last_prefetch_memory.is_some(),
            self.tool_budget_events,
            elapsed,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_state_defaults() {
        let s = AgentLoopState::new("sess1", "Turn on lights");
        assert_eq!(s.phase, AgentPhase::GoalParsing);
        assert_eq!(s.round, 0);
        assert_eq!(s.token_budget, AgentLoopState::DEFAULT_TOKEN_BUDGET);
        assert_eq!(
            s.compact_threshold,
            AgentLoopState::DEFAULT_COMPACT_THRESHOLD
        );
        assert_eq!(s.last_eval_verdict, EvalVerdict::NotStarted);
    }

    #[test]
    fn test_transition_updates_phase() {
        let mut s = AgentLoopState::new("sess1", "goal");
        s.transition(AgentPhase::ContextLoading);
        assert_eq!(s.phase, AgentPhase::ContextLoading);
    }

    #[test]
    fn test_needs_compaction_at_90_percent() {
        let mut s = AgentLoopState::new("sess1", "goal");
        s.token_budget = 100;
        s.token_used = 89;
        assert!(!s.needs_compaction());
        s.token_used = 90;
        assert!(s.needs_compaction());
        s.token_used = 100;
        assert!(s.needs_compaction());
    }

    #[test]
    fn test_needs_compaction_zero_budget() {
        let mut s = AgentLoopState::new("sess1", "goal");
        s.token_budget = 0;
        s.token_used = 1000;
        assert!(!s.needs_compaction());
    }

    #[test]
    fn test_round_limit() {
        let mut s = AgentLoopState::new("sess1", "goal");
        s.max_tool_rounds = 5;
        s.round = 4;
        assert!(!s.is_round_limit_reached());
        s.round = 5;
        assert!(s.is_round_limit_reached());
    }

    #[test]
    fn test_round_limit_is_disabled_when_zero() {
        let mut s = AgentLoopState::new("sess1", "goal");
        s.max_tool_rounds = 0;
        s.round = 999;
        assert!(!s.is_round_limit_reached());
    }

    #[test]
    fn test_observe_output_stuck_detection() {
        let mut s = AgentLoopState::new("sess1", "goal");
        assert_ne!(s.observe_output("same"), EvalVerdict::Stuck);
        assert_ne!(s.observe_output("same"), EvalVerdict::Stuck);
        assert_eq!(s.observe_output("same"), EvalVerdict::Stuck);
    }

    #[test]
    fn test_observe_output_not_stuck_if_different() {
        let mut s = AgentLoopState::new("sess1", "goal");
        s.observe_output("a");
        s.observe_output("b");
        assert_ne!(s.observe_output("c"), EvalVerdict::Stuck);
    }

    #[test]
    fn test_with_budget_override() {
        let s = AgentLoopState::new("s", "g").with_budget(64_000, 0.85);
        assert_eq!(s.token_budget, 64_000);
        assert!((s.compact_threshold - 0.85).abs() < f32::EPSILON);
    }

    #[test]
    fn test_phase_as_str_all_variants() {
        let phases = [
            AgentPhase::GoalParsing,
            AgentPhase::ContextLoading,
            AgentPhase::Planning,
            AgentPhase::DecisionMaking,
            AgentPhase::SafetyCheck,
            AgentPhase::ToolDispatching,
            AgentPhase::ObservationCollect,
            AgentPhase::Evaluating,
            AgentPhase::RePlanning,
            AgentPhase::ErrorRecovery,
            AgentPhase::StateTracking,
            AgentPhase::SelfInspection,
            AgentPhase::TerminationCheck,
            AgentPhase::ResultReporting,
            AgentPhase::Complete,
        ];
        for p in &phases {
            assert!(!p.as_str().is_empty());
        }
    }

    #[test]
    fn test_eval_verdict_as_str() {
        assert_eq!(EvalVerdict::GoalAchieved.as_str(), "GoalAchieved");
        assert_eq!(EvalVerdict::Stuck.as_str(), "Stuck");
        assert_eq!(EvalVerdict::Failed.as_str(), "Failed");
    }

    #[test]
    fn test_follow_up_and_prefetch_tracking() {
        let mut s = AgentLoopState::new("sess1", "goal");
        s.mark_follow_up(LoopTransitionReason::ToolCallsRequested, "tool follow-up");
        s.record_prefetch_memory(Some("memory preview".into()));
        s.record_prefetch_skills(vec!["skill_a".into(), "skill_b".into()]);
        s.record_budget_events(2);

        assert!(s.needs_follow_up);
        assert_eq!(s.last_transition_reason, "tool_calls_requested");
        assert_eq!(s.last_transition_detail, "tool follow-up");
        assert_eq!(s.last_prefetch_memory.as_deref(), Some("memory preview"));
        assert_eq!(s.last_prefetch_skills.len(), 2);
        assert_eq!(s.tool_budget_events, 2);
    }

    #[test]
    fn test_snapshot_reflects_runtime_state() {
        let mut s = AgentLoopState::new("sess1", "goal");
        s.plan_steps = vec!["plan".into(), "execute".into()];
        s.current_step = 1;
        s.round = 2;
        s.error_count = 1;
        s.tool_retry_count = 3;
        s.max_tool_rounds = 7;
        s.last_eval_verdict = EvalVerdict::PartialProgress;
        s.mark_follow_up(
            LoopTransitionReason::WorkflowStepAdvance,
            "workflow advanced",
        );
        s.last_error = Some("temporary failure".into());
        s.total_tool_calls = 4;
        s.stuck_retry_count = 1;
        s.tool_budget_events = 2;
        s.active_workflow_id = Some("wf-1".into());
        s.current_workflow_step = 5;

        let snapshot = s.snapshot();

        assert_eq!(snapshot.session_id, "sess1");
        assert_eq!(snapshot.phase, "GoalParsing");
        assert_eq!(snapshot.plan_step_count, 2);
        assert_eq!(snapshot.current_step, 1);
        assert_eq!(snapshot.round, 2);
        assert_eq!(snapshot.error_count, 1);
        assert_eq!(snapshot.tool_retry_count, 3);
        assert_eq!(snapshot.max_tool_rounds, 7);
        assert_eq!(snapshot.last_eval_verdict, "PartialProgress");
        assert!(snapshot.needs_follow_up);
        assert_eq!(snapshot.last_transition_reason, "workflow_step_advance");
        assert_eq!(snapshot.last_transition_detail, "workflow advanced");
        assert_eq!(snapshot.last_error.as_deref(), Some("temporary failure"));
        assert_eq!(snapshot.total_tool_calls, 4);
        assert_eq!(snapshot.stuck_retry_count, 1);
        assert_eq!(snapshot.tool_budget_events, 2);
        assert_eq!(snapshot.active_workflow_id.as_deref(), Some("wf-1"));
        assert_eq!(snapshot.current_workflow_step, 5);
    }

    #[test]
    fn test_mark_terminal_clears_follow_up() {
        let mut s = AgentLoopState::new("sess1", "goal");
        s.mark_follow_up(LoopTransitionReason::ToolCallsRequested, "tools pending");
        s.mark_terminal(LoopTransitionReason::GoalAchieved, "done");

        assert!(!s.needs_follow_up);
        assert_eq!(s.last_transition_reason, "goal_achieved");
        assert_eq!(s.last_transition_detail, "done");
    }
}
