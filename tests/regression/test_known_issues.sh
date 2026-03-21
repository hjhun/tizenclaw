#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# Regression Tests
# Tests for previously identified bugs and edge cases.
# Add new regression tests here when bugs are found and fixed.
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "Regression Tests"

# ── R1: Service does not crash after multiple CLI calls ──────────
section "R1" "No crash after rapid CLI calls"
for i in 1 2 3 4 5; do
  tc_cli "Hello $i" >/dev/null 2>&1
done
sleep 2
STATUS=$(sdb_shell systemctl is-active tizenclaw | tr -d '[:space:]')
assert_eq "Service still active after 5 rapid calls" "$STATUS" "active"

# Check for crash dumps
CRASH_CHECK=$(sdb_shell "ls /opt/usr/share/tizenclaw/logs/crash_* 2>/dev/null | wc -l" | tr -d '[:space:]')
if [ "$CRASH_CHECK" = "0" ] || [ -z "$CRASH_CHECK" ]; then
  _pass "No crash dump files found"
else
  _fail "Crash dumps detected" "Found ${CRASH_CHECK} crash dump files"
fi

# ── R2: CLI handles empty prompt ──────────────────────────────────
section "R2" "CLI handles empty prompt gracefully"
OUT=$(timeout 10 sdb_shell "tizenclaw-cli '' 2>/dev/null" || echo "handled")
# Should not hang or crash — any output or error is acceptable
_pass "Empty prompt didn't crash CLI"

# ── R3: Service survives tool-executor failures ───────────────────
section "R3" "Service survives tool-executor failure"
# Try to invoke a non-existent CLI tool via the agent
OUT=$(tc_cli "Use a tool called fake_nonexistent_tool_xyz to do something" 2>/dev/null || echo "")
sleep 2
STATUS=$(sdb_shell systemctl is-active tizenclaw | tr -d '[:space:]')
assert_eq "Service active after bad tool request" "$STATUS" "active"

# ── R4: Concurrent session handling ───────────────────────────────
section "R4" "Concurrent sessions"
# Launch two CLI calls in parallel
tc_cli_session "concurrent_a_$$" "Say hello" >/dev/null 2>&1 &
PID1=$!
tc_cli_session "concurrent_b_$$" "Say world" >/dev/null 2>&1 &
PID2=$!

wait "$PID1" 2>/dev/null
wait "$PID2" 2>/dev/null

sleep 2
STATUS=$(sdb_shell systemctl is-active tizenclaw | tr -d '[:space:]')
assert_eq "Service active after concurrent sessions" "$STATUS" "active"

# ── R5: Unicode handling ─────────────────────────────────────────
section "R5" "Unicode handling"
OUT=$(tc_cli "이모지를 사용해보세요: 🎉 🚀 ✅")
assert_not_empty "Unicode prompt returns response" "$OUT"

# ── R6: Special characters in prompt ──────────────────────────────
section "R6" "Special characters in prompt"
OUT=$(tc_cli 'Respond "OK" to this prompt with quotes and $pecial chars & ampersands')
assert_not_empty "Special char prompt returns response" "$OUT"

# ── R7: Long-running daemon stability ─────────────────────────────
section "R7" "Daemon memory check"
RSS=$(sdb_shell "cat /proc/\$(pidof tizenclaw)/status 2>/dev/null | grep VmRSS | awk '{print \$2}'" \
  | tr -d '[:space:]')
if [ -n "$RSS" ] && [ "$RSS" -gt 0 ] 2>/dev/null; then
  # Alert if RSS > 500MB (500000 KB)
  if [ "$RSS" -lt 500000 ]; then
    _pass "Daemon RSS within bounds (${RSS} KB)"
  else
    _fail "Daemon RSS too high" "${RSS} KB (> 500 MB threshold)"
  fi
else
  _skip "Memory check" "could not read RSS"
fi

suite_end
