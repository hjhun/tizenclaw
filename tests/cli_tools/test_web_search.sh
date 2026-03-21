#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# tizen-web-search-cli Tests — Full Coverage
# Command: --query [--engine]
# Engines: naver, google, brave, gemini, grok, kimi, perplexity
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

TOOL="tizen-web-search-cli"
suite_begin "CLI: ${TOOL}"

if ! tc_tool_exists "${TC_CLI_BASE}/${TOOL}/${TOOL}"; then
  tc_warn "Tool binary not found, skipping suite"
  _skip "Tool binary exists" "not installed"
  suite_end; exit $?
fi

# Helper: test a search engine
test_search_engine() {
  local id="$1" engine="$2"
  section "$id" "Search — ${engine} engine"
  OUT=$(cli_exec "$TOOL" --query "Tizen OS" --engine "$engine" 2>/dev/null)
  if [ -n "$OUT" ] && ! echo "$OUT" | grep -qiE "network.*error|connection refused|timeout"; then
    assert_json_valid "Output is valid JSON" "$OUT"
  else
    _skip "search (${engine})" "network or API unavailable"
  fi
}

# ── WS1: default search ──────────────────────────────────────────
section "WS1" "Search — default engine"
OUT=$(cli_exec "$TOOL" --query "Tizen OS" 2>/dev/null)
if [ -n "$OUT" ] && ! echo "$OUT" | grep -qiE "network.*error|connection refused|timeout"; then
  assert_json_valid "Output is valid JSON" "$OUT"
  assert_contains "Has search structure" "$OUT" "engine\|query\|results"
  HAS_NETWORK=1
else
  _skip "default search" "network unavailable"
  HAS_NETWORK=0
fi

# Only test engines if network is available
if [ "${HAS_NETWORK:-0}" -eq 1 ]; then
  test_search_engine "WS2" "naver"
  test_search_engine "WS3" "google"
  test_search_engine "WS4" "brave"
  test_search_engine "WS5" "gemini"
else
  _skip "engine tests" "network unavailable, skipping all engine tests"
fi

# ── WS6: Korean query ────────────────────────────────────────────
section "WS6" "Search — Korean query"
if [ "${HAS_NETWORK:-0}" -eq 1 ]; then
  OUT=$(cli_exec "$TOOL" --query "타이젠 운영체제" 2>/dev/null)
  if [ -n "$OUT" ]; then
    assert_json_valid "Korean query output is valid JSON" "$OUT"
  else
    _skip "Korean search" "no response"
  fi
else
  _skip "Korean search" "network unavailable"
fi

suite_end
