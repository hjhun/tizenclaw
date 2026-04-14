use serde_json::json;

use crate::{
    compact::CompactionPlan,
    permissions::{PermissionLevel, PermissionScope},
    session::{
        ConversationMessage, SessionCompactionMetadata, SessionContentBlock, SessionMessageRole,
        SessionRecord,
    },
    usage::UsageSnapshot,
};

use super::{
    ConversationEvent, ToolCallRequest, ToolDefinition, ToolExecutionOutput, ToolFailure,
    TurnSummary,
};

pub(super) fn emit<F>(events: &mut Vec<ConversationEvent>, observer: &mut F, event: ConversationEvent)
where
    F: FnMut(&ConversationEvent),
{
    observer(&event);
    events.push(event);
}

pub(super) fn assistant_message(
    assistant_text: String,
    tool_calls: &[ToolCallRequest],
    usage: Option<UsageSnapshot>,
) -> ConversationMessage {
    let mut message = ConversationMessage::new(SessionMessageRole::Assistant);
    if !assistant_text.is_empty() {
        message.content.push(SessionContentBlock::Text {
            text: assistant_text,
        });
    }
    message
        .content
        .extend(tool_calls.iter().map(|call| SessionContentBlock::ToolCall {
            id: call.id.clone(),
            name: call.name.clone(),
            input: call.input.clone(),
        }));
    message.usage = usage;
    message
}

pub(super) fn tool_success_message(
    call: &ToolCallRequest,
    result: &ToolExecutionOutput,
) -> ConversationMessage {
    let mut message = ConversationMessage::new(SessionMessageRole::Tool);
    message.name = Some(call.name.clone());
    if let Some(summary) = &result.summary {
        message.content.push(SessionContentBlock::Text {
            text: summary.clone(),
        });
    }
    message.content.push(SessionContentBlock::ToolResult {
        tool_call_id: result.tool_call_id.clone(),
        output: result.output.clone(),
    });
    message
}

pub(super) fn tool_error_message(failure: &ToolFailure) -> ConversationMessage {
    let mut message = ConversationMessage::new(SessionMessageRole::Tool);
    message.name = Some(failure.name.clone());
    message.content.push(SessionContentBlock::Text {
        text: failure.message.clone(),
    });
    message.content.push(SessionContentBlock::ToolResult {
        tool_call_id: failure.tool_call_id.clone(),
        output: json!({
            "ok": false,
            "error": failure.message.clone(),
            "recoverable": failure.recoverable,
            "tool_name": failure.name.clone(),
        }),
    });
    message
}

pub(super) fn build_summary_text(summary: &TurnSummary) -> String {
    if !summary.assistant_text.is_empty() {
        return summary.assistant_text.clone();
    }

    if !summary.tool_names.is_empty() {
        return format!("Executed tools: {}", summary.tool_names.join(", "));
    }

    format!(
        "Completed {} request(s) with no assistant text",
        summary.request_count
    )
}

pub(super) fn normalize_summary_text(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut summary = collapsed.trim().to_string();
    if summary.len() > 160 {
        summary.truncate(160);
        summary.push_str("...");
    }
    summary
}

pub(super) fn tool_permission_scope(
    definitions: &[ToolDefinition],
    tool_name: &str,
) -> PermissionScope {
    definitions
        .iter()
        .find(|definition| definition.name == tool_name)
        .map(|definition| definition.permission_scope.clone())
        .unwrap_or(PermissionScope::Execute)
}

pub(super) fn tool_permission_level(
    definitions: &[ToolDefinition],
    tool_name: &str,
) -> PermissionLevel {
    definitions
        .iter()
        .find(|definition| definition.name == tool_name)
        .map(|definition| definition.minimum_permission_level)
        .unwrap_or(PermissionLevel::Standard)
}

pub(super) fn apply_compaction(
    session: &mut SessionRecord,
    summary: &str,
    plan: &CompactionPlan,
) -> Option<SessionCompactionMetadata> {
    let source_count = session.messages.len();
    if source_count <= plan.max_items {
        return None;
    }

    let retain_count = plan
        .preserve_latest
        .max(1)
        .min(source_count)
        .min(plan.max_items.max(1));
    let start = source_count.saturating_sub(retain_count);
    session.messages = session.messages[start..].to_vec();

    let metadata = SessionCompactionMetadata {
        compacted_at: Some(plan.target.clone()),
        summary: Some(summary.to_string()),
        source_message_count: source_count,
        retained_message_count: session.messages.len(),
    };
    session.record_compaction(metadata.clone());
    Some(metadata)
}
