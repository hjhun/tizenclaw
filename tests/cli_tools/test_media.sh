#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# tizen-media-cli Tests — Full Coverage
# Commands: content, metadata, mime, mime-ext
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

TOOL="tizen-media-cli"
suite_begin "CLI: ${TOOL}"

if ! tc_tool_exists "${TC_CLI_BASE}/${TOOL}/${TOOL}"; then
  tc_warn "Tool binary not found, skipping suite"
  _skip "Tool binary exists" "not installed"
  suite_end; exit $?
fi

# ── ME1: content — all types ──────────────────────────────────────
section "ME1" "content — all media types"
OUT=$(cli_exec "$TOOL" content)
assert_json_valid "Output is valid JSON" "$OUT"

# ── ME2: content — images only ────────────────────────────────────
section "ME2" "content — images"
OUT=$(cli_exec "$TOOL" content --type image)
assert_json_valid "Output is valid JSON" "$OUT"

# ── ME3: content — video only ─────────────────────────────────────
section "ME3" "content — video"
OUT=$(cli_exec "$TOOL" content --type video)
assert_json_valid "Output is valid JSON" "$OUT"

# ── ME4: content — music only ─────────────────────────────────────
section "ME4" "content — music"
OUT=$(cli_exec "$TOOL" content --type music)
assert_json_valid "Output is valid JSON" "$OUT"

# ── ME5: content — sound only ─────────────────────────────────────
section "ME5" "content — sound"
OUT=$(cli_exec "$TOOL" content --type sound)
assert_json_valid "Output is valid JSON" "$OUT"

# ── ME6: content — with max limit ─────────────────────────────────
section "ME6" "content — max limit"
OUT=$(cli_exec "$TOOL" content --max 3)
assert_json_valid "Output is valid JSON" "$OUT"

# ── ME7: mime-ext — image/png ─────────────────────────────────────
section "ME7" "mime-ext — image/png"
OUT=$(cli_exec "$TOOL" mime-ext --mime "image/png")
assert_json_valid "Output is valid JSON" "$OUT"
assert_contains "Has png extension" "$OUT" "png"

# ── ME8: mime-ext — text/plain ────────────────────────────────────
section "ME8" "mime-ext — text/plain"
OUT=$(cli_exec "$TOOL" mime-ext --mime "text/plain")
assert_json_valid "Output is valid JSON" "$OUT"
assert_contains "Has txt extension" "$OUT" "txt"

# ── ME9: mime — file MIME type lookup ─────────────────────────────
section "ME9" "mime — file type lookup"
OUT=$(cli_exec "$TOOL" mime --path "/etc/hosts" 2>/dev/null)
if [ -n "$OUT" ]; then
  if echo "$OUT" | grep -qE '^\s*[\{\[]'; then
    assert_json_valid "mime output is valid JSON" "$OUT"
  else
    assert_contains "Has MIME info" "$OUT" "text\|application\|mime\|octet"
  fi
else
  _skip "mime file lookup" "file may not be accessible"
fi

# ── ME10: metadata — audio/video file ─────────────────────────────
section "ME10" "metadata — media file"
# Find any media file on device
MEDIA_FILE=$(echo "$(cli_exec "$TOOL" content --max 1)" | jq -r '.files[0].path // empty' 2>/dev/null)
if [ -n "$MEDIA_FILE" ]; then
  OUT=$(cli_exec "$TOOL" metadata --path "$MEDIA_FILE")
  assert_json_valid "metadata output is valid JSON" "$OUT"
else
  _skip "metadata" "no media files found on device"
fi

suite_end
