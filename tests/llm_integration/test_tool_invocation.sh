#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# LLM Integration Tests — Tool Invocation
# Verifies the LLM correctly invokes device tools when prompted.
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "LLM Integration: Tool Invocation"

SESSION_ID="e2e_tool_invoke_$$"

# ── TI1: Device info tool ────────────────────────────────────────
section "TI1" "get_device_info tool invocation"
OUT=$(tc_cli_session "$SESSION_ID" \
  "get_device_info 도구를 호출해서 결과를 보여주세요")
assert_not_empty "Tool returns result" "$OUT"

# Verify in daemon logs
TOOL_LOG=$(sdb_shell dlogutil -d TIZENCLAW 2>/dev/null \
  | grep -i "get_device_info\|Executing skill" | tail -3)
assert_not_empty "Daemon log shows tool execution" "$TOOL_LOG"

# ── TI2: List workflows tool ─────────────────────────────────────
section "TI2" "list_workflows tool"
OUT=$(tc_cli_session "${SESSION_ID}_wf" \
  "Use the list_workflows tool to show the workflow list")
assert_not_empty "list_workflows returns output" "$OUT"

# ── TI3: File system tool ────────────────────────────────────────
section "TI3" "File list tool"
OUT=$(tc_cli_session "${SESSION_ID}_fs" \
  "Use the appropriate tool to list files in /tmp/ directory")
assert_not_empty "File list returns output" "$OUT"

# ── TI4: Multi-tool prompt ───────────────────────────────────────
section "TI4" "Multi-tool in single prompt"
OUT=$(tc_cli_session "${SESSION_ID}_multi" \
  "먼저 디바이스 배터리 정보를 알려주고, 그 다음 현재 볼륨 레벨도 알려주세요")
assert_not_empty "Multi-tool prompt returns results" "$OUT"

# ── TI5: Tool with parameters ────────────────────────────────────
section "TI5" "Tool with specific parameters"
OUT=$(tc_cli_session "${SESSION_ID}_param" \
  "Use the execute_code tool to run: import json; print(json.dumps({'test': 'ok'}))")
assert_not_empty "Parameterized tool returns output" "$OUT"

suite_end
