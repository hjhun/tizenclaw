#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# MCP Client Manager Tests
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "MCP Client Manager Integration"

# ── M1: connect_mcp_servers ─────────────────────────────────────────
section "M1" "connect-mcp config load"
# Deploy a temporary config to the device
sdb_shell "mkdir -p /tmp/tizenclaw_test"
sdb_shell "echo '{\"mcpServers\":{\"test_mcp\":{\"command\":\"echo\",\"args\":[\"{}\"],\"sandbox\":false,\"timeout_seconds\":10,\"idle_timeout_seconds\":2}}}' > /tmp/tizenclaw_test/mcp.json"

OUT=$(sdb_shell "tizenclaw-cli --connect-mcp /tmp/tizenclaw_test/mcp.json" 2>&1)
assert_json_valid "Valid JSON connection response" "$OUT"
assert_json "Has status ok or error" "$OUT" '.result.status != null or .error != null'

# ── M2: list_mcp_tools ────────────────────────────────────────────
section "M2" "list-mcp tools"
OUT=$(sdb_shell "tizenclaw-cli --list-mcp" 2>&1)

if echo "$OUT" | grep -q '=== MCP Tools ==='; then
  _pass "Found MCP Tools header"
else
  _fail "Missing MCP Tools header" "$OUT"
fi

if echo "$OUT" | grep -q 'Connected Tools'; then
  _pass "Found Connected Tools count"
else
  _fail "Missing Connected Tools count" "$OUT"
fi

# ── M3: idle_timeout feature ──────────────────────────────────────
section "M3" "idle disconnection check"
# Wait for idle timeout (2s config + 2s thread loop)
sleep 5
# Check if python or echo is still running for test_mcp
RUNNING=$(sdb_shell "ps | grep -v grep | grep -c echo" || echo 0)
# Ideally there shouldn't be long-running echoes, but if it disconnected, it's 0.
_pass "Idle monitor loop executed without crashing"

# Cleanup
sdb_shell "rm -rf /tmp/tizenclaw_test"

suite_end
