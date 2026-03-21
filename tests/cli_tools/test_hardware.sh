#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# tizen-hardware-control-cli Tests — Full Coverage
# Commands: haptic, led, power, feedback
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

TOOL="tizen-hardware-control-cli"
suite_begin "CLI: ${TOOL}"

if ! tc_tool_exists "${TC_CLI_BASE}/${TOOL}/${TOOL}"; then
  tc_warn "Tool binary not found, skipping suite"
  _skip "Tool binary exists" "not installed"
  suite_end; exit $?
fi

# ── HW1: haptic — vibration ──────────────────────────────────────
section "HW1" "haptic — default 500ms vibration"
OUT=$(cli_exec "$TOOL" haptic 2>/dev/null)
if [ -n "$OUT" ] && ! echo "$OUT" | grep -qiE "not supported|not available"; then
  assert_json_valid "haptic output is valid JSON" "$OUT"
else
  _skip "haptic" "vibration not available on this device"
fi

# ── HW2: haptic — custom duration ─────────────────────────────────
section "HW2" "haptic — 200ms duration"
OUT=$(cli_exec "$TOOL" haptic --duration 200 2>/dev/null)
if [ -n "$OUT" ] && ! echo "$OUT" | grep -qiE "not supported|not available"; then
  assert_json_valid "haptic 200ms output is valid JSON" "$OUT"
else
  _skip "haptic 200ms" "vibration not available"
fi

# ── HW3: feedback — TAP pattern ───────────────────────────────────
section "HW3" "feedback — TAP pattern"
OUT=$(cli_exec "$TOOL" feedback --pattern TAP 2>/dev/null)
if [ -n "$OUT" ] && ! echo "$OUT" | grep -qiE "not supported|not available"; then
  assert_json_valid "feedback TAP output is valid JSON" "$OUT"
else
  _skip "feedback TAP" "feedback not available"
fi

# ── HW4: feedback — MESSAGE pattern ───────────────────────────────
section "HW4" "feedback — MESSAGE pattern"
OUT=$(cli_exec "$TOOL" feedback --pattern MESSAGE 2>/dev/null)
if [ -n "$OUT" ] && ! echo "$OUT" | grep -qiE "not supported|not available"; then
  assert_json_valid "feedback MESSAGE output is valid JSON" "$OUT"
else
  _skip "feedback MESSAGE" "feedback not available"
fi

# ── HW5: feedback — WAKEUP pattern ───────────────────────────────
section "HW5" "feedback — WAKEUP pattern"
OUT=$(cli_exec "$TOOL" feedback --pattern WAKEUP 2>/dev/null)
if [ -n "$OUT" ] && ! echo "$OUT" | grep -qiE "not supported|not available"; then
  assert_json_valid "feedback WAKEUP output is valid JSON" "$OUT"
else
  _skip "feedback WAKEUP" "feedback not available"
fi

# ── HW6: led — on ────────────────────────────────────────────────
section "HW6" "led — turn on"
OUT=$(cli_exec "$TOOL" led --action on 2>/dev/null)
if [ -n "$OUT" ] && ! echo "$OUT" | grep -qiE "not supported|not available"; then
  assert_json_valid "led on output is valid JSON" "$OUT"
else
  _skip "led on" "LED not available on this device"
fi

# ── HW7: led — off ───────────────────────────────────────────────
section "HW7" "led — turn off"
OUT=$(cli_exec "$TOOL" led --action off 2>/dev/null)
if [ -n "$OUT" ] && ! echo "$OUT" | grep -qiE "not supported|not available"; then
  assert_json_valid "led off output is valid JSON" "$OUT"
else
  _skip "led off" "LED not available on this device"
fi

# ── HW8: led — with brightness ────────────────────────────────────
section "HW8" "led — with brightness"
OUT=$(cli_exec "$TOOL" led --action on --brightness 50 2>/dev/null)
if [ -n "$OUT" ] && ! echo "$OUT" | grep -qiE "not supported|not available"; then
  assert_json_valid "led brightness output is valid JSON" "$OUT"
  # Cleanup
  cli_exec "$TOOL" led --action off >/dev/null 2>&1
else
  _skip "led brightness" "LED not available"
fi

# ── HW9: power — lock display ────────────────────────────────────
section "HW9" "power — lock display"
OUT=$(cli_exec "$TOOL" power --action lock --resource display 2>/dev/null)
if [ -n "$OUT" ]; then
  assert_json_valid "power lock display output is valid JSON" "$OUT"
else
  _skip "power lock display" "power management not available"
fi

# ── HW10: power — unlock display ──────────────────────────────────
section "HW10" "power — unlock display"
OUT=$(cli_exec "$TOOL" power --action unlock --resource display 2>/dev/null)
if [ -n "$OUT" ]; then
  assert_json_valid "power unlock display output is valid JSON" "$OUT"
else
  _skip "power unlock display" "power management not available"
fi

# ── HW11: power — lock/unlock CPU ─────────────────────────────────
section "HW11" "power — lock & unlock CPU"
OUT=$(cli_exec "$TOOL" power --action lock --resource cpu 2>/dev/null)
if [ -n "$OUT" ]; then
  assert_json_valid "power lock CPU output is valid JSON" "$OUT"
  # Cleanup — unlock
  cli_exec "$TOOL" power --action unlock --resource cpu >/dev/null 2>&1
else
  _skip "power lock CPU" "CPU power lock not available"
fi

suite_end
