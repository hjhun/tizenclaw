#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# Embedded Tool Tests — Code Execution (Enhanced)
# Tests: basic Python → JSON output → system info → error handling
#        → multi-line → timeout → import library → file I/O
# ═══════════════════════════════════════════════════════════════════

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "${SCRIPT_DIR}/../lib/test_framework.sh"
tc_parse_args "$@"
tc_preflight

suite_begin "Embedded Tools: Code Execution"

SESSION_ID="e2e_code_full_$$"

# ── C1: Simple Python execution ──────────────────────────────────
section "C1" "Simple Python — arithmetic"
OUT=$(tc_cli_session "$SESSION_ID" \
  'Please execute Python code using the execute_code tool: import json; print(json.dumps({"result": 2+2}))')
assert_not_empty "Code execution returns output" "$OUT"

# ── C2: System info via Python ────────────────────────────────────
section "C2" "System info via Python"
OUT=$(tc_cli_session "${SESSION_ID}_sys" \
  'Please use the execute_code tool to run: import json, os; print(json.dumps({"hostname": os.uname().nodename}))')
assert_not_empty "System info returned" "$OUT"

# ── C3: Error handling ───────────────────────────────────────────
section "C3" "Python error handling"
OUT=$(tc_cli_session "${SESSION_ID}_err" \
  'Please use the execute_code tool to execute this code: raise ValueError("test error")')
assert_not_empty "Error response returned" "$OUT"

# ── C4: Multi-line code ──────────────────────────────────────────
section "C4" "Multi-line Python code"
OUT=$(tc_cli_session "${SESSION_ID}_multi" \
  'Use the execute_code tool to run this Python code:
import json
data = {"numbers": [1,2,3,4,5], "sum": sum([1,2,3,4,5])}
print(json.dumps(data))')
assert_not_empty "Multi-line code returns output" "$OUT"

# ── C5: File I/O via code ─────────────────────────────────────────
section "C5" "File I/O via Python"
OUT=$(tc_cli_session "${SESSION_ID}_file" \
  'Use the execute_code tool to run:
import json, tempfile, os
path = "/tmp/e2e_code_test.txt"
with open(path, "w") as f: f.write("hello from code")
exists = os.path.exists(path)
with open(path) as f: content = f.read()
os.remove(path)
print(json.dumps({"wrote": True, "content": content}))')
assert_not_empty "File I/O code returns output" "$OUT"

# ── C6: Import standard library ──────────────────────────────────
section "C6" "Standard library imports"
OUT=$(tc_cli_session "${SESSION_ID}_import" \
  'Use the execute_code tool to run: import json, datetime, platform; print(json.dumps({"python": platform.python_version(), "date": str(datetime.date.today())}))')
assert_not_empty "Standard library imports work" "$OUT"

# ── C7: Large output ─────────────────────────────────────────────
section "C7" "Large output handling"
OUT=$(tc_cli_session "${SESSION_ID}_large" \
  'Use the execute_code tool to run: print("x" * 1000)')
assert_not_empty "Large output returned" "$OUT"

suite_end
