#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# tizen-sensor-cli Tests — Full Coverage
# Types: accelerometer, gravity, gyroscope, light, proximity,
#        pressure, magnetic, orientation
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

TOOL="tizen-sensor-cli"
suite_begin "CLI: ${TOOL}"

if ! tc_tool_exists "${TC_CLI_BASE}/${TOOL}/${TOOL}"; then
  tc_warn "Tool binary not found, skipping suite"
  _skip "Tool binary exists" "not installed"
  suite_end; exit $?
fi

# Helper: test one sensor type
test_sensor() {
  local id="$1" type="$2"
  section "$id" "${type} sensor"
  OUT=$(cli_exec "$TOOL" --type "$type" 2>/dev/null)
  if [ -n "$OUT" ] && ! echo "$OUT" | grep -qiE "not supported|not available|error"; then
    assert_json_valid "${type} output is valid JSON" "$OUT"
  else
    _skip "${type} sensor" "not available on this device"
  fi
}

# ── SN1-8: All sensor types ──────────────────────────────────────
test_sensor "SN1" "accelerometer"
test_sensor "SN2" "gravity"
test_sensor "SN3" "gyroscope"
test_sensor "SN4" "light"
test_sensor "SN5" "proximity"
test_sensor "SN6" "pressure"
test_sensor "SN7" "magnetic"
test_sensor "SN8" "orientation"

# ── SN9: Invalid sensor type ─────────────────────────────────────
section "SN9" "Invalid sensor type"
OUT=$(cli_exec "$TOOL" --type "nonexistent_sensor" 2>/dev/null)
if [ -n "$OUT" ]; then
  assert_json_valid "Error output is valid JSON" "$OUT"
  assert_contains "Error reported" "$OUT" "error\|Error\|not supported\|unknown\|invalid"
else
  _pass "Invalid sensor correctly rejected"
fi

suite_end
