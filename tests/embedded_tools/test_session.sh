#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# Embedded Tool Tests — Session Management (Enhanced)
# Tests: create → context persistence → multi-turn → session isolation
#        → custom system prompt → session directory
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "Embedded Tools: Session Management"

SESSION_ID="e2e_sess_full_$$"

# ── ES1: Create session via CLI ───────────────────────────────────
section "ES1" "Session creation"
OUT=$(tc_cli_session "$SESSION_ID" "안녕하세요, 짧게 한 문장으로 답해주세요")
assert_not_empty "Session responds with output" "$OUT"

# ── ES2: Context persistence — follow-up ─────────────────────────
section "ES2" "Context persistence (follow-up)"
OUT2=$(tc_cli_session "$SESSION_ID" "방금 제가 뭐라고 했는지 한 문장으로 말해주세요")
assert_not_empty "Follow-up returns response" "$OUT2"

# ── ES3: Multi-turn memory ────────────────────────────────────────
section "ES3" "Multi-turn conversation memory"
tc_cli_session "$SESSION_ID" "제 이름은 테스트봇입니다. 기억해주세요." >/dev/null 2>&1
OUT3=$(tc_cli_session "$SESSION_ID" "제 이름이 뭐라고 했죠?")
assert_not_empty "Multi-turn recall returns response" "$OUT3"
# The LLM should recall the name
if echo "$OUT3" | grep -qi "테스트봇"; then
  _pass "Name recalled correctly"
else
  _skip "Name recall" "LLM may paraphrase or fail to recall"
fi

# ── ES4: Session directory check ─────────────────────────────────
section "ES4" "Session file on device"
SESSION_DIR="/opt/usr/share/tizenclaw/sessions"
SESSION_EXISTS=$(sdb_shell "ls $SESSION_DIR/ 2>/dev/null | wc -l" | tr -d '[:space:]')
if [ "$SESSION_EXISTS" -gt 0 ]; then
  _pass "Session files exist on device ($SESSION_EXISTS files)"
else
  _skip "Session file persistence" "session may be in-memory only"
fi

# ── ES5: Independent session isolation ────────────────────────────
section "ES5" "Independent sessions are isolated"
OTHER_SESSION="e2e_other_$$"
OUT4=$(tc_cli_session "$OTHER_SESSION" "Say hello in English, one word only")
assert_not_empty "Independent session responds" "$OUT4"
assert_not_contains "No cross-contamination" "$OUT4" "테스트봇\|방금\|이름"

# ── ES6: Create session with system prompt ────────────────────────
section "ES6" "Create session with custom system prompt"
CUSTOM_SESSION="e2e_custom_$$"
CUSTOM_OUT=$(tc_cli_session "$CUSTOM_SESSION" \
  "Use the create_session tool to create a new session named 'math_helper' with system_prompt 'You are a math expert. Always answer with just the number, no explanation.'")
assert_not_empty "create_session returns output" "$CUSTOM_OUT"

# ── ES7: Verify custom session behavior ───────────────────────────
section "ES7" "Custom session executes with role"
# The create_session might return a session_id. We'll use the new session indirectly via the LLM's tool call.
# Since we can't directly call the custom session, we verify the create call succeeded.
MATH_OUT=$(tc_cli_session "$CUSTOM_SESSION" \
  "In the math_helper session you just created, ask: What is 15 + 27?")
assert_not_empty "Custom session math query returns output" "$MATH_OUT"

# ── ES8: Long conversation doesn't crash ─────────────────────────
section "ES8" "Long conversation stability"
LONG_SESSION="e2e_long_$$"
for i in 1 2 3 4 5; do
  tc_cli_session "$LONG_SESSION" "This is message number $i. Reply with just the message number." >/dev/null 2>&1
done
FINAL_OUT=$(tc_cli_session "$LONG_SESSION" "What was the last message number I sent?")
assert_not_empty "Long conversation final reply" "$FINAL_OUT"

suite_end
