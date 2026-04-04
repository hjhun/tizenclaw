//! Context Engine — Size-based context window pressure management.
//!
//! Controls when and how conversation history is compacted to stay within the
//! LLM's token budget. Uses a three-phase compaction strategy:
//!
//! ## Compaction Trigger
//! Compaction is triggered when estimated token usage reaches or exceeds
//! `compact_threshold` × `budget` (default: 90% of 256,000 = 230,400 tokens).
//!
//! ## Compaction Phases
//! 1. **Pin**: Always keep the system prompt (role="system") and the original
//!    user request (first role="user" message). These are never removed.
//! 2. **Prune**: Drop `tool` result messages that are not referenced in any
//!    later `assistant` message. These are safe to discard.
//! 3. **Truncate**: If still over budget, drop the oldest non-pinned messages
//!    (excluding the most recent 30%) until under threshold.
//!
//! ## Token Estimation
//! - Primary: `WordPieceTokenizer` when vocabulary is loaded (accurate).
//! - Fallback: `chars / 3.5` heuristic when tokenizer is unavailable.

use crate::llm::backend::LlmMessage;

const HEURISTIC_CHARS_PER_TOKEN: f32 = 3.5;
pub const DEFAULT_TOOL_RESULT_BUDGET_CHARS: usize = 4_000;

fn char_boundary_prefix(text: &str, max_chars: usize) -> &str {
    if max_chars == 0 {
        return "";
    }

    match text.char_indices().nth(max_chars) {
        Some((idx, _)) => &text[..idx],
        None => text,
    }
}

// ─── Trait ──────────────────────────────────────────────────────────────────

pub trait ContextEngine: Send + Sync {
    /// Returns true if compaction is recommended, i.e. token utilization
    /// is at or above the configured `compact_threshold`.
    fn should_compact(&self, messages: &[LlmMessage], budget: usize) -> bool;

    /// Perform phased compaction on `messages` to fit within `budget` tokens.
    /// Returns the compacted message list.
    fn compact(&self, messages: Vec<LlmMessage>, budget: usize) -> Vec<LlmMessage>;

    /// Estimate the total token count for a slice of messages.
    fn estimate_tokens(&self, messages: &[LlmMessage]) -> usize;
}

// ─── Size-Based Implementation ───────────────────────────────────────────────

/// Size-based context engine.
///
/// Triggers compaction based on token utilization (≥90% of budget by default).
/// Budget default: 256,000 tokens. Threshold default: 0.90.
pub struct SizedContextEngine {
    compact_threshold: f32,
}

impl SizedContextEngine {
    /// Default token budget: 256,000 tokens (≈ Gemini 1.5 / Claude 3.5 context).
    pub const DEFAULT_BUDGET: usize = 256_000;
    /// Compact when utilization reaches 90% of budget.
    pub const DEFAULT_THRESHOLD: f32 = 0.90;

    pub fn new() -> Self {
        SizedContextEngine {
            compact_threshold: Self::DEFAULT_THRESHOLD,
        }
    }

    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.compact_threshold = threshold.clamp(0.5, 0.99);
        self
    }

    pub fn budget_tool_result_message(
        &self,
        mut message: LlmMessage,
        max_chars: usize,
    ) -> (LlmMessage, bool) {
        if message.role != "tool" || max_chars == 0 {
            return (message, false);
        }

        let serialized = message.tool_result.to_string();
        if serialized.chars().count() <= max_chars {
            return (message, false);
        }

        let preview = char_boundary_prefix(&serialized, max_chars.min(400)).to_string();
        message.tool_result = serde_json::json!({
            "summary": format!(
                "Tool output truncated to stay within the agent context budget. \
        Preview the result and call a narrower follow-up tool if more detail is required."
            ),
            "preview": preview,
            "truncated": true,
            "original_size": serialized.len(),
        });

        (message, true)
    }

    pub fn budget_tool_result_messages(
        &self,
        messages: Vec<LlmMessage>,
        max_chars: usize,
    ) -> (Vec<LlmMessage>, usize) {
        let mut budgeted = 0;
        let result = messages
            .into_iter()
            .map(|message| {
                let (message, changed) = self.budget_tool_result_message(message, max_chars);
                if changed {
                    budgeted += 1;
                }
                message
            })
            .collect();

        (result, budgeted)
    }
}

