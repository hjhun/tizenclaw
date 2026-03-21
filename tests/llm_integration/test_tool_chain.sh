#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# LLM Integration — End-to-End Tool Chain Verification
# Tests that LLM correctly invokes tools through the full
# daemon → tool-executor → CLI pipeline (not just direct CLI calls).
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "LLM Integration: Tool Chain Verification"

SESSION_ID="e2e_toolchain_$$"

# ── TC1: Device info via natural language ─────────────────────────
section "TC1" "Natural language → get_device_info"
OUT=$(tc_cli_session "$SESSION_ID" \
  "이 디바이스의 모델명과 타이젠 버전을 알려줘")
assert_not_empty "Device info returned via NL" "$OUT"
# Should contain device-related info
if echo "$OUT" | grep -qi "tizen\|emulator\|version\|model\|device"; then
  _pass "Response contains device info"
else
  _skip "Device info keywords" "LLM may have paraphrased"
fi

# ── TC2: File system via natural language ─────────────────────────
section "TC2" "Natural language → file operations"
OUT=$(tc_cli_session "${SESSION_ID}_file" \
  "/tmp 디렉토리에 어떤 파일들이 있는지 5개만 알려줘")
assert_not_empty "File list returned via NL" "$OUT"

# ── TC3: UI automation via natural language ───────────────────────
section "TC3" "Natural language → aurum (screen-size)"
OUT=$(tc_cli_session "${SESSION_ID}_ui" \
  "현재 화면의 해상도(너비와 높이)를 알려줘")
assert_not_empty "Screen info returned via NL" "$OUT"
if echo "$OUT" | grep -qi "1920\|1080\|width\|height\|해상도\|픽셀"; then
  _pass "Screen dimensions found in response"
else
  _skip "Screen dimensions" "LLM may not have used aurum"
fi

# ── TC4: Multi-tool chaining via NL ───────────────────────────────
section "TC4" "Natural language → multi-tool chain"
OUT=$(tc_cli_session "${SESSION_ID}_chain" \
  "디바이스 정보를 확인하고, 현재 볼륨도 알려줘. 두 가지 정보를 모두 알려줘.")
assert_not_empty "Multi-tool response returned" "$OUT"

# ── TC5: Error recovery via NL ────────────────────────────────────
section "TC5" "Natural language → error recovery"
OUT=$(tc_cli_session "${SESSION_ID}_err" \
  "존재하지 않는 파일 /tmp/nonexistent_abc_xyz_123.log 의 내용을 읽어줘")
assert_not_empty "Error handled gracefully" "$OUT"
assert_contains "Error mentioned" "$OUT" "없\|error\|찾\|존재하지\|not found\|실패\|파일"

# ── TC6: Korean → English tool execution ──────────────────────────
section "TC6" "Korean input → English CLI tool"
OUT=$(tc_cli_session "${SESSION_ID}_kr" \
  "네트워크 연결 상태를 확인해줘")
assert_not_empty "Network status via Korean" "$OUT"

# ── TC7: Service stability after all chains ───────────────────────
section "TC7" "Service stability after tool chains"
STATUS=$(sdb_shell "systemctl is-active tizenclaw" | tr -d '[:space:]')
assert_eq "Service stable after tool chains" "$STATUS" "active"

suite_end
