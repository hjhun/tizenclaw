#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# Embedded Tool Tests — Workflow CRUD
# Tests create/list/delete workflow operations via tizenclaw-cli.
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "Embedded Tools: Workflow CRUD"

SESSION_ID="e2e_workflow_test_$$"
WF_NAME="e2e_test_workflow_$$"

# ── W1: List workflows (baseline) ────────────────────────────────
section "W1" "List workflows (baseline)"
OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the list_workflows tool to show the list of registered workflows")
assert_not_empty "list_workflows returns output" "$OUT"

# ── W2: Create workflow ──────────────────────────────────────────
section "W2" "Create workflow"
CREATE_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the create_workflow tool to create the following workflow:
---
name: ${WF_NAME}
description: E2E test workflow for automated testing
trigger: manual
---
## Step 1: Greet
- type: prompt
- instruction: Say hello
- output_var: greeting")
assert_not_empty "create_workflow returns output" "$CREATE_OUT"

# ── W3: Verify workflow appears in list ───────────────────────────
section "W3" "Verify workflow in list"
LIST_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the list_workflows tool to list all workflows")
assert_not_empty "list_workflows returns output" "$LIST_OUT"
# Check daemon logs for tool execution
TOOL_LOG=$(sdb_shell dlogutil -d TIZENCLAW 2>/dev/null \
  | grep -i "workflow\|list_workflows" | tail -5)
assert_not_empty "Daemon log shows workflow activity" "$TOOL_LOG"

# ── W4: Delete workflow ──────────────────────────────────────────
section "W4" "Delete workflow"
DELETE_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the delete_workflow tool to delete the workflow named '${WF_NAME}'")
assert_not_empty "delete_workflow returns output" "$DELETE_OUT"

suite_end
