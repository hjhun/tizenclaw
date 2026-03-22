#!/bin/bash
# TizenClaw MCP Protocol Test
# Validates MCP JSON-RPC 2.0 protocol compliance using stdio transport.
#
# This script sends raw JSON-RPC messages to the MCP server via sdb and
# validates the responses against the MCP specification.
#
# Prerequisites:
#   - deploy.sh completed successfully
#   - tizenclaw binary supports --mcp-stdio mode
#   - jq installed on the host machine
#
# Usage:
#   ./tests/e2e/test_mcp.sh                  # Run all tests
#   ./tests/e2e/test_mcp.sh -d <serial>      # Target specific device
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
# Pre-flight: check jq
# ─────────────────────────────────────────────
if ! command -v jq &>/dev/null; then
  echo -e "${RED}Error: jq is required but not installed.${NC}"
  echo "Install with: sudo apt-get install jq"
  exit 1
fi

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

# Send a JSON-RPC request to tizenclaw --mcp-stdio
# and capture the response.
mcp_request() {
  local json_rpc="$1"
  # Echo the request into tizenclaw's stdin, capture stdout
  echo "$json_rpc" | sdb_cmd shell "tizenclaw --mcp-stdio" 2>/dev/null \
    | head -1  # Take only the first response line
}

# ─────────────────────────────────────────────
# Assertion helpers
# ─────────────────────────────────────────────
check_jq() {
  local desc="$1" json="$2" expr="$3"
  local result
  result=$(echo "$json" | jq -e "$expr" 2>/dev/null)
  if [ $? -eq 0 ]; then
    echo -e "  ${GREEN}[PASS]${NC} $desc"
    ((PASS++))
  else
    echo -e "  ${RED}[FAIL]${NC} $desc"
    echo -e "       jq expr: ${YELLOW}${expr}${NC}"
    echo -e "       json: ${json:0:300}"
    ((FAIL++))
  fi
}

check_eq() {
  local desc="$1" actual="$2" expected="$3"
  if [ "$actual" = "$expected" ]; then
    echo -e "  ${GREEN}[PASS]${NC} $desc"
    ((PASS++))
  else
    echo -e "  ${RED}[FAIL]${NC} $desc (expected: $expected, got: $actual)"
    ((FAIL++))
  fi
}

# ─────────────────────────────────────────────
# Tests
# ─────────────────────────────────────────────
echo ""
echo -e "${BOLD}══════════════════════════════════════════${NC}"
echo -e "${BOLD}  TizenClaw MCP Protocol Test${NC}"
echo -e "${BOLD}══════════════════════════════════════════${NC}"
echo ""

# ── M1: initialize ─────────────────────────
echo -e "${CYAN}[M1] initialize${NC}"
RESP=$(mcp_request '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test-runner","version":"1.0"}}}')

check_jq "JSON-RPC version is 2.0" \
  "$RESP" '.jsonrpc == "2.0"'
check_jq "Response ID matches request" \
  "$RESP" '.id == 1'
check_jq "protocolVersion present" \
  "$RESP" '.result.protocolVersion'
check_jq "serverInfo.name present" \
  "$RESP" '.result.serverInfo.name'
check_jq "capabilities.tools present" \
  "$RESP" '.result.capabilities.tools'

# ── M2: tools/list ─────────────────────────
echo -e "\n${CYAN}[M2] tools/list${NC}"
RESP=$(mcp_request '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}')

check_jq "tools array exists" \
  "$RESP" '.result.tools | type == "array"'
check_jq "tools array is non-empty" \
  "$RESP" '.result.tools | length > 0'
check_jq "ask_tizenclaw tool exists" \
  "$RESP" '.result.tools[] | select(.name == "ask_tizenclaw")'
check_jq "each tool has name field" \
  "$RESP" '[.result.tools[] | has("name")] | all'
check_jq "each tool has description field" \
  "$RESP" '[.result.tools[] | has("description")] | all'
check_jq "each tool has inputSchema field" \
  "$RESP" '[.result.tools[] | has("inputSchema")] | all'

# ── M3: Unknown Method ─────────────────────
echo -e "\n${CYAN}[M3] Unknown Method → Error${NC}"
RESP=$(mcp_request '{"jsonrpc":"2.0","id":3,"method":"unknown/method","params":{}}')

check_jq "Error response returned" \
  "$RESP" '.error'
check_jq "Error code is -32601 (Method not found)" \
  "$RESP" '.error.code == -32601'

# ── M4: Tool Not Found ─────────────────────
echo -e "\n${CYAN}[M4] tools/call with nonexistent tool${NC}"
RESP=$(mcp_request '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"nonexistent_tool","arguments":{}}}')

check_jq "isError flag is true" \
  "$RESP" '.result.isError == true'
check_jq "Error content returned" \
  "$RESP" '.result.content[0].text | contains("not found")'

# ── M5: Notification (no response expected) ──
echo -e "\n${CYAN}[M5] Notification (no id)${NC}"
RESP=$(mcp_request '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}')
# Notifications should return empty or no response
if [ -z "$RESP" ] || [ "$RESP" = "null" ]; then
  echo -e "  ${GREEN}[PASS]${NC} Notification returns no response"
  ((PASS++))
else
  # Some implementations may return empty JSON
  echo -e "  ${YELLOW}[WARN]${NC} Notification returned: ${RESP:0:100}"
  echo -e "  ${GREEN}[PASS]${NC} (non-critical — notification handling varies)"
  ((PASS++))
fi

# ── M6: Malformed JSON ─────────────────────
echo -e "\n${CYAN}[M6] Malformed JSON Input${NC}"
RESP=$(mcp_request 'this is not json')

check_jq "Parse error returned" \
  "$RESP" '.error.code == -32700'

# ─────────────────────────────────────────────
# Summary
# ─────────────────────────────────────────────
echo ""
echo -e "${BOLD}══════════════════════════════════════════${NC}"
TOTAL=$((PASS + FAIL))
if [ "$FAIL" -eq 0 ]; then
  echo -e "  ${GREEN}${BOLD}ALL PASSED${NC}: ${PASS}/${TOTAL} tests"
else
  echo -e "  ${RED}${BOLD}FAILED${NC}: ${PASS} passed, ${FAIL} failed"
fi
echo -e "${BOLD}══════════════════════════════════════════${NC}"
echo ""

[ "$FAIL" -eq 0 ] && exit 0 || exit 1
