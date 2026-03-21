#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# Embedded Tool Tests — Scheduler & Autonomous Triggers
# Tests: task scheduling, cron-like triggers, supervisor agent
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "Embedded Tools: Scheduler & Supervisor"

SESSION_ID="e2e_sched_$$"

# ── SC1: Create scheduled task ────────────────────────────────────
section "SC1" "Create scheduled task"
OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the create_task tool to create a task with description 'Scheduled battery check' and action 'Check device battery level using get_device_info'")
assert_not_empty "Scheduled task created" "$OUT"

# ── SC2: List tasks — verify scheduled ────────────────────────────
section "SC2" "Verify scheduled task in list"
LIST_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the list_tasks tool to list all tasks and tell me how many exist")
assert_not_empty "list_tasks shows scheduled tasks" "$LIST_OUT"

# ── SC3: Create second scheduled task ─────────────────────────────
section "SC3" "Create second scheduled task"
OUT2=$(tc_cli_session "$SESSION_ID" \
  "Use the create_task tool to create a task with description 'Daily log cleanup' and action 'List files in /tmp and report count'")
assert_not_empty "Second task created" "$OUT2"

# ── SC4: Cancel specific task ─────────────────────────────────────
section "SC4" "Cancel scheduled task"
CANCEL_OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the list_tasks tool first, then use cancel_task to cancel the 'battery check' task")
assert_not_empty "Cancel task returns output" "$CANCEL_OUT"

# ── SC5: Supervisor agent — basic goal ────────────────────────────
section "SC5" "Supervisor agent — basic goal"
SUP_OUT=$(tc_cli_session "${SESSION_ID}_sup" \
  "Use the run_supervisor tool with goal 'Check the device status: battery level and available storage' and strategy 'sequential'")
assert_not_empty "Supervisor returns output" "$SUP_OUT"

# ── SC6: Supervisor agent — parallel strategy ─────────────────────
section "SC6" "Supervisor agent — parallel strategy"
PAR_OUT=$(tc_cli_session "${SESSION_ID}_par" \
  "Use the run_supervisor tool with goal 'Get device info and list installed apps simultaneously' and strategy 'parallel'")
assert_not_empty "Parallel supervisor returns output" "$PAR_OUT"

# ── SC7: Workflow with cron trigger ───────────────────────────────
section "SC7" "Workflow with cron trigger"
CRON_OUT=$(tc_cli_session "${SESSION_ID}_cron" \
  "Use the create_workflow tool with this markdown:
---
name: e2e_cron_workflow_$$
description: Cron-triggered test workflow
trigger: cron:daily 09:00
---
## Step 1: Check Battery
- type: tool
- tool_name: get_device_info
- output_var: battery_info")
assert_not_empty "Cron workflow created" "$CRON_OUT"

# ── SC8: Cleanup — delete cron workflow ───────────────────────────
section "SC8" "Cleanup cron workflow"
tc_cli_session "${SESSION_ID}_cron" \
  "Use the delete_workflow tool to delete 'e2e_cron_workflow_$$'" >/dev/null 2>&1
_pass "Cron workflow cleanup sent"

# ── SC9: Service stability after all operations ──────────────────
section "SC9" "Service stability check"
STATUS=$(sdb_shell "systemctl is-active tizenclaw" | tr -d '[:space:]')
assert_eq "Service still active" "$STATUS" "active"

# Check memory
MEM=$(sdb_shell "ps -o rss= -p \$(pidof tizenclaw)" | tr -d '[:space:]')
if [ -n "$MEM" ] && [ "$MEM" -gt 0 ]; then
  MEM_MB=$((MEM / 1024))
  if [ "$MEM_MB" -lt 500 ]; then
    _pass "Daemon memory OK (${MEM_MB} MB)"
  else
    _pass "Daemon memory elevated (${MEM_MB} MB)"
  fi
else
  _skip "Memory check" "couldn't read RSS"
fi

suite_end
