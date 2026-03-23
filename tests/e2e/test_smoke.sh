#!/bin/bash
# TizenClaw E2E Smoke Test
# Validates service health, skill loading, and basic CLI functionality.
#
# Prerequisites:
#   - deploy.sh completed successfully
#   - tizenclaw service is running on the device
#   - sdb connection is established
#
# Usage:
#   ./tests/e2e/test_smoke.sh                  # Run all tests
#   ./tests/e2e/test_smoke.sh -d <serial>      # Target specific device
#
# Exit codes:
#   0 = all tests passed
#   1 = one or more tests failed

set -uo pipefail

# ─────────────────────────────────────────────
# Colors
# ─────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# ─────────────────────────────────────────────
# Counters
# ─────────────────────────────────────────────
PASS=0
FAIL=0
SKIP=0
DEVICE_SERIAL=""

# ─────────────────────────────────────────────
# Argument parsing
# ─────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    -d|--device) DEVICE_SERIAL="$2"; shift 2 ;;
    -h|--help)
      echo "Usage: $0 [-d <device-serial>]"
      exit 0
      ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

# ─────────────────────────────────────────────
# sdb wrapper
# ─────────────────────────────────────────────
sdb_cmd() {
  if [ -n "${DEVICE_SERIAL}" ]; then
    sdb -s "${DEVICE_SERIAL}" "$@"
  else
    sdb "$@"
  fi
}

sdb_shell() {
  sdb_cmd shell "$@" 2>/dev/null
}

# ─────────────────────────────────────────────
# Assertion helpers
# ─────────────────────────────────────────────
assert_contains() {
  local desc="$1" output="$2" expected="$3"
  if echo "$output" | grep -qiE "$expected"; then
    echo -e "  ${GREEN}[PASS]${NC} $desc"
    ((PASS++))
  else
    echo -e "  ${RED}[FAIL]${NC} $desc"
    echo -e "       expected pattern: ${YELLOW}${expected}${NC}"
    echo -e "       actual (first 200 chars): ${output:0:200}"
    ((FAIL++))
  fi
}

assert_not_empty() {
  local desc="$1" output="$2"
  if [ -n "$output" ]; then
    echo -e "  ${GREEN}[PASS]${NC} $desc"
    ((PASS++))
  else
    echo -e "  ${RED}[FAIL]${NC} $desc (empty output)"
    ((FAIL++))
  fi
}

assert_ge() {
  local desc="$1" actual="$2" min="$3"
  if [ "$actual" -ge "$min" ] 2>/dev/null; then
    echo -e "  ${GREEN}[PASS]${NC} $desc (${actual} >= ${min})"
    ((PASS++))
  else
    echo -e "  ${RED}[FAIL]${NC} $desc (${actual} < ${min})"
    ((FAIL++))
  fi
}

assert_file_exists() {
  local desc="$1" path="$2"
  local exists
  exists=$(sdb_shell "test -f '$path' && echo yes || echo no")
  if [ "$exists" = "yes" ]; then
    echo -e "  ${GREEN}[PASS]${NC} $desc"
    ((PASS++))
  else
    echo -e "  ${RED}[FAIL]${NC} $desc ($path not found)"
    ((FAIL++))
  fi
}

# ─────────────────────────────────────────────
# Tests
# ─────────────────────────────────────────────
echo ""
echo -e "${BOLD}══════════════════════════════════════════${NC}"
echo -e "${BOLD}  TizenClaw E2E Smoke Test${NC}"
echo -e "${BOLD}══════════════════════════════════════════${NC}"
echo ""

# ── T1: Service Status ─────────────────────
echo -e "${CYAN}[T1] Service Status${NC}"
STATUS=$(sdb_shell systemctl is-active tizenclaw || echo "inactive")
# Strip trailing whitespace/newline from sdb output
STATUS=$(echo "$STATUS" | tr -d '[:space:]')
assert_contains "tizenclaw service is active" "$STATUS" "^active$"

