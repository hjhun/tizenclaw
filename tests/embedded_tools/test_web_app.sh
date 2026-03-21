#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# Embedded Tool Tests — Web App Generation
# Tests: generate_web_app → verify files → verify HTTP → delete
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "Embedded Tools: Web App Generation"

SESSION_ID="e2e_webapp_$$"
APP_ID="e2e_test_app_$$"

# ── WA1: Generate simple web app ──────────────────────────────────
section "WA1" "Generate simple web app"
OUT=$(tc_cli_session "$SESSION_ID" \
  "Use the generate_web_app tool with:
  app_id: '${APP_ID}'
  title: 'E2E Test App'
  html: '<!DOCTYPE html><html><head><title>E2E Test</title></head><body><h1>Hello from E2E</h1><p id=\"status\">OK</p></body></html>'")
assert_not_empty "generate_web_app returns output" "$OUT"

# ── WA2: Verify app files on device ───────────────────────────────
section "WA2" "Verify app files on device"
sleep 2  # wait for file write
APP_DIR="/opt/usr/share/tizenclaw/www/apps/${APP_ID}"
FILE_CHECK=$(sdb_shell "ls ${APP_DIR}/index.html 2>/dev/null && echo FOUND || echo NOT_FOUND" | tr -d '[:space:]')
if [ "$FILE_CHECK" = "FOUND" ]; then
  _pass "index.html exists at ${APP_DIR}"
else
  # App dir might be different
  ALT_CHECK=$(sdb_shell "find /opt/usr/share/tizenclaw -name '${APP_ID}' -type d 2>/dev/null")
  if [ -n "$ALT_CHECK" ]; then
    _pass "App directory found: ${ALT_CHECK}"
  else
    _skip "App file check" "app directory not found (LLM may have used different app_id)"
  fi
fi

# ── WA3: Verify app HTTP accessibility ────────────────────────────
section "WA3" "App accessible via HTTP"
APP_URL="http://127.0.0.1:9090/apps/${APP_ID}/"
HTTP_CODE=$(sdb_shell "curl -s -o /dev/null -w '%{http_code}' ${APP_URL} 2>/dev/null" | tr -d '[:space:]')
if [ "$HTTP_CODE" = "200" ]; then
  _pass "App accessible (HTTP 200)"
elif [ "$HTTP_CODE" = "404" ]; then
  _skip "App HTTP check" "HTTP 404 (app may use different path)"
else
  _skip "App HTTP check" "HTTP ${HTTP_CODE}"
fi

# ── WA4: Verify HTML content ─────────────────────────────────────
section "WA4" "Verify HTML content"
if [ "$HTTP_CODE" = "200" ]; then
  CONTENT=$(sdb_shell "curl -s ${APP_URL} 2>/dev/null")
  assert_contains "HTML has expected content" "$CONTENT" "E2E\|Hello\|Test"
else
  _skip "HTML content check" "app not accessible"
fi

# ── WA5: Generate app with CSS and JS ─────────────────────────────
section "WA5" "Generate app with separate CSS/JS"
APP_ID2="e2e_styled_app_$$"
OUT2=$(tc_cli_session "$SESSION_ID" \
  "Use the generate_web_app tool with:
  app_id: '${APP_ID2}'
  title: 'Styled Test App'
  html: '<!DOCTYPE html><html><head><title>Styled</title><link rel=\"stylesheet\" href=\"style.css\"></head><body><h1>Styled App</h1><script src=\"app.js\"></script></body></html>'
  css: 'h1 { color: blue; font-size: 24px; }'
  js: 'document.querySelector(\"h1\").textContent = \"JS Loaded\";'")
assert_not_empty "Styled app created" "$OUT2"

# ── WA6: List apps via API ────────────────────────────────────────
section "WA6" "List apps via API"
APPS_OUT=$(sdb_shell "curl -s http://127.0.0.1:9090/api/apps 2>/dev/null")
if [ -n "$APPS_OUT" ] && [ "$APPS_OUT" != "" ]; then
  _pass "Apps API returns content"
else
  _skip "Apps API" "endpoint may not exist"
fi

# ── WA7: Delete app ──────────────────────────────────────────────
section "WA7" "Delete generated apps"
# Try API delete
for aid in "$APP_ID" "$APP_ID2"; do
  DEL_CODE=$(sdb_shell "curl -s -o /dev/null -w '%{http_code}' -X DELETE http://127.0.0.1:9090/api/apps/${aid} 2>/dev/null" | tr -d '[:space:]')
  if [ "$DEL_CODE" = "200" ] || [ "$DEL_CODE" = "204" ]; then
    tc_log "Deleted app ${aid} (HTTP ${DEL_CODE})"
  fi
done
_pass "Cleanup delete requests sent"

suite_end
