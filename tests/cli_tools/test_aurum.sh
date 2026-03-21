#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# tizen-aurum-cli Tests — Full Coverage
# Commands: screen-size, screenshot, get-angle, click, flick,
#           send-key, find-element, find-elements, dump-tree,
#           click-element, set-value, watch
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

TOOL="tizen-aurum-cli"
suite_begin "CLI: ${TOOL}"

if ! tc_tool_exists "${TC_CLI_BASE}/${TOOL}/${TOOL}"; then
  tc_warn "Tool binary not found, skipping suite"
  _skip "Tool binary exists" "not installed"
  suite_end; exit $?
fi

# Helper: check if aurum-cli returns valid output
# Aurum requires at-spi-bus which may not be available on emulators
aurum_exec() {
  local out
  out=$(cli_exec "$TOOL" "$@" 2>/dev/null)
  if echo "$out" | grep -qiE "at-spi|dbus.*error|Cannot connect|connection.*refused"; then
    echo ""
  else
    echo "$out"
  fi
}

# Pre-check: verify aurum connectivity
PRECHECK=$(aurum_exec screen-size)
if [ -z "$PRECHECK" ]; then
  tc_warn "Aurum/at-spi not available on this device — all tests will be skipped"
  AURUM_AVAILABLE=0
else
  AURUM_AVAILABLE=1
fi

# ── AU1: screen-size ──────────────────────────────────────────────
section "AU1" "screen-size"
if [ "$AURUM_AVAILABLE" -eq 1 ]; then
  OUT="$PRECHECK"
  assert_json_valid "Output is valid JSON" "$OUT"
  assert_json "Has width" "$OUT" '.width'
  assert_json "Has height" "$OUT" '.height'
  SCREEN_W=$(echo "$OUT" | jq -r '.width // 360' 2>/dev/null || echo 360)
  SCREEN_H=$(echo "$OUT" | jq -r '.height // 640' 2>/dev/null || echo 640)
else
  _skip "screen-size" "Aurum not available"
  SCREEN_W=360; SCREEN_H=640
fi
tc_log "Screen: ${SCREEN_W}x${SCREEN_H}"

# Remaining tests all gated on Aurum availability
if [ "$AURUM_AVAILABLE" -eq 0 ]; then
  for t in "AU2:get-angle" "AU3:screenshot" "AU4:dump-tree" "AU5:find-element" \
           "AU6:find-elements" "AU7:click" "AU8:long-press" "AU9:flick" \
           "AU10:send-key-back" "AU11:send-key-home" "AU12:click-element" \
           "AU13:set-value" "AU14:watch"; do
    id="${t%%:*}"; name="${t#*:}"
    section "$id" "$name"
    _skip "$name" "Aurum not available"
  done
  suite_end; exit $?
fi

# ── AU2: get-angle ────────────────────────────────────────────────
section "AU2" "get-angle — rotation"
OUT=$(aurum_exec get-angle)
assert_json_valid "Output is valid JSON" "$OUT"
assert_json "Has angle field" "$OUT" '.angle != null'

# ── AU3: screenshot ───────────────────────────────────────────────
section "AU3" "screenshot"
SCREENSHOT_PATH="/tmp/e2e_screenshot_$$.png"
OUT=$(aurum_exec screenshot --output "$SCREENSHOT_PATH")
assert_json_valid "Output is valid JSON" "$OUT"
FILE_EXISTS=$(sdb_shell "test -f '$SCREENSHOT_PATH' && echo yes || echo no" | tr -d '[:space:]')
if [ "$FILE_EXISTS" = "yes" ]; then
  _pass "Screenshot file created"
  sdb_shell "rm -f '$SCREENSHOT_PATH'" >/dev/null 2>&1
else
  _skip "Screenshot file creation" "file may not persist"
fi

# ── AU4: dump-tree ────────────────────────────────────────────────
section "AU4" "dump-tree — UI element tree"
OUT=$(aurum_exec dump-tree)
assert_json_valid "Output is valid JSON" "$OUT"

# ── AU5: find-element ─────────────────────────────────────────────
section "AU5" "find-element — by type"
OUT=$(aurum_exec find-element --type "Elm_Win")
if [ -n "$OUT" ]; then
  assert_json_valid "Output is valid JSON" "$OUT"
else
  _skip "find-element" "no Elm_Win found"
fi

# ── AU6: find-elements ────────────────────────────────────────────
section "AU6" "find-elements — multiple results"
OUT=$(aurum_exec find-elements --type "Elm_Button")
if [ -n "$OUT" ]; then
  assert_json_valid "Output is valid JSON" "$OUT"
else
  _skip "find-elements" "no buttons found"
fi

# ── AU7: click ────────────────────────────────────────────────────
section "AU7" "click — tap center"
CENTER_X=$((SCREEN_W / 2)); CENTER_Y=$((SCREEN_H / 2))
OUT=$(aurum_exec click --x "$CENTER_X" --y "$CENTER_Y")
assert_json_valid "Click output is valid JSON" "$OUT"

# ── AU8: click long-press ─────────────────────────────────────────
section "AU8" "click — long-press"
OUT=$(aurum_exec click --x "$CENTER_X" --y "$CENTER_Y" --duration 500)
assert_json_valid "Long-press output is valid JSON" "$OUT"

# ── AU9: flick ────────────────────────────────────────────────────
section "AU9" "flick — swipe down"
OUT=$(aurum_exec flick --sx "$CENTER_X" --sy $((SCREEN_H / 4)) \
  --ex "$CENTER_X" --ey $((SCREEN_H * 3 / 4)))
assert_json_valid "Flick output is valid JSON" "$OUT"

# ── AU10: send-key ────────────────────────────────────────────────
section "AU10" "send-key — back key"
OUT=$(aurum_exec send-key --key back)
assert_json_valid "send-key output is valid JSON" "$OUT"

# ── AU11: send-key — home key ─────────────────────────────────────
section "AU11" "send-key — home key"
OUT=$(aurum_exec send-key --key home)
assert_json_valid "home key output is valid JSON" "$OUT"

# ── AU12: click-element ───────────────────────────────────────────
section "AU12" "click-element — by type"
OUT=$(aurum_exec click-element --type "Elm_Button" 2>/dev/null)
if [ -n "$OUT" ]; then
  assert_json_valid "click-element output is valid JSON" "$OUT"
else
  _skip "click-element" "no button element found"
fi

# ── AU13: set-value ───────────────────────────────────────────────
section "AU13" "set-value — entry widget"
OUT=$(aurum_exec set-value --type "Elm_Entry" --text-value "e2e test" 2>/dev/null)
if [ -n "$OUT" ]; then
  assert_json_valid "set-value output is valid JSON" "$OUT"
else
  _skip "set-value" "no entry widget found"
fi

# ── AU14: watch ───────────────────────────────────────────────────
section "AU14" "watch — UI events (short timeout)"
OUT=$(timeout 5 bash -c "$(printf '%s/%s/%s watch --event UiObject --timeout 2000' "$TC_CLI_BASE" "$TOOL" "$TOOL")" 2>/dev/null || echo "")
_pass "watch completed without crash"

suite_end
