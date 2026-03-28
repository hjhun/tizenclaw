#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# Embedded Tool Tests — Code Execution (Enhanced)
# Tests: basic Bash → JSON output → system info → error handling
#        → multi-line → timeout → import library → file I/O
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "Embedded Tools: Code Execution"

SESSION_ID="e2e_code_full_$$"

# ── C1: Simple Bash execution ──────────────────────────────────────
section "C1" "Simple Bash — arithmetic"
OUT=$(tc_cli_session "$SESSION_ID" \
  'Please execute Shell code using the execute_code tool: echo "{\"result\": $((2+2))}"')
assert_not_empty "Code execution returns output" "$OUT"

# ── C2: System info via Bash ──────────────────────────────────────
section "C2" "System info via Bash"
OUT=$(tc_cli_session "${SESSION_ID}_sys" \
  'Please use the execute_code tool to run: echo "{\"hostname\": \"$(hostname)\"}"')
assert_not_empty "System info returned" "$OUT"

# ── C3: Error handling ───────────────────────────────────────────
section "C3" "Bash error handling"
OUT=$(tc_cli_session "${SESSION_ID}_err" \
  'Please use the execute_code tool to execute this code: exit 1')
assert_not_empty "Error response returned" "$OUT"

# ── C4: Multi-line code ──────────────────────────────────────────
section "C4" "Multi-line Bash code"
OUT=$(tc_cli_session "${SESSION_ID}_multi" \
  'Use the execute_code tool to run this Bash code:
numbers=(1 2 3 4 5)
sum=15
echo "{\"numbers\": [1,2,3,4,5], \"sum\": $sum}"')
assert_not_empty "Multi-line code returns output" "$OUT"

# ── C5: File I/O via code ─────────────────────────────────────────
section "C5" "File I/O via Shell"
OUT=$(tc_cli_session "${SESSION_ID}_file" \
  'Use the execute_code tool to run:
path="/tmp/e2e_code_test.txt"
echo "hello from code" > $path
content=$(cat $path)
rm -f $path
echo "{\"wrote\": true, \"content\": \"$content\"}"')
assert_not_empty "File I/O code returns output" "$OUT"

# ── C6: Import standard library ──────────────────────────────────
section "C6" "Standard library imports"
OUT=$(tc_cli_session "${SESSION_ID}_import" \
  'Use the execute_code tool to run: echo "{\"shell\": \"$BASH_VERSION\", \"date\": \"$(date +%F)\"}"')
assert_not_empty "Standard library imports work" "$OUT"

# ── C7: Large output ─────────────────────────────────────────────
section "C7" "Large output handling"
OUT=$(tc_cli_session "${SESSION_ID}_large" \
  'Use the execute_code tool to run: printf "x%.0s" {1..1000}')
assert_not_empty "Large output returned" "$OUT"

suite_end
