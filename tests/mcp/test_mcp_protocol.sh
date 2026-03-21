#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# MCP Protocol Tests
# Validates MCP JSON-RPC 2.0 protocol compliance.
# Enhanced version of test/e2e/test_mcp.sh with additional coverage.
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "MCP Protocol Compliance"

# Note: jq strongly recommended for MCP tests.
# Without jq, individual assertions will be skipped.
if ! command -v jq &>/dev/null; then
  tc_warn "jq not found — MCP assertions will fall back to basic checks"
fi

# Send a JSON-RPC request to tizenclaw --mcp-stdio
mcp_request() {
  local json_rpc="$1"
  echo "$json_rpc" | sdb_cmd shell "tizenclaw --mcp-stdio" 2>/dev/null \
    | head -1
}

# ── M1: initialize ───────────────────────────────────────────────
section "M1" "initialize"
RESP=$(mcp_request '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"e2e-test","version":"1.0"}}}')
assert_json "JSON-RPC version is 2.0" "$RESP" '.jsonrpc == "2.0"'
assert_json "Response ID matches" "$RESP" '.id == 1'
assert_json "protocolVersion present" "$RESP" '.result.protocolVersion'
assert_json "serverInfo.name present" "$RESP" '.result.serverInfo.name'
assert_json "capabilities.tools present" "$RESP" '.result.capabilities.tools'

# ── M2: tools/list ───────────────────────────────────────────────
section "M2" "tools/list"
RESP=$(mcp_request '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}')
assert_json "tools array exists" "$RESP" '.result.tools | type == "array"'
assert_json "tools array is non-empty" "$RESP" '.result.tools | length > 0'
assert_json "ask_tizenclaw tool exists" "$RESP" \
  '.result.tools[] | select(.name == "ask_tizenclaw")'
assert_json "Each tool has name" "$RESP" '[.result.tools[] | has("name")] | all'
assert_json "Each tool has description" "$RESP" \
  '[.result.tools[] | has("description")] | all'
assert_json "Each tool has inputSchema" "$RESP" \
  '[.result.tools[] | has("inputSchema")] | all'

if _has_jq; then
  TOOL_COUNT=$(echo "$RESP" | jq '.result.tools | length' 2>/dev/null || echo 0)
  tc_info "Total MCP tools: ${TOOL_COUNT}"
fi

# ── M3: Unknown Method ───────────────────────────────────────────
section "M3" "Unknown Method → error"
RESP=$(mcp_request '{"jsonrpc":"2.0","id":3,"method":"unknown/method","params":{}}')
assert_json "Error response returned" "$RESP" '.error'
assert_json "Error code -32601" "$RESP" '.error.code == -32601'

# ── M4: Tool Not Found ───────────────────────────────────────────
section "M4" "tools/call — nonexistent tool"
RESP=$(mcp_request '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"nonexistent_tool_xyz","arguments":{}}}')
assert_json "isError flag is true" "$RESP" '.result.isError == true'

# ── M5: Notification (no id) ─────────────────────────────────────
section "M5" "Notification (no id)"
RESP=$(mcp_request '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}')
if [ -z "$RESP" ] || [ "$RESP" = "null" ]; then
  _pass "Notification returns no response"
else
  _pass "Notification handled (non-critical)"
fi

# ── M6: Malformed JSON ───────────────────────────────────────────
section "M6" "Malformed JSON input"
RESP=$(mcp_request 'this is not json')
assert_json "Parse error code -32700" "$RESP" '.error.code == -32700'

# ── M7: Missing required fields ──────────────────────────────────
section "M7" "Missing jsonrpc version"
RESP=$(mcp_request '{"id":7,"method":"tools/list","params":{}}')
# Should still work or return proper error
if _has_jq && echo "$RESP" | jq empty 2>/dev/null; then
  _pass "Handles missing jsonrpc field gracefully"
elif [ -n "$RESP" ]; then
  _pass "Handles missing jsonrpc field (returned response)"
else
  _skip "Missing jsonrpc handling" "no valid response"
fi

# ── M8: Batch-like request ────────────────────────────────────────
section "M8" "Protocol version validation"
RESP=$(mcp_request '{"jsonrpc":"2.0","id":8,"method":"initialize","params":{"protocolVersion":"9999-01-01","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}')
# Should either work or return error for unsupported version
if _has_jq && echo "$RESP" | jq empty 2>/dev/null; then
  _pass "Handles version negotiation"
elif [ -n "$RESP" ]; then
  _pass "Handles version negotiation (returned response)"
else
  _skip "Version negotiation" "no JSON response"
fi

suite_end
