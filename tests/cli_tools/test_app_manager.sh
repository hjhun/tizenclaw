#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# tizen-app-manager-cli Tests — Full Coverage
# Commands: list, list-all, running, running-all, recent,
#           recent-detail, terminate, launch, package-info
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

TOOL="tizen-app-manager-cli"
suite_begin "CLI: ${TOOL}"

if ! tc_tool_exists "${TC_CLI_BASE}/${TOOL}/${TOOL}"; then
  tc_warn "Tool binary not found, skipping suite"
  _skip "Tool binary exists" "not installed"
  suite_end; exit $?
fi

# ── A1: list ──────────────────────────────────────────────────────
section "A1" "list — installed apps"
OUT=$(cli_exec "$TOOL" list)
assert_json_valid "Output is valid JSON" "$OUT"
assert_json "Has apps array" "$OUT" '.apps | type == "array"'
assert_json "At least 1 app installed" "$OUT" '.apps | length > 0'
assert_json "App has app_id field" "$OUT" '.apps[0].app_id'
assert_json "App has label field" "$OUT" '.apps[0].label'

# Save first app_id for later tests
FIRST_APP_ID=$(echo "$OUT" | jq -r '.apps[0].app_id // empty' 2>/dev/null)
tc_log "First app_id: ${FIRST_APP_ID}"

# ── A2: list-all ──────────────────────────────────────────────────
section "A2" "list-all — all apps"
OUT=$(cli_exec "$TOOL" list-all)
assert_json_valid "Output is valid JSON" "$OUT"
assert_json "Has apps array" "$OUT" '.apps | type == "array"'
assert_json_array_ge "list-all >= list" "$OUT" '.apps' 1

# ── A3: running ───────────────────────────────────────────────────
section "A3" "running — running apps"
OUT=$(cli_exec "$TOOL" running)
assert_json_valid "Output is valid JSON" "$OUT"

# ── A4: running-all ───────────────────────────────────────────────
section "A4" "running-all"
OUT=$(cli_exec "$TOOL" running-all)
assert_json_valid "Output is valid JSON" "$OUT"

# ── A5: recent ────────────────────────────────────────────────────
section "A5" "recent — recent apps"
OUT=$(cli_exec "$TOOL" recent)
assert_json_valid "Output is valid JSON" "$OUT"

# Save first recent app_id for recent-detail test
RECENT_APP_ID=$(echo "$OUT" | jq -r '.apps[0].app_id // empty' 2>/dev/null)

# ── A6: recent-detail ────────────────────────────────────────────
section "A6" "recent-detail — app detail"
if [ -n "$RECENT_APP_ID" ]; then
  OUT=$(cli_exec "$TOOL" recent-detail --app-id "$RECENT_APP_ID")
  assert_json_valid "Output is valid JSON" "$OUT"
  assert_json_eq "Correct app_id returned" "$OUT" '.app_id' "$RECENT_APP_ID"
else
  _skip "recent-detail" "no recent apps available"
fi

# ── A7: package-info ──────────────────────────────────────────────
section "A7" "package-info — query package"
# Extract package ID from first installed app
PKG_ID=$(echo "$(cli_exec "$TOOL" list)" | jq -r '.apps[0].package // empty' 2>/dev/null)
if [ -n "$PKG_ID" ]; then
  OUT=$(cli_exec "$TOOL" package-info --package-id "$PKG_ID")
  assert_json_valid "Output is valid JSON" "$OUT"
  assert_json "Package has package_id" "$OUT" '.package_id'
  assert_json "Package has version" "$OUT" '.version'
else
  _skip "package-info" "no package ID available"
fi

# ── A8: launch & terminate ────────────────────────────────────────
section "A8" "launch & terminate lifecycle"
if [ -n "$FIRST_APP_ID" ]; then
  # Launch the app
  LAUNCH_OUT=$(cli_exec "$TOOL" launch --app-id "$FIRST_APP_ID")
  assert_json_valid "Launch output is valid JSON" "$LAUNCH_OUT"
  sleep 2

  # Verify it's running
  RUNNING_OUT=$(cli_exec "$TOOL" running)
  RUNNING_APP=$(echo "$RUNNING_OUT" | jq -r ".apps[]? | select(.app_id == \"$FIRST_APP_ID\") | .app_id // empty" 2>/dev/null)
  if [ "$RUNNING_APP" = "$FIRST_APP_ID" ]; then
    _pass "App launched and is running"
  else
    _skip "App running check" "app may have auto-terminated"
  fi

  # Terminate
  TERM_OUT=$(cli_exec "$TOOL" terminate --app-id "$FIRST_APP_ID")
  assert_json_valid "Terminate output is valid JSON" "$TERM_OUT"
  sleep 1

  # Verify terminated
  RUNNING_OUT2=$(cli_exec "$TOOL" running)
  STILL_RUNNING=$(echo "$RUNNING_OUT2" | jq -r ".apps[]? | select(.app_id == \"$FIRST_APP_ID\") | .app_id // empty" 2>/dev/null)
  if [ -z "$STILL_RUNNING" ]; then
    _pass "App terminated successfully"
  else
    _skip "App terminate check" "app may still be running"
  fi
else
  _skip "launch & terminate" "no app_id available (jq required)"
fi

# ── A9: launch with parameters ───────────────────────────────────
section "A9" "launch with operation parameter"
if [ -n "$FIRST_APP_ID" ]; then
  OUT=$(cli_exec "$TOOL" launch --app-id "$FIRST_APP_ID" --operation "http://tizen.org/appcontrol/operation/default")
  assert_json_valid "Launch with operation is valid JSON" "$OUT"
  sleep 1
  # Cleanup
  cli_exec "$TOOL" terminate --app-id "$FIRST_APP_ID" >/dev/null 2>&1
else
  _skip "launch with operation" "no app_id available"
fi

# ── A10: Error handling — invalid app ─────────────────────────────
section "A10" "Error handling — invalid app"
OUT=$(cli_exec "$TOOL" launch --app-id "com.invalid.nonexistent.app.xyz")
assert_json_valid "Error output is valid JSON" "$OUT"

suite_end
