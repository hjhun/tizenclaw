#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# tizen-control-display-cli Tests
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

TOOL="tizen-control-display-cli"
suite_begin "CLI: ${TOOL}"

if ! tc_tool_exists "${TC_CLI_BASE}/${TOOL}/${TOOL}"; then
  _skip "Tool binary exists" "not installed"
  suite_end; exit $?
fi

# ── B1: info ──────────────────────────────────────────────────────
section "B1" "display info"
OUT=$(cli_exec "$TOOL" --info)
assert_json_valid "Valid JSON" "$OUT"
assert_json "Has brightness field" "$OUT" '.brightness != null or .current != null'
assert_json "Has max brightness" "$OUT" '.max_brightness != null or .max != null'

# ── B2: set brightness ───────────────────────────────────────────
section "B2" "set brightness"
# Save current
CURRENT_BRIGHTNESS=$(echo "$OUT" | jq -r '.brightness // .current // 50' 2>/dev/null)

# Set to a safe mid value
SET_OUT=$(cli_exec "$TOOL" --brightness 50)
assert_json_valid "Set response valid" "$SET_OUT"

# Restore
cli_exec "$TOOL" --brightness "$CURRENT_BRIGHTNESS" >/dev/null 2>&1

suite_end
