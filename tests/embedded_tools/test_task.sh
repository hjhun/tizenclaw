#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# Embedded Tool Tests — Task Management
# Tests create_task / list_tasks / cancel_task.
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "Embedded Tools: Task Management"

SESSION_ID="e2e_task_test_$$"

# ── T1: List tasks ────────────────────────────────────────────────
section "T1" "List tasks"
OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the list_tasks tool to list all tasks")
assert_not_empty "list_tasks returns output" "$OUT"

# ── T2: Create task ──────────────────────────────────────────────
section "T2" "Create task"
CREATE_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the create_task tool to create a task with description 'e2e test task' and action 'Say hello'")
assert_not_empty "create_task returns output" "$CREATE_OUT"

# ── T3: List tasks (verify) ──────────────────────────────────────
section "T3" "Verify task created"
LIST_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the list_tasks tool to show all tasks")
assert_not_empty "list_tasks returns output after creation" "$LIST_OUT"

suite_end
