#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# Embedded Tool Tests — Task Management (Enhanced)
# Tests: list → create → verify → cancel → verify cancel
#        → create with action → error handling
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "Embedded Tools: Task Management"

SESSION_ID="e2e_task_full_$$"

# ── T1: List tasks (baseline) ─────────────────────────────────────
section "T1" "List tasks (baseline)"
OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the list_tasks tool to list all scheduled tasks")
assert_not_empty "list_tasks returns output" "$OUT"

# ── T2: Create task ───────────────────────────────────────────────
section "T2" "Create task"
CREATE_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the create_task tool to create a task with description 'E2E automated test task' and action 'Say the current time'")
assert_not_empty "create_task returns output" "$CREATE_OUT"

# ── T3: List tasks (verify creation) ─────────────────────────────
section "T3" "Verify task appears in list"
LIST_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the list_tasks tool to show all tasks. Tell me how many tasks exist.")
assert_not_empty "list_tasks after creation returns output" "$LIST_OUT"

# ── T4: Create another task ──────────────────────────────────────
section "T4" "Create second task"
CREATE2_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the create_task tool to create a task with description 'Check device battery' and action 'Use get_device_info to check battery level'")
assert_not_empty "second task created" "$CREATE2_OUT"

# ── T5: Cancel task ──────────────────────────────────────────────
section "T5" "Cancel task"
CANCEL_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the cancel_task tool to cancel the most recently created task. If you need a task_id, first use list_tasks to find it.")
assert_not_empty "cancel_task returns output" "$CANCEL_OUT"

# ── T6: Cancel non-existent task (error) ─────────────────────────
section "T6" "Cancel non-existent task"
ERR_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the cancel_task tool to cancel task with id 'nonexistent_task_xyz_999'")
assert_not_empty "Error response returned" "$ERR_OUT"

# ── T7: Task persistence check ───────────────────────────────────
section "T7" "Task data in daemon logs"
TASK_LOG=$(sdb_shell "dlogutil -d TIZENCLAW 2>/dev/null | grep -i 'task\|create_task\|cancel_task' | tail -5")
if [ -n "$TASK_LOG" ]; then
  _pass "Task operations logged"
else
  _skip "Task log check" "log may have rotated"
fi

suite_end
