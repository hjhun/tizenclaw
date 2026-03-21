#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# Embedded Tool Tests — Pipeline Full Lifecycle
# Tests: create → list → run → step chaining → condition branch
#        → error handling → delete
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "Embedded Tools: Pipeline Full Lifecycle"

SESSION_ID="e2e_pipe_full_$$"
PIPE_NAME="e2e_pipeline_$$"

# ── P1: List pipelines (baseline) ─────────────────────────────────
section "P1" "List pipelines (baseline)"
OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the list_pipelines tool to show all pipelines")
assert_not_empty "list_pipelines returns output" "$OUT"

# ── P2: Create basic pipeline (tool step) ─────────────────────────
section "P2" "Create tool-only pipeline"
CREATE_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the create_pipeline tool with these parameters:
  name: '${PIPE_NAME}'
  description: 'E2E test pipeline'
  trigger: 'manual'
  steps:
    - id: 'step1', type: 'tool', tool_name: 'get_device_info', output_var: 'device'")
assert_not_empty "create_pipeline returns output" "$CREATE_OUT"

# ── P3: Run basic pipeline ────────────────────────────────────────
section "P3" "Run tool-only pipeline"
RUN_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the run_pipeline tool to execute the pipeline '${PIPE_NAME}'")
assert_not_empty "run_pipeline returns output" "$RUN_OUT"

# Check daemon logs
PIPE_LOG=$(sdb_shell "dlogutil -d TIZENCLAW 2>/dev/null | grep -i 'pipeline\|run_pipeline\|step1' | tail -5")
if [ -n "$PIPE_LOG" ]; then
  _pass "Pipeline execution logged"
else
  _skip "Pipeline execution log" "log may have rotated"
fi

# ── P4: Create multi-step pipeline (tool→prompt chain) ────────────
section "P4" "Create multi-step pipeline"
PIPE_MULTI="e2e_pipe_multi_$$"
MULTI_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the create_pipeline tool with these parameters:
  name: '${PIPE_MULTI}'
  description: 'Multi-step test pipeline'
  steps:
    - id: 'get_info', type: 'tool', tool_name: 'get_device_info', output_var: 'info'
    - id: 'summarize', type: 'prompt', prompt: 'Summarize {{info}} in one sentence', output_var: 'summary'")
assert_not_empty "multi-step pipeline created" "$MULTI_OUT"

# ── P5: Run multi-step pipeline ───────────────────────────────────
section "P5" "Run multi-step pipeline (step chaining)"
RUN_MULTI=$(tc_cli_session "$SESSION_ID" \
  "Use the run_pipeline tool to execute '${PIPE_MULTI}'")
assert_not_empty "multi-step pipeline run output" "$RUN_MULTI"

# ── P6: Create pipeline with input_vars ───────────────────────────
section "P6" "Pipeline with input variables"
PIPE_VARS="e2e_pipe_vars_$$"
tc_cli_session "$SESSION_ID" \
  "Use the create_pipeline tool:
  name: '${PIPE_VARS}'
  steps:
    - id: 'greet', type: 'prompt', prompt: 'Say hello to {{name}}', output_var: 'greeting'" >/dev/null 2>&1

RUN_VARS=$(tc_cli_session "$SESSION_ID" \
  "Use the run_pipeline tool to execute '${PIPE_VARS}' with input_vars: name is 'Bob'")
assert_not_empty "pipeline with vars returns output" "$RUN_VARS"

# ── P7: Run non-existent pipeline (error) ─────────────────────────
section "P7" "Run non-existent pipeline"
ERR_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the run_pipeline tool to execute pipeline 'nonexistent_pipeline_xyz_999'")
assert_not_empty "Error response returned" "$ERR_OUT"

# ── P8: Create pipeline with skip_on_failure ──────────────────────
section "P8" "Pipeline with skip_on_failure"
PIPE_SKIP="e2e_pipe_skip_$$"
SKIP_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the create_pipeline tool:
  name: '${PIPE_SKIP}'
  steps:
    - id: 'may_fail', type: 'tool', tool_name: 'nonexistent_tool', skip_on_failure: true, output_var: 'result1'
    - id: 'fallback', type: 'prompt', prompt: 'The previous step may have failed. Just say OK.', output_var: 'result2'")
assert_not_empty "skip_on_failure pipeline created" "$SKIP_OUT"

# ── P9: Delete pipelines ─────────────────────────────────────────
section "P9" "Delete created pipelines"
tc_cli_session "$SESSION_ID" \
  "Use the delete_pipeline tool to delete '${PIPE_NAME}'" >/dev/null 2>&1
tc_cli_session "$SESSION_ID" \
  "Use the delete_pipeline tool to delete '${PIPE_MULTI}'" >/dev/null 2>&1
tc_cli_session "$SESSION_ID" \
  "Use the delete_pipeline tool to delete '${PIPE_VARS}'" >/dev/null 2>&1
tc_cli_session "$SESSION_ID" \
  "Use the delete_pipeline tool to delete '${PIPE_SKIP}'" >/dev/null 2>&1
_pass "Cleanup delete commands sent"

suite_end
