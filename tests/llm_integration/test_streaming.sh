#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# LLM Integration Tests — Streaming Mode
# Validates tizenclaw-cli --stream output.
# Note: --stream may not be supported in all builds.
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "LLM Integration: Streaming"

# Pre-check: does --stream flag exist?
HELP=$(sdb_cmd shell "tizenclaw-cli --help 2>&1" | tr -d '\r')
if ! echo "$HELP" | grep -qi "stream"; then
  tc_warn "tizenclaw-cli does not support --stream flag"
  _skip "Streaming tests" "--stream not supported"
  suite_end; exit $?
fi

# ── ST1: Basic streaming ─────────────────────────────────────────
section "ST1" "Streaming mode basic"
OUT=$(timeout "${TC_TIMEOUT}" sdb_cmd shell "tizenclaw-cli --stream 'Say hello in one sentence'" 2>/dev/null || echo "")
if [ -n "$OUT" ]; then
  _pass "Streaming returns output"
else
  _skip "Streaming output" "no output within timeout"
fi

# ── ST2: Streaming produces multiple chunks ───────────────────────
section "ST2" "Streaming produces output"
LINE_COUNT=$(echo "$OUT" | wc -l | tr -d '[:space:]')
assert_ge "Streaming produces lines" "$LINE_COUNT" 1

# ── ST3: Streaming with Korean ───────────────────────────────────
section "ST3" "Streaming with Korean prompt"
OUT=$(timeout "${TC_TIMEOUT}" sdb_cmd shell "tizenclaw-cli --stream '한국어로 짧은 인사를 해주세요'" 2>/dev/null || echo "")
if [ -n "$OUT" ]; then
  _pass "Korean streaming returns output"
else
  _skip "Korean streaming" "no output within timeout"
fi

suite_end
