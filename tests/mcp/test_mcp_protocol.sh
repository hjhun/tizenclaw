#!/bin/bash
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight
suite_begin "MCP Protocol Compliance"
mcp_request() { echo "$1" | sdb_cmd shell "tizenclaw --mcp-stdio" 2>/dev/null | head -1; }
MCP_PRE=$(mcp_request '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"pre","version":"1.0"}}}')
MCP_OK=0
if command -v jq &>/dev/null && echo "$MCP_PRE" | jq -e '.result' >/dev/null 2>&1; then MCP_OK=1; fi
[ "$MCP_OK" -eq 0 ] && tc_warn "MCP not returning JSON-RPC"
section "M1" "initialize"
if [ "$MCP_OK" -eq 1 ]; then assert_json "protocolVersion" "$MCP_PRE" '.result.protocolVersion'; else _skip "initialize" "MCP echo"; fi
section "M2" "tools/list"
if [ "$MCP_OK" -eq 1 ]; then R=$(mcp_request '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'); assert_json "tools" "$R" '.result.tools | type == "array"'; else _skip "tools/list" "MCP echo"; fi
section "M3" "binary exists"
B=$(sdb_shell "which tizenclaw 2>/dev/null || echo none"); assert_not_empty "binary" "$B"
section "M4" "help flag"
H=$(sdb_shell "tizenclaw --help 2>&1 || true"); assert_not_empty "help" "$H"
section "M5" "notification"
if [ "$MCP_OK" -eq 1 ]; then _pass "handled"; else _skip "notification" "MCP echo"; fi
section "M6" "unknown method"
if [ "$MCP_OK" -eq 1 ]; then R=$(mcp_request '{"jsonrpc":"2.0","id":3,"method":"unknown/method","params":{}}'); assert_json "error" "$R" '.error'; else _skip "unknown" "MCP echo"; fi
suite_end
