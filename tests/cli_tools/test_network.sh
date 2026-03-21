#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# tizen-network-info-cli Tests — Full Coverage
# Commands: network, wifi, wifi-scan, bluetooth, bt-scan, data-usage
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

TOOL="tizen-network-info-cli"
suite_begin "CLI: ${TOOL}"

if ! tc_tool_exists "${TC_CLI_BASE}/${TOOL}/${TOOL}"; then
  tc_warn "Tool binary not found, skipping suite"
  _skip "Tool binary exists" "not installed"
  suite_end; exit $?
fi

# ── N1: network — connection info ─────────────────────────────────
section "N1" "network — connection info"
OUT=$(cli_exec "$TOOL" network)
assert_json_valid "Output is valid JSON" "$OUT"
assert_json "Has connection_type" "$OUT" '.connection_type'

# ── N2: wifi — status ─────────────────────────────────────────────
section "N2" "wifi — status"
OUT=$(cli_exec "$TOOL" wifi)
assert_json_valid "Output is valid JSON" "$OUT"

# ── N3: wifi-scan — scan results ──────────────────────────────────
section "N3" "wifi-scan — scan results"
OUT=$(cli_exec "$TOOL" wifi-scan 2>/dev/null)
if [ -n "$OUT" ]; then
  assert_json_valid "wifi-scan output is valid JSON" "$OUT"
else
  _skip "wifi-scan" "Wi-Fi scanning not available"
fi

# ── N4: bluetooth — adapter info ──────────────────────────────────
section "N4" "bluetooth — adapter info"
OUT=$(cli_exec "$TOOL" bluetooth)
assert_json_valid "Output is valid JSON" "$OUT"

# ── N5: bt-scan — paired devices ──────────────────────────────────
section "N5" "bt-scan — paired devices"
OUT=$(cli_exec "$TOOL" bt-scan 2>/dev/null)
if [ -n "$OUT" ]; then
  assert_json_valid "bt-scan output is valid JSON" "$OUT"
else
  _skip "bt-scan" "Bluetooth scanning not available"
fi

# ── N6: data-usage ────────────────────────────────────────────────
section "N6" "data-usage — statistics"
OUT=$(cli_exec "$TOOL" data-usage)
assert_json_valid "Output is valid JSON" "$OUT"

suite_end
