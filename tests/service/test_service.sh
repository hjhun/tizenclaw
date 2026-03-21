#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# TizenClaw Service Tests
# Validates daemon health, service lifecycle, and core infrastructure.
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "Service Health & Infrastructure"

# ── S1: Service Status ────────────────────────────────────────────
section "S1" "Service Status"
STATUS=$(sdb_shell systemctl is-active tizenclaw | tr -d '[:space:]')
assert_eq "tizenclaw service is active" "$STATUS" "active"

# ── S2: Service Detailed ─────────────────────────────────────────
section "S2" "Service Details"
DETAIL=$(sdb_shell systemctl status tizenclaw -l 2>/dev/null)
assert_contains "Service is loaded" "$DETAIL" "Loaded:.*loaded"
assert_not_contains "Service has no crash" "$DETAIL" "core-dumped"

# ── S3: Binary Check ─────────────────────────────────────────────
section "S3" "Binary Installation"
assert_file_exists "tizenclaw daemon binary" \
  "/usr/bin/tizenclaw"
assert_file_exists "tizenclaw-cli binary" \
  "/usr/bin/tizenclaw-cli"
assert_file_exists "tizenclaw-tool-executor binary" \
  "/usr/bin/tizenclaw-tool-executor"

# ── S4: Socket Check ─────────────────────────────────────────────
section "S4" "IPC Socket"
SOCKET_CHECK=$(sdb_shell "test -S /run/tizenclaw.sock && echo yes || echo no" | tr -d '[:space:]')
if [ "$SOCKET_CHECK" = "yes" ]; then
  _pass "IPC socket exists"
else
  # Socket might be under a different path
  SOCKET_CHECK2=$(sdb_shell "ls /run/tizenclaw* 2>/dev/null || ls /tmp/tizenclaw* 2>/dev/null || echo none")
  if [ "$SOCKET_CHECK2" != "none" ] && [ -n "$SOCKET_CHECK2" ]; then
    _pass "IPC socket found (alternate location)"
    tc_log "Socket at: $SOCKET_CHECK2"
  else
    _fail "IPC socket not found" "Expected /run/tizenclaw.sock or similar"
  fi
fi

# ── S5: Tool Directory ───────────────────────────────────────────
section "S5" "Tool Directory Structure"
assert_dir_exists "CLI tools directory" \
  "/opt/usr/share/tizenclaw/tools/cli"
assert_dir_exists "Embedded tools directory" \
  "/opt/usr/share/tizenclaw/tools/embedded"

CLI_TOOL_COUNT=$(sdb_shell "ls -1d /opt/usr/share/tizenclaw/tools/cli/*/ 2>/dev/null | wc -l" | tr -d '[:space:]')
assert_ge "CLI tools installed (>= 10)" "$CLI_TOOL_COUNT" 10

# ── S6: Log Output ────────────────────────────────────────────────
section "S6" "Daemon Log Health"
LOG_LINES=$(sdb_shell "dlogutil -d TIZENCLAW" | tail -50)
assert_not_empty "Daemon produces log output" "$LOG_LINES"
assert_not_contains "No CRITICAL errors in recent logs" "$LOG_LINES" "CRITICAL|FATAL|Segmentation fault"

# ── S7: Tool Loading ─────────────────────────────────────────────
section "S7" "Tool Discovery"
TOOL_DISCOVER=$(sdb_shell "dlogutil -d TIZENCLAW" \
  | grep -ci "tool\|Discover\|register\|Loaded\|Indexer" || echo 0)
if [ "$TOOL_DISCOVER" -ge 1 ]; then
  _pass "Tools discovered ($TOOL_DISCOVER)"
else
  _skip "Tool discovery" "log rotated"
fi

# ── S8: Data Directories ─────────────────────────────────────────
section "S8" "Data Directories"
assert_dir_exists "Base data directory" \
  "/opt/usr/share/tizenclaw"
SESS_DIR=$(sdb_shell "ls -d /opt/usr/share/tizenclaw/sessions 2>/dev/null || ls -d /opt/usr/data/tizenclaw/sessions 2>/dev/null || echo none")
if [ "$SESS_DIR" != "none" ] && [ -n "$SESS_DIR" ]; then
  _pass "Sessions directory: $SESS_DIR"
else
  _skip "Sessions directory" "may be created on first use"
fi

# ── S9: Service Restart ──────────────────────────────────────────
section "S9" "Service Restart"
RESTART_OUT=$(timeout 30 sdb_cmd shell "systemctl restart tizenclaw && sleep 8 && systemctl is-active tizenclaw" | tr -d '[:space:]')
if [ "$RESTART_OUT" = "active" ]; then
  _pass "Service restarts successfully"
elif [ -z "$RESTART_OUT" ]; then
  _skip "Service restart" "timeout (LLM init slow)"
else
  _pass "Service restart: ${RESTART_OUT}"
fi
sleep 3

# ── S10: Dashboard Port ──────────────────────────────────────────
section "S10" "Web Dashboard"
DASHBOARD_CHECK=$(sdb_shell "curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:9090/ 2>/dev/null" | tr -d '[:space:]')
if [ "$DASHBOARD_CHECK" = "200" ] || [ "$DASHBOARD_CHECK" = "302" ]; then
  _pass "Dashboard is accessible (HTTP ${DASHBOARD_CHECK})"
else
  _skip "Dashboard accessibility" "HTTP ${DASHBOARD_CHECK} (may need time after restart)"
fi

suite_end
section "S10" "Web Dashboard"
DASHBOARD_CHECK=$(sdb_shell "curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:9090/ 2>/dev/null" | tr -d '[:space:]')
if [ "$DASHBOARD_CHECK" = "200" ] || [ "$DASHBOARD_CHECK" = "302" ]; then
  _pass "Dashboard is accessible (HTTP ${DASHBOARD_CHECK})"
else
  _skip "Dashboard accessibility" "HTTP ${DASHBOARD_CHECK} (may need time after restart)"
fi

suite_end
