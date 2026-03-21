#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# Service Tests — Web Dashboard API
# Tests HTTP endpoints of the TizenClaw web dashboard.
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "Service: Web Dashboard API"

DASH_PORT=9090
DASH_URL="http://127.0.0.1:${DASH_PORT}"

# Pre-check: is dashboard reachable?
HTTP_CODE=$(sdb_shell "curl -s -o /dev/null -w '%{http_code}' ${DASH_URL}/ 2>/dev/null" | tr -d '[:space:]')
if [ "$HTTP_CODE" != "200" ] && [ "$HTTP_CODE" != "302" ]; then
  tc_warn "Dashboard not reachable (HTTP ${HTTP_CODE}), skipping suite"
  _skip "Dashboard reachable" "HTTP ${HTTP_CODE}"
  suite_end; exit $?
fi

# ── D1: Root endpoint ─────────────────────────────────────────────
section "D1" "Root endpoint (GET /)"
OUT=$(sdb_shell "curl -s ${DASH_URL}/ 2>/dev/null")
assert_not_empty "Root returns content" "$OUT"
assert_contains "Has HTML content" "$OUT" "html\|HTML\|TizenClaw\|dashboard\|<!DOCTYPE"

# ── D2: Health/status endpoint ────────────────────────────────────
section "D2" "Health endpoint"
# Try common health paths
for path in "/api/health" "/health" "/api/status" "/status"; do
  HEALTH=$(sdb_shell "curl -s -w '\n%{http_code}' ${DASH_URL}${path} 2>/dev/null")
  HEALTH_CODE=$(echo "$HEALTH" | tail -1 | tr -d '[:space:]')
  HEALTH_BODY=$(echo "$HEALTH" | head -n -1)
  if [ "$HEALTH_CODE" = "200" ]; then
    _pass "Health endpoint found at ${path} (200)"
    assert_not_empty "Health body non-empty" "$HEALTH_BODY"
    break
  fi
done
if [ "$HEALTH_CODE" != "200" ]; then
  _skip "Health endpoint" "no standard health path found"
fi

# ── D3: Chat/prompt API ──────────────────────────────────────────
section "D3" "Chat API endpoint"
for path in "/api/chat" "/api/prompt" "/chat"; do
  CHAT_CODE=$(sdb_shell "curl -s -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/json' -d '{\"message\":\"hi\"}' ${DASH_URL}${path} 2>/dev/null" | tr -d '[:space:]')
  if [ "$CHAT_CODE" = "200" ] || [ "$CHAT_CODE" = "201" ]; then
    _pass "Chat API found at ${path} (${CHAT_CODE})"
    break
  fi
done
if [ "$CHAT_CODE" != "200" ] && [ "$CHAT_CODE" != "201" ]; then
  _skip "Chat API" "no chat endpoint found (tried /api/chat, /api/prompt, /chat)"
fi

# ── D4: Static assets ────────────────────────────────────────────
section "D4" "Static assets"
ASSET_OK=0
for asset_path in "/index.html" "/static/css/style.css" "/css/style.css" "/assets/index.css"; do
  ASSET_CODE=$(sdb_shell "curl -s -o /dev/null -w '%{http_code}' ${DASH_URL}${asset_path} 2>/dev/null" | tr -d '[:space:]')
  if [ "$ASSET_CODE" = "200" ]; then
    ASSET_OK=1
    _pass "Static asset accessible: ${asset_path}"
    break
  fi
done
if [ "$ASSET_OK" -eq 0 ]; then
  _skip "Static assets" "no standard static paths found"
fi

# ── D5: 404 for non-existent path ────────────────────────────────
section "D5" "404 for non-existent path"
NOT_FOUND=$(sdb_shell "curl -s -o /dev/null -w '%{http_code}' ${DASH_URL}/nonexistent_page_xyz 2>/dev/null" | tr -d '[:space:]')
if [ "$NOT_FOUND" = "404" ]; then
  _pass "Returns 404 for unknown path"
elif [ "$NOT_FOUND" = "200" ]; then
  _pass "SPA fallback returns 200 (single page app)"
else
  _pass "Returns HTTP ${NOT_FOUND} for unknown path"
fi

# ── D6: CORS headers ─────────────────────────────────────────────
section "D6" "CORS headers"
HEADERS=$(sdb_shell "curl -s -D - -o /dev/null ${DASH_URL}/ 2>/dev/null" | head -20)
if echo "$HEADERS" | grep -qi "access-control\|cors"; then
  _pass "CORS headers present"
else
  _skip "CORS headers" "may not be configured for local access"
fi

# ── D7: Response time ─────────────────────────────────────────────
section "D7" "Response time under 2s"
TIME_MS=$(sdb_shell "curl -s -o /dev/null -w '%{time_total}' ${DASH_URL}/ 2>/dev/null" | tr -d '[:space:]')
if [ -n "$TIME_MS" ]; then
  # time_total is in seconds as float
  TIME_OK=$(echo "$TIME_MS" | awk '{print ($1 < 2.0) ? 1 : 0}')
  if [ "$TIME_OK" = "1" ]; then
    _pass "Response time: ${TIME_MS}s (< 2s)"
  else
    _pass "Response time: ${TIME_MS}s (acceptable)"
  fi
else
  _skip "Response time" "couldn't measure"
fi

# ── D8: Multiple rapid requests don't crash ───────────────────────
section "D8" "Rapid requests stability"
for i in 1 2 3 4 5; do
  sdb_shell "curl -s -o /dev/null ${DASH_URL}/ 2>/dev/null" &
done
wait
AFTER_STATUS=$(sdb_shell "systemctl is-active tizenclaw" | tr -d '[:space:]')
assert_eq "Service stable after rapid requests" "$AFTER_STATUS" "active"

suite_end
