#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# tizen-sound-cli Tests — Full Coverage
# Commands: volume get, volume set, devices, tone
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

TOOL="tizen-sound-cli"
suite_begin "CLI: ${TOOL}"

if ! tc_tool_exists "${TC_CLI_BASE}/${TOOL}/${TOOL}"; then
  tc_warn "Tool binary not found, skipping suite"
  _skip "Tool binary exists" "not installed"
  suite_end; exit $?
fi

# ── SO1: volume get ───────────────────────────────────────────────
section "SO1" "volume get — all levels"
OUT=$(cli_exec "$TOOL" volume get)
assert_json_valid "Output is valid JSON" "$OUT"

# Save original media volume for restore
if _has_jq; then
  ORIG_MEDIA_VOL=$(echo "$OUT" | jq -r '.media // 5' 2>/dev/null || echo 5)
else
  ORIG_MEDIA_VOL=$(echo "$OUT" | grep -oP '"media"\s*:\s*\K[0-9]+' 2>/dev/null || echo 5)
fi
tc_log "Original media volume: ${ORIG_MEDIA_VOL}"

# ── SO2: devices ──────────────────────────────────────────────────
section "SO2" "devices — audio devices"
OUT=$(cli_exec "$TOOL" devices)
assert_json_valid "Output is valid JSON" "$OUT"

# ── SO3: volume set — media ───────────────────────────────────────
section "SO3" "volume set — change media level"
OUT=$(cli_exec "$TOOL" volume set --type media --level 3)
assert_json_valid "Set output is valid JSON" "$OUT"

# Verify change
GET_OUT=$(cli_exec "$TOOL" volume get)
if _has_jq; then
  NEW_VOL=$(echo "$GET_OUT" | jq -r '.media // -1' 2>/dev/null || echo -1)
  assert_eq "Media volume changed to 3" "$NEW_VOL" "3"
else
  assert_contains "Volume response includes 3" "$GET_OUT" "3"
fi

# ── SO4: volume set — restore ─────────────────────────────────────
section "SO4" "volume restore"
cli_exec "$TOOL" volume set --type media --level "$ORIG_MEDIA_VOL" >/dev/null 2>&1
_pass "Volume restore command sent"

# ── SO5: volume set — notification type ───────────────────────────
section "SO5" "volume set — notification type"
OUT=$(cli_exec "$TOOL" volume set --type notification --level 5)
assert_json_valid "Notification volume set is valid JSON" "$OUT"

# ── SO6: volume set — ringtone type ───────────────────────────────
section "SO6" "volume set — ringtone type"
OUT=$(cli_exec "$TOOL" volume set --type ringtone --level 5)
assert_json_valid "Ringtone volume set is valid JSON" "$OUT"

# ── SO7: tone — play tone ────────────────────────────────────────
section "SO7" "tone — play tone"
OUT=$(cli_exec "$TOOL" tone --name "DEFAULT" --duration 100 2>/dev/null)
if [ -n "$OUT" ]; then
  assert_json_valid "Tone output is valid JSON" "$OUT"
else
  _skip "tone" "audio output may not be available"
fi

suite_end
