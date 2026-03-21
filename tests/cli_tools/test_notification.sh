#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# tizen-notification-cli Tests — Full Coverage
# Commands: notify, alarm
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

TOOL="tizen-notification-cli"
suite_begin "CLI: ${TOOL}"

if ! tc_tool_exists "${TC_CLI_BASE}/${TOOL}/${TOOL}"; then
  tc_warn "Tool binary not found, skipping suite"
  _skip "Tool binary exists" "not installed"
  suite_end; exit $?
fi

# ── NT1: notify — basic notification ──────────────────────────────
section "NT1" "notify — basic notification"
OUT=$(cli_exec "$TOOL" notify --title "E2E Test" --body "Automated test notification $$")
assert_json_valid "Output is valid JSON" "$OUT"
assert_not_contains "No error" "$OUT" "error.*occurred\|failed to"

# ── NT2: notify — Korean text ─────────────────────────────────────
section "NT2" "notify — Korean text"
OUT=$(cli_exec "$TOOL" notify --title "테스트 알림" --body "자동화 테스트 알림입니다")
assert_json_valid "Output is valid JSON" "$OUT"

# ── NT3: notify — long body ───────────────────────────────────────
section "NT3" "notify — long body text"
LONG_BODY="This is an automated test notification with a longer body text to verify that the notification system handles extended content properly without truncation or errors."
OUT=$(cli_exec "$TOOL" notify --title "Long Test" --body "$LONG_BODY")
assert_json_valid "Output is valid JSON" "$OUT"

# ── NT4: alarm — schedule future alarm ────────────────────────────
section "NT4" "alarm — schedule alarm"
# Schedule alarm 1 hour in the future
FUTURE=$(sdb_shell "date -d '+1 hour' '+%Y-%m-%dT%H:%M:%S' 2>/dev/null" | tr -d '[:space:]')
if [ -n "$FUTURE" ] && [ "$FUTURE" != "" ]; then
  # Get a valid app-id
  APP_ID=$(sdb_shell "${TC_CLI_BASE}/tizen-app-manager-cli/tizen-app-manager-cli list 2>/dev/null" \
    | jq -r '.apps[0].app_id // empty' 2>/dev/null)
  if [ -n "$APP_ID" ]; then
    OUT=$(cli_exec "$TOOL" alarm --app-id "$APP_ID" --datetime "$FUTURE")
    assert_json_valid "alarm output is valid JSON" "$OUT"
  else
    _skip "alarm" "no app-id available (jq required)"
  fi
else
  _skip "alarm" "cannot compute future datetime"
fi

suite_end
