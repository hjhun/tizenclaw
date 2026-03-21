#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# Embedded Tool Tests — Pipeline Management
# Tests create/list/run/delete pipeline operations.
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "Embedded Tools: Pipeline Management"

SESSION_ID="e2e_pipeline_test_$$"

# ── P1: List pipelines ────────────────────────────────────────────
section "P1" "List pipelines"
OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the list_pipelines tool to show all registered pipelines")
assert_not_empty "list_pipelines returns output" "$OUT"

# ── P2: Create pipeline ──────────────────────────────────────────
section "P2" "Create pipeline"
CREATE_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the create_pipeline tool to create a pipeline named 'e2e_test_pipeline' with description 'Test pipeline for automated testing' and steps: step 1 is a prompt step that says 'hello world'")
assert_not_empty "create_pipeline returns output" "$CREATE_OUT"

# ── P3: Delete pipeline ──────────────────────────────────────────
section "P3" "Delete pipeline"
DELETE_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the delete_pipeline tool to delete the pipeline named 'e2e_test_pipeline'")
assert_not_empty "delete_pipeline returns output" "$DELETE_OUT"

suite_end