# ── T2: Skill Loading ──────────────────────
echo -e "\n${CYAN}[T2] Skill Loading${NC}"
TOOL_COUNT=$(sdb_shell dlogutil -d TIZENCLAW 2>/dev/null \
  | grep -c "MCP: Discovered tool" || echo 0)
assert_ge "Loaded tools count" "$TOOL_COUNT" 10

# ── T3: tools.md Generation ────────────────
echo -e "\n${CYAN}[T3] Tool Index Files${NC}"
assert_file_exists "tools.md exists" \
  "/opt/usr/share/tizen-tools/tools.md"
assert_file_exists "skills/index.md exists" \
  "/opt/usr/share/tizen-tools/skills/index.md"

# ── T4: CLI Basic Response ─────────────────
echo -e "\n${CYAN}[T4] CLI Basic Response${NC}"
CLI_RESP=$(sdb_shell tizenclaw-cli -s smoke_e2e_basic \
  "안녕하세요, 짧게 한 문장으로 답해주세요" 2>/dev/null || echo "")
assert_not_empty "CLI returns non-empty response" "$CLI_RESP"

# ── T5: Tool Invocation via CLI ─────────────
echo -e "\n${CYAN}[T5] Tool Invocation (get_device_info)${NC}"
CLI_TOOL=$(sdb_shell tizenclaw-cli -s smoke_e2e_tool \
  "get_device_info 도구를 호출해서 결과를 보여주세요" 2>/dev/null || echo "")
assert_not_empty "Tool invocation returns result" "$CLI_TOOL"

# Check daemon log for actual tool execution
TOOL_LOG=$(sdb_shell dlogutil -d TIZENCLAW 2>/dev/null \
  | grep "Executing skill: get_device_info" | tail -1 || echo "")
assert_not_empty "Daemon log shows tool execution" "$TOOL_LOG"

# ── T6: Manifest Parsing ───────────────────
echo -e "\n${CYAN}[T6] Manifest Parsing Integrity${NC}"
# Check that a known skill manifest is readable
MANIFEST_CHECK=$(sdb_shell \
  "cat /opt/usr/share/tizen-tools/skills/get_device_info/manifest.json" \
  2>/dev/null || echo "")
assert_contains "get_device_info manifest readable" \
  "$MANIFEST_CHECK" "parameters"

# ── T7: Session Persistence ────────────────
echo -e "\n${CYAN}[T7] Session Persistence${NC}"
SESSION_DIR=$(sdb_shell \
  "ls /opt/usr/share/tizenclaw/work/sessions/ 2>/dev/null | head -1" || echo "")
# Sessions directory should exist (may be empty if fresh)
SESSION_DIR_EXISTS=$(sdb_shell \
  "test -d /opt/usr/share/tizenclaw/work/sessions && echo yes || echo no")
assert_contains "Sessions directory exists" "$SESSION_DIR_EXISTS" "yes"

# ─────────────────────────────────────────────
# Summary
# ─────────────────────────────────────────────
echo ""
echo -e "${BOLD}══════════════════════════════════════════${NC}"
TOTAL=$((PASS + FAIL + SKIP))
if [ "$FAIL" -eq 0 ]; then
  echo -e "  ${GREEN}${BOLD}ALL PASSED${NC}: ${PASS}/${TOTAL} tests"
else
  echo -e "  ${RED}${BOLD}FAILED${NC}: ${PASS} passed, ${FAIL} failed"
fi
echo -e "${BOLD}══════════════════════════════════════════${NC}"
echo ""

# Dump recent daemon logs on failure for debugging
if [ "$FAIL" -gt 0 ]; then
  echo -e "${YELLOW}Recent daemon logs:${NC}"
  sdb_shell dlogutil -d TIZENCLAW 2>/dev/null | tail -20
  echo ""
fi

[ "$FAIL" -eq 0 ] && exit 0 || exit 1
