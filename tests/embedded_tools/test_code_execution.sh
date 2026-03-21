#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# Embedded Tool Tests — Code Execution
# Tests execute_code tool via tizenclaw-cli.
#
# NOTE: These tests rely on non-deterministic LLM behavior.
# The LLM may or may not invoke execute_code directly.
# We validate that the system returns meaningful responses
# without crashing, rather than exact output matching.
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "Embedded Tools: Code Execution"

SESSION_ID="e2e_code_exec_$$"

# ── C1: Simple Python execution ──────────────────────────────────
section "C1" "Simple Python code execution"
OUT=$(tc_cli_session "$SESSION_ID" \
  'Please execute Python code using the execute_code tool: import json; print(json.dumps({"result": 2+2}))')
assert_not_empty "Code execution returns output" "$OUT"

# ── C2: System info via Python ────────────────────────────────────
section "C2" "System info via Python"
OUT=$(tc_cli_session "${SESSION_ID}_sys" \
  'Please use the execute_code tool to run: import json, os; print(json.dumps({"hostname": os.uname().nodename}))')
assert_not_empty "System info returned" "$OUT"

# ── C3: Error handling ───────────────────────────────────────────
section "C3" "Python error handling"
OUT=$(tc_cli_session "${SESSION_ID}_err" \
  'Please use the execute_code tool to execute this code: raise ValueError("test error")')
assert_not_empty "Error response returned" "$OUT"

suite_end
