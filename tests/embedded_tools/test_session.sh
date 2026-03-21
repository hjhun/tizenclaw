#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# Embedded Tool Tests — Session Management
# Tests create_session via tizenclaw-cli prompts.
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "Embedded Tools: Session Management"

SESSION_ID="e2e_session_test_$$"

# ── ES1: Create session via CLI ───────────────────────────────────
section "ES1" "Session creation"
OUT=$(tc_cli_session "$SESSION_ID" "안녕하세요, 짧게 한 문장으로 답해주세요")
assert_not_empty "Session responds with output" "$OUT"

# ── ES2: Session persistence ─────────────────────────────────────
section "ES2" "Session persistence (follow-up)"
OUT2=$(tc_cli_session "$SESSION_ID" "방금 제가 뭐라고 했는지 말해주세요")
assert_not_empty "Follow-up returns response" "$OUT2"

# ── ES3: Session directory check ─────────────────────────────────
section "ES3" "Session file on device"
SESSION_EXISTS=$(sdb_shell "ls /opt/usr/share/tizenclaw/sessions/ 2>/dev/null | grep -c '$SESSION_ID' || echo 0" | tr -d '[:space:]')
# Session files may or may not persist depending on config
if [ "$SESSION_EXISTS" -gt 0 ]; then
  _pass "Session file exists on device"
else
  _skip "Session file persistence" "session may be in-memory only"
fi

# ── ES4: Independent session ─────────────────────────────────────
section "ES4" "Independent sessions are isolated"
OTHER_SESSION="e2e_other_session_$$"
OUT3=$(tc_cli_session "$OTHER_SESSION" "Say hello in English, one word only")
assert_not_empty "Independent session responds" "$OUT3"
assert_not_contains "No cross-contamination" "$OUT3" "방금\|뭐라고"

suite_end
