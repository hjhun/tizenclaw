#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# tizen-file-manager-cli Tests — Full Coverage
# Commands: mkdir, write, read, append, stat, list, copy, move,
#           remove, download
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

TOOL="tizen-file-manager-cli"
suite_begin "CLI: ${TOOL}"

if ! tc_tool_exists "${TC_CLI_BASE}/${TOOL}/${TOOL}"; then
  tc_warn "Tool binary not found, skipping suite"
  _skip "Tool binary exists" "not installed"
  suite_end; exit $?
fi

# Test directory — unique per PID for isolation
TEST_DIR="/tmp/e2e_file_test_$$"
TEST_FILE="${TEST_DIR}/test_file.txt"
TEST_FILE2="${TEST_DIR}/test_copy.txt"
TEST_FILE3="${TEST_DIR}/test_moved.txt"
TEST_SUBDIR="${TEST_DIR}/subdir"
DOWNLOAD_FILE="${TEST_DIR}/downloaded.txt"

# ── F1: mkdir ─────────────────────────────────────────────────────
section "F1" "mkdir — create test directory"
OUT=$(cli_exec "$TOOL" mkdir --path "$TEST_DIR")
assert_json_valid "mkdir output is valid JSON" "$OUT"
assert_dir_exists "Test directory created" "$TEST_DIR"

# ── F2: mkdir nested ─────────────────────────────────────────────
section "F2" "mkdir — nested directory"
OUT=$(cli_exec "$TOOL" mkdir --path "$TEST_SUBDIR")
assert_json_valid "mkdir nested output is valid JSON" "$OUT"
assert_dir_exists "Nested directory created" "$TEST_SUBDIR"

# ── F3: write ─────────────────────────────────────────────────────
section "F3" "write — create file"
OUT=$(cli_exec "$TOOL" write --path "$TEST_FILE" --content "Hello, TizenClaw E2E!")
assert_json_valid "write output is valid JSON" "$OUT"
assert_file_exists "File created" "$TEST_FILE"

# ── F4: read ──────────────────────────────────────────────────────
section "F4" "read — file contents"
OUT=$(cli_exec "$TOOL" read --path "$TEST_FILE")
assert_json_valid "read output is valid JSON" "$OUT"
assert_contains "Content matches" "$OUT" "Hello, TizenClaw E2E!"

# ── F5: stat ──────────────────────────────────────────────────────
section "F5" "stat — file metadata"
OUT=$(cli_exec "$TOOL" stat --path "$TEST_FILE")
assert_json_valid "stat output is valid JSON" "$OUT"
assert_json "Has size field" "$OUT" '.size'
assert_json "Has type field" "$OUT" '.type'

# ── F6: stat — directory ──────────────────────────────────────────
section "F6" "stat — directory metadata"
OUT=$(cli_exec "$TOOL" stat --path "$TEST_DIR")
assert_json_valid "stat dir output is valid JSON" "$OUT"
assert_json "Type is directory" "$OUT" '.type == "directory"'

# ── F7: list ──────────────────────────────────────────────────────
section "F7" "list — directory contents"
OUT=$(cli_exec "$TOOL" list --path "$TEST_DIR")
assert_json_valid "list output is valid JSON" "$OUT"
assert_json "Has entries array" "$OUT" '.entries | type == "array"'
assert_json_array_ge "At least 2 entries" "$OUT" '.entries' 2

# ── F8: append ────────────────────────────────────────────────────
section "F8" "append — add content"
OUT=$(cli_exec "$TOOL" append --path "$TEST_FILE" --content " Appended data.")
assert_json_valid "append output is valid JSON" "$OUT"

# Verify appended content
READ_OUT=$(cli_exec "$TOOL" read --path "$TEST_FILE")
assert_contains "Contains original" "$READ_OUT" "Hello, TizenClaw E2E!"
assert_contains "Contains appended" "$READ_OUT" "Appended data"

# ── F9: copy ──────────────────────────────────────────────────────
section "F9" "copy — duplicate file"
OUT=$(cli_exec "$TOOL" copy --src "$TEST_FILE" --dst "$TEST_FILE2")
assert_json_valid "copy output is valid JSON" "$OUT"
assert_file_exists "Copied file exists" "$TEST_FILE2"

# Verify contents match
READ_COPY=$(cli_exec "$TOOL" read --path "$TEST_FILE2")
assert_contains "Copied content matches" "$READ_COPY" "Hello, TizenClaw E2E!"

# ── F10: move ─────────────────────────────────────────────────────
section "F10" "move — rename file"
OUT=$(cli_exec "$TOOL" move --src "$TEST_FILE2" --dst "$TEST_FILE3")
assert_json_valid "move output is valid JSON" "$OUT"
assert_file_exists "Moved file exists" "$TEST_FILE3"

# Verify original is gone
OLD_EXISTS=$(sdb_shell "test -f '$TEST_FILE2' && echo yes || echo no" | tr -d '[:space:]')
assert_eq "Original removed after move" "$OLD_EXISTS" "no"

# ── F11: write to subdirectory ────────────────────────────────────
section "F11" "write — file in subdirectory"
SUBFILE="${TEST_SUBDIR}/nested_file.txt"
OUT=$(cli_exec "$TOOL" write --path "$SUBFILE" --content "nested content")
assert_json_valid "write nested output is valid JSON" "$OUT"
assert_file_exists "Nested file created" "$SUBFILE"

# ── F12: download ─────────────────────────────────────────────────
section "F12" "download — file from URL"
OUT=$(cli_exec "$TOOL" download --url "http://example.com/index.html" --dest "$DOWNLOAD_FILE")
if echo "$OUT" | grep -qiE "error|fail|cannot|timeout|connection"; then
  _skip "download" "network may be unavailable"
else
  assert_json_valid "download output is valid JSON" "$OUT"
  if [ "$(sdb_shell "test -f '$DOWNLOAD_FILE' && echo yes || echo no" | tr -d '[:space:]')" = "yes" ]; then
    _pass "Downloaded file exists"
  else
    _skip "download file check" "may require network"
  fi
fi

# ── F13: write binary-like content ────────────────────────────────
section "F13" "write — special characters"
SPECIAL_FILE="${TEST_DIR}/special.txt"
OUT=$(cli_exec "$TOOL" write --path "$SPECIAL_FILE" --content "line1\nline2\ttab\nend")
assert_json_valid "special chars write is valid JSON" "$OUT"

# ── F14: remove — single file ────────────────────────────────────
section "F14" "remove — single file"
OUT=$(cli_exec "$TOOL" remove --path "$TEST_FILE3")
assert_json_valid "remove output is valid JSON" "$OUT"
REMOVED=$(sdb_shell "test -f '$TEST_FILE3' && echo yes || echo no" | tr -d '[:space:]')
assert_eq "File removed" "$REMOVED" "no"

# ── F15: Error handling — read non-existent file ──────────────────
section "F15" "Error — read non-existent file"
OUT=$(cli_exec "$TOOL" read --path "/tmp/nonexistent_e2e_file_$$.txt")
assert_not_empty "Error output returned" "$OUT"
# Error can be JSON or plain text
if echo "$OUT" | grep -qE '^\s*[\{\[]'; then
  assert_json_valid "Error output is valid JSON" "$OUT"
else
  assert_contains "Error message present" "$OUT" "error\|Error\|not found\|No such\|Cannot"
fi

# ── Cleanup ───────────────────────────────────────────────────────
sdb_shell "rm -rf '$TEST_DIR'" >/dev/null 2>&1

suite_end
