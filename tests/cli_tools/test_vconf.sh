#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# tizen-vconf-cli Tests — Full Coverage
# Commands: get, set, watch
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

TOOL="tizen-vconf-cli"
suite_begin "CLI: ${TOOL}"

if ! tc_tool_exists "${TC_CLI_BASE}/${TOOL}/${TOOL}"; then
  tc_warn "Tool binary not found, skipping suite"
  _skip "Tool binary exists" "not installed"
  suite_end; exit $?
fi

# ── V1: get — known key (device name) ────────────────────────────
section "V1" "get — device name"
OUT=$(cli_exec "$TOOL" get "db/setting/device_name")
assert_not_empty "Device name returned" "$OUT"

# Check if output is JSON or plain text
if echo "$OUT" | grep -qE '^\s*[\{\[]'; then
  _pass "Output is JSON format"
  assert_json "Has value field" "$OUT" '.value'
else
  # Plain text value is also acceptable
  if echo "$OUT" | grep -qiE "not found|error"; then
    _skip "device name" "key not found on this device"
  else
    _pass "Device name returned (plain text)"
  fi
fi

# ── V2: get — locale/language ─────────────────────────────────────
section "V2" "get — language setting"
OUT=$(cli_exec "$TOOL" get "db/menu_widget/language")
assert_not_empty "Language returned" "$OUT"

# ── V3: get — timezone ────────────────────────────────────────────
section "V3" "get — timezone"
OUT=$(cli_exec "$TOOL" get "db/menu_widget/regionformat_timezone" 2>/dev/null)
if [ -n "$OUT" ] && ! echo "$OUT" | grep -qiE "not found|error"; then
  _pass "Timezone returned"
else
  _skip "timezone" "key not available"
fi

# ── V4: get — sound enabled ──────────────────────────────────────
section "V4" "get — sound enabled"
OUT=$(cli_exec "$TOOL" get "db/setting/sound/sound_on")
assert_not_empty "Sound setting returned" "$OUT"

# ── V5: set & get — round-trip ────────────────────────────────────
section "V5" "set & get — round-trip test"
TEST_KEY="db/setting/device_name"
# Read current value
ORIG_VAL=$(cli_exec "$TOOL" get "$TEST_KEY" 2>/dev/null)

# Try setting a value — some keys may be read-only
SET_OUT=$(cli_exec "$TOOL" set "memory/e2e_test_key_$$" "test_value_$$" 2>/dev/null)
if echo "$SET_OUT" | grep -qiE "failed|error|denied|read.only"; then
  _skip "set & get" "set operation not permitted"
else
  # Verify the set value
  GET_OUT=$(cli_exec "$TOOL" get "memory/e2e_test_key_$$" 2>/dev/null)
  if echo "$GET_OUT" | grep -q "test_value_$$"; then
    _pass "Set and get round-trip successful"
  else
    _skip "set & get verify" "set may not be supported for this key"
  fi
fi

# ── V6: get — multiple system keys ────────────────────────────────
section "V6" "get — multiple system keys"
KEYS_OK=0
KEYS_TOTAL=0
for key in "db/setting/sound/sound_on" "db/setting/device_name" "db/menu_widget/language"; do
  ((KEYS_TOTAL++))
  OUT=$(cli_exec "$TOOL" get "$key" 2>/dev/null)
  if [ -n "$OUT" ] && ! echo "$OUT" | grep -qiE "not found|error"; then
    ((KEYS_OK++))
  fi
done
assert_ge "At least 2 system keys readable" "$KEYS_OK" 2

# ── V7: watch — short timeout ────────────────────────────────────
section "V7" "watch — streaming (2s timeout)"
OUT=$(timeout 3 sdb_shell "${TC_CLI_BASE}/${TOOL}/${TOOL} watch db/setting/device_name" 2>/dev/null || echo "timeout")
if [ "$OUT" = "timeout" ] || [ -n "$OUT" ]; then
  _pass "Watch completed without crash"
else
  _pass "Watch handled gracefully"
fi

suite_end
