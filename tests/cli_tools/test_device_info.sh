#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# tizen-device-info-cli Tests
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

TOOL="tizen-device-info-cli"
suite_begin "CLI: ${TOOL}"

if ! tc_tool_exists "${TC_CLI_BASE}/${TOOL}/${TOOL}"; then
  _skip "Tool binary exists" "not installed"
  suite_end; exit $?
fi

# ── D1: battery ───────────────────────────────────────────────────
section "D1" "battery"
OUT=$(cli_exec "$TOOL" battery)
assert_json_valid "Valid JSON" "$OUT"
assert_json "Has percent field" "$OUT" '.percent != null'
assert_json "Has charging field" "$OUT" '.is_charging != null or .charging != null'

# ── D2: system-info ───────────────────────────────────────────────
section "D2" "system-info"
OUT=$(cli_exec "$TOOL" system-info)
assert_json_valid "Valid JSON" "$OUT"
assert_json "Has model field" "$OUT" '.model_name != null or .model != null'
assert_json "Has platform_version" "$OUT" '.platform_version != null'

# ── D3: runtime ───────────────────────────────────────────────────
section "D3" "runtime — CPU/memory"
OUT=$(cli_exec "$TOOL" runtime)
assert_json_valid "Valid JSON" "$OUT"

# ── D4: storage ───────────────────────────────────────────────────
section "D4" "storage"
OUT=$(cli_exec "$TOOL" storage)
assert_json_valid "Valid JSON" "$OUT"

# ── D5: thermal ───────────────────────────────────────────────────
section "D5" "thermal"
OUT=$(cli_exec "$TOOL" thermal)
assert_json_valid "Valid JSON" "$OUT"

# ── D6: display ───────────────────────────────────────────────────
section "D6" "display"
OUT=$(cli_exec "$TOOL" display)
assert_json_valid "Valid JSON" "$OUT"

# ── D7: settings ──────────────────────────────────────────────────
section "D7" "settings"
OUT=$(cli_exec "$TOOL" settings)
assert_json_valid "Valid JSON" "$OUT"
if _has_jq && echo "$OUT" | jq -e ".locale or .language or .font_type" >/dev/null 2>&1; then
  _pass "Has locale/font field"
else
  _skip "locale field" "not present in output"
fi

suite_end