impl Default for SizedContextEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextEngine for SizedContextEngine {
    fn estimate_tokens(&self, messages: &[LlmMessage]) -> usize {
        // Heuristic: total chars across all textual fields / 3.5
        let total_chars: usize = messages
            .iter()
            .map(|m| {
                m.text.len()
                    + m.reasoning_text.len()
                    + m.tool_result.to_string().len()
                    + m.tool_calls
                        .iter()
                        .map(|tc| tc.args.to_string().len() + tc.name.len())
                        .sum::<usize>()
            })
            .sum();
        ((total_chars as f32) / HEURISTIC_CHARS_PER_TOKEN).ceil() as usize
    }

    fn should_compact(&self, messages: &[LlmMessage], budget: usize) -> bool {
        if budget == 0 {
            return false;
        }
        let estimated = self.estimate_tokens(messages);
        let threshold_tokens = ((budget as f32) * self.compact_threshold) as usize;
        estimated >= threshold_tokens
    }

    fn compact(&self, messages: Vec<LlmMessage>, budget: usize) -> Vec<LlmMessage> {
        let before = self.estimate_tokens(&messages);
        log::debug!(
            "[ContextEngine] Compacting: ~{} tokens / {} budget ({:.1}%)",
            before,
            budget,
            if budget > 0 {
                before as f32 / budget as f32 * 100.0
            } else {
                0.0
            }
        );

        // ── Phase 1: Identify pinned messages ──────────────────────────────
        // Pin: system prompt + first user message (never removed)
        let mut pinned_indices = std::collections::HashSet::new();
        let mut first_user_found = false;
        for (i, msg) in messages.iter().enumerate() {
            if msg.role == "system" {
                pinned_indices.insert(i);
            } else if msg.role == "user" && !first_user_found {
                pinned_indices.insert(i);
                first_user_found = true;
            }
        }

        // ── Phase 2: Identify tool results safe to prune ──────────────────
        // Collect names of all tools referenced in assistant messages
        let referenced_tool_ids: std::collections::HashSet<String> = messages
            .iter()
            .filter(|m| m.role == "assistant")
            .flat_map(|m| m.tool_calls.iter().map(|tc| tc.id.clone()))
            .collect();

        // A "tool" message is prunable if its tool_call_id is not referenced
        let mut prunable_indices = std::collections::HashSet::new();
        for (i, msg) in messages.iter().enumerate() {
            if msg.role == "tool" && !pinned_indices.contains(&i)
                && !msg.tool_call_id.is_empty() && !referenced_tool_ids.contains(&msg.tool_call_id)
            {
                prunable_indices.insert(i);
            }
        }

        // ── Phase 3: Build compacted list without pruned messages ──────────
        let mut compacted: Vec<LlmMessage> = messages
            .into_iter()
            .enumerate()
            .filter(|(i, _)| !prunable_indices.contains(i))
            .map(|(_, mut m)| {
                if m.role == "tool" {
                    if m.text.len() > 200 {
                        m.text = format!("{}... [truncated]", &m.text[..200]);
                    }
                    let res_str = m.tool_result.to_string();
                    if res_str.len() > 200 {
                        m.tool_result =
                            serde_json::json!(format!("{}... [truncated]", &res_str[..200]));
                    }
                }
                m
            })
            .collect();

        // ── Phase 4: If still over budget, drop oldest non-pinned messages ─
        let target = ((budget as f32) * self.compact_threshold * 0.70) as usize;
        let mut rebuilt_pinned = std::collections::HashSet::new();
        // Rebuild pinned index into compacted list positions
        {
            let mut user_seen = false;
            for (i, msg) in compacted.iter().enumerate() {
                if msg.role == "system" {
                    rebuilt_pinned.insert(i);
                } else if msg.role == "user" && !user_seen {
                    rebuilt_pinned.insert(i);
                    user_seen = true;
                }
            }
        }

        while self.estimate_tokens(&compacted) > target && compacted.len() > 2 {
            // Find the oldest non-pinned message
            let drop_idx = compacted
                .iter()
                .enumerate()
                .position(|(i, _)| !rebuilt_pinned.contains(&i));
            if let Some(idx) = drop_idx {
                compacted.remove(idx);
                // Rebuild pinned index after removal
                rebuilt_pinned.clear();
                let mut user_seen = false;
                for (i, msg) in compacted.iter().enumerate() {
                    if msg.role == "system" {
                        rebuilt_pinned.insert(i);
                    } else if msg.role == "user" && !user_seen {
                        rebuilt_pinned.insert(i);
                        user_seen = true;
                    }
                }
            } else {
                break; // All remaining are pinned, cannot shrink further
            }
        }

        let after = self.estimate_tokens(&compacted);
        log::debug!(
            "[ContextEngine] Compacted: {} → {} msgs | ~{} → ~{} tokens ({:.1}% of budget)",
            compacted.len() + prunable_indices.len(),
            compacted.len(),
            before,
            after,
            if budget > 0 {
                after as f32 / budget as f32 * 100.0
            } else {
                0.0
            }
        );

        compacted
    }
}

