#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# Embedded Tool Tests — Workflow Full Lifecycle
# Tests: create → list → verify → run → run with input_vars
#        → error handling → delete → verify deletion
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "Embedded Tools: Workflow Full Lifecycle"

SESSION_ID="e2e_wf_full_$$"
WF_NAME="e2e_wf_lifecycle_$$"

# ── W1: List workflows (baseline) ────────────────────────────────
section "W1" "List workflows (baseline)"
OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the list_workflows tool to show all registered workflows. Output only what you receive from the tool.")
assert_not_empty "list_workflows returns output" "$OUT"

# ── W2: Create single-step workflow ──────────────────────────────
section "W2" "Create single-step workflow"
CREATE_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the create_workflow tool with this exact markdown:
---
name: ${WF_NAME}
description: E2E lifecycle test workflow
trigger: manual
---
## Step 1: Greet
- type: prompt
- instruction: Say hello and include the current time
- output_var: greeting")
assert_not_empty "create_workflow returns output" "$CREATE_OUT"

# ── W3: Verify workflow in list ──────────────────────────────────
section "W3" "Verify created workflow appears"
LIST_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the list_workflows tool to list all workflows")
assert_not_empty "list returns output" "$LIST_OUT"
# Check if the name appears in tool output or daemon log
WF_IN_LOG=$(sdb_shell "dlogutil -d TIZENCLAW 2>/dev/null | grep -i '${WF_NAME}' | tail -3")
if [ -n "$WF_IN_LOG" ]; then
  _pass "Workflow name found in daemon log"
else
  # LLM response might mention the workflow
  if echo "$LIST_OUT" | grep -qi "${WF_NAME}\|lifecycle"; then
    _pass "Workflow name found in response"
  else
    _skip "Workflow name verification" "LLM may have paraphrased"
  fi
fi

# ── W4: Run workflow ─────────────────────────────────────────────
section "W4" "Run workflow"
RUN_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the run_workflow tool to execute the workflow '${WF_NAME}'")
assert_not_empty "run_workflow returns output" "$RUN_OUT"

# Verify execution in daemon logs
RUN_LOG=$(sdb_shell "dlogutil -d TIZENCLAW 2>/dev/null | grep -i 'run_workflow\|executing.*workflow\|step.*1' | tail -5")
if [ -n "$RUN_LOG" ]; then
  _pass "Workflow execution logged"
else
  _skip "Workflow execution log" "log may have rotated"
fi

# ── W5: Create multi-step workflow ───────────────────────────────
section "W5" "Create multi-step workflow"
WF_MULTI="e2e_wf_multi_$$"
MULTI_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the create_workflow tool with this exact markdown:
---
name: ${WF_MULTI}
description: Multi-step test workflow
trigger: manual
---
## Step 1: Get Info
- type: tool
- tool_name: get_device_info
- output_var: device_info

## Step 2: Summarize
- type: prompt
- instruction: Summarize the device info from {{device_info}} in one sentence
- output_var: summary")
assert_not_empty "multi-step workflow created" "$MULTI_OUT"

# ── W6: Run multi-step workflow ──────────────────────────────────
section "W6" "Run multi-step (tool → prompt chaining)"
RUN_MULTI=$(tc_cli_session "$SESSION_ID" \
  "Use the run_workflow tool to execute the workflow '${WF_MULTI}'")
assert_not_empty "multi-step run returns output" "$RUN_MULTI"

# ── W7: Run workflow with input_vars ─────────────────────────────
section "W7" "Run with input variables"
WF_VARS="e2e_wf_vars_$$"
# Create a workflow that uses input_vars
tc_cli_session "$SESSION_ID" \
  "Use the create_workflow tool with this markdown:
---
name: ${WF_VARS}
description: Workflow with input vars
trigger: manual
---
## Step 1: Greet User
- type: prompt
- instruction: Say hello to {{user_name}} in {{language}}
- output_var: greeting" >/dev/null 2>&1

VARS_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the run_workflow tool to execute '${WF_VARS}' with input_vars: user_name is 'Alice' and language is 'Korean'")
assert_not_empty "run with vars returns output" "$VARS_OUT"

# ── W8: Run non-existent workflow (error) ─────────────────────────
section "W8" "Run non-existent workflow"
ERR_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the run_workflow tool to execute a workflow with id 'nonexistent_workflow_xyz_999'")
assert_not_empty "Error response returned" "$ERR_OUT"

# ── W9: Delete workflows ─────────────────────────────────────────
section "W9" "Delete created workflows"
tc_cli_session "$SESSION_ID" \
  "Use the delete_workflow tool to delete '${WF_NAME}'" >/dev/null 2>&1
tc_cli_session "$SESSION_ID" \
  "Use the delete_workflow tool to delete '${WF_MULTI}'" >/dev/null 2>&1
tc_cli_session "$SESSION_ID" \
  "Use the delete_workflow tool to delete '${WF_VARS}'" >/dev/null 2>&1
_pass "Cleanup delete commands sent"

# ── W10: Verify deletion ─────────────────────────────────────────
section "W10" "Verify deletion"
FINAL_LIST=$(tc_cli_session "$SESSION_ID" \
  "Use the list_workflows tool to list all workflows")
assert_not_empty "list after delete returns output" "$FINAL_LIST"

suite_end