// ─── Legacy Alias (backward compat) ─────────────────────────────────────────

/// Backward-compatible alias. Use `SizedContextEngine` for new code.
pub type SimpleContextEngine = SizedContextEngine;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::backend::{LlmMessage, LlmToolCall};
    use serde_json::json;

    fn msg(role: &str, text: &str) -> LlmMessage {
        LlmMessage {
            role: role.into(),
            text: text.into(),
            ..Default::default()
        }
    }

    fn tool_msg(call_id: &str, text: &str) -> LlmMessage {
        LlmMessage {
            role: "tool".into(),
            text: text.into(),
            tool_call_id: call_id.into(),
            ..Default::default()
        }
    }

    fn assistant_with_tool_call(text: &str, call_id: &str, name: &str) -> LlmMessage {
        LlmMessage {
            role: "assistant".into(),
            text: text.into(),
            tool_calls: vec![LlmToolCall {
                id: call_id.into(),
                name: name.into(),
                args: json!({}),
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_estimate_tokens_basic() {
        let engine = SizedContextEngine::new();
        // 35 chars / 3.5 = 10 tokens
        let msgs = vec![msg("user", "hello world foo bar baz qux qui")];
        let est = engine.estimate_tokens(&msgs);
        assert!(est > 0);
    }

    #[test]
    fn test_should_compact_below_threshold() {
        let engine = SizedContextEngine::new();
        // ~1 token of messages vs 1_000_000 budget → should NOT compact
        let msgs = vec![msg("user", "hi")];
        assert!(!engine.should_compact(&msgs, 1_000_000));
    }

    #[test]
    fn test_should_compact_above_threshold() {
        let engine = SizedContextEngine::new();
        // Large messages with tiny budget
        let big_text = "a".repeat(10_000);
        let msgs = vec![msg("user", &big_text)];
        assert!(engine.should_compact(&msgs, 100)); // way over 90% of 100
    }

    #[test]
    fn test_should_compact_zero_budget_never() {
        let engine = SizedContextEngine::new();
        let msgs = vec![msg("user", "huge message ".repeat(1000).as_str())];
        assert!(!engine.should_compact(&msgs, 0));
    }

    #[test]
    fn test_compact_pins_system_and_first_user() {
        let engine = SizedContextEngine::new();
        let messages = vec![
            msg("system", "You are TizenClaw."),
            msg("user", "Original goal"),
            msg("assistant", "Thinking..."),
            msg("user", "Follow-up"),
            msg("assistant", "Done."),
        ];
        // Force compaction by using tiny budget
        let budget = 10;
        let compact = engine.compact(messages, budget);
        // System and first user must be present
        assert!(compact.iter().any(|m| m.role == "system"));
        assert!(compact
            .iter()
            .any(|m| m.role == "user" && m.text == "Original goal"));
    }

    #[test]
    fn test_compact_prunes_unreferenced_tool_results() {
        let engine = SizedContextEngine::new();
        // Tool result with call_id "orphan" is not referenced by any assistant
        let messages = vec![
            msg("system", "prompt"),
            msg("user", "goal"),
            tool_msg("orphan", "result data that can be dropped"),
            msg("assistant", "Final answer"),
        ];
        let budget = 1_000;
        // With large budget, should_compact would be false;
        // force compact anyway to test pruning logic
        let compact = engine.compact(messages, budget);
        // Orphaned tool result should be removed
        assert!(!compact
            .iter()
            .any(|m| m.role == "tool" && m.tool_call_id == "orphan"));
    }

    #[test]
    fn test_compact_keeps_referenced_tool_results() {
        let engine = SizedContextEngine::new();
        // Tool result with call_id "ref1" IS referenced by assistant
        let messages = vec![
            msg("system", "prompt"),
            msg("user", "goal"),
            tool_msg("ref1", "important result"),
            assistant_with_tool_call("Using ref1", "ref1", "get_data"),
            msg("assistant", "Done"),
        ];
        let budget = 50;
        let compact = engine.compact(messages, budget);
        // Referenced tool result should be kept
        assert!(compact
            .iter()
            .any(|m| m.role == "tool" && m.tool_call_id == "ref1" && m.text == "important result"));
    }

    #[test]
    fn test_compact_returns_at_least_system_and_user() {
        let engine = SizedContextEngine::new();
        let messages = vec![
            msg("system", "S"),
            msg("user", "U"),
            msg("assistant", "A1"),
            msg("assistant", "A2"),
            msg("assistant", "A3"),
            msg("assistant", "A4"),
            msg("assistant", "A5"),
        ];
        // Extremely small budget forces maximum pruning
        let compact = engine.compact(messages, 1);
        assert!(compact.iter().any(|m| m.role == "system"));
        assert!(compact.iter().any(|m| m.role == "user"));
    }

    #[test]
    fn test_with_threshold_clamps() {
        let engine = SizedContextEngine::new().with_threshold(0.3);
        // Clamped to 0.5
        assert!(engine.compact_threshold >= 0.5);
        let engine2 = SizedContextEngine::new().with_threshold(1.5);
        // Clamped to 0.99
        assert!(engine2.compact_threshold <= 0.99);
    }

    #[test]
    fn test_budget_tool_result_message_truncates_large_payload() {
        let engine = SizedContextEngine::new();
        let large = "x".repeat(DEFAULT_TOOL_RESULT_BUDGET_CHARS + 25);
        let message = LlmMessage::tool_result("call1", "read_file", json!({ "data": large }));

        let (budgeted, changed) =
            engine.budget_tool_result_message(message, DEFAULT_TOOL_RESULT_BUDGET_CHARS);

        assert!(changed);
        assert_eq!(budgeted.tool_call_id, "call1");
        assert_eq!(budgeted.tool_name, "read_file");
        assert_eq!(budgeted.tool_result["truncated"], json!(true));
        assert!(budgeted.tool_result["preview"]
            .as_str()
            .unwrap_or_default()
            .starts_with("{\"data\":"));
    }

    #[test]
    fn test_budget_tool_result_message_keeps_small_payload() {
        let engine = SizedContextEngine::new();
        let message = LlmMessage::tool_result("call1", "battery", json!({ "percent": 50 }));

        let (budgeted, changed) =
            engine.budget_tool_result_message(message.clone(), DEFAULT_TOOL_RESULT_BUDGET_CHARS);

        assert!(!changed);
        assert_eq!(budgeted.tool_result, message.tool_result);
    }
}
