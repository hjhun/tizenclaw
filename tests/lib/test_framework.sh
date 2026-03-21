#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# TizenClaw Test Framework Library
# Shared assertion helpers, device wrappers, and report utilities.
#
# Source this file at the top of every test script:
#   source "$(dirname "$0")/../lib/test_framework.sh"
# ═══════════════════════════════════════════════════════════════════

# ─── Strict mode ──────────────────────────────────────────────────
set -uo pipefail

# ─── Colors ───────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m'

# ─── Counters ─────────────────────────────────────────────────────
TC_PASS=0
TC_FAIL=0
TC_SKIP=0
TC_SUITE_NAME=""
TC_START_TIME=0

# ─── Global config ───────────────────────────────────────────────
TC_DEVICE_SERIAL="${TC_DEVICE_SERIAL:-}"
TC_TIMEOUT="${TC_TIMEOUT:-30}"
TC_VERBOSE="${TC_VERBOSE:-0}"
TC_CLI_BASE="/opt/usr/share/tizenclaw/tools/cli"

# ═══════════════════════════════════════════════════════════════════
# Device Communication
# ═══════════════════════════════════════════════════════════════════

# sdb wrapper that honors TC_DEVICE_SERIAL
sdb_cmd() {
  if [ -n "${TC_DEVICE_SERIAL}" ]; then
    sdb -s "${TC_DEVICE_SERIAL}" "$@"
  else
    sdb "$@"
  fi
}

# Execute a shell command on the device
sdb_shell() {
  sdb_cmd shell "$@" 2>/dev/null
}

# Execute a CLI tool on the device
# Usage: cli_exec <tool-name> <args...>
cli_exec() {
  local tool="$1"; shift
  sdb_shell "${TC_CLI_BASE}/${tool}/${tool}" "$@" 2>/dev/null
}

# Execute a CLI tool and return parsed JSON field
# Usage: cli_json_field <tool-name> <jq-expr> <args...>
cli_json_field() {
  local tool="$1"; shift
  local jq_expr="$1"; shift
  local output
  output=$(cli_exec "$tool" "$@")
  echo "$output" | jq -r "$jq_expr" 2>/dev/null
}

# Execute tizenclaw-cli with prompt
# Usage: tc_cli <prompt> [extra-args...]
tc_cli() {
  local prompt="$1"; shift
  sdb_shell tizenclaw-cli "$@" "$prompt" 2>/dev/null
}

# Execute tizenclaw-cli with session
# Usage: tc_cli_session <session_id> <prompt>
tc_cli_session() {
  local session="$1"; shift
  local prompt="$1"; shift
  sdb_shell tizenclaw-cli -s "$session" "$prompt" 2>/dev/null
}

# ═══════════════════════════════════════════════════════════════════
# Device Info Helpers
# ═══════════════════════════════════════════════════════════════════

# Detect device profile (tv, wearable, mobile, refrigerator, etc.)
tc_device_profile() {
  sdb_shell "cat /etc/tizen-release 2>/dev/null | grep PROFILE | cut -d= -f2" \
    | tr -d '[:space:]' || echo "unknown"
}

# Check if the device has a specific feature
# Usage: tc_has_feature <feature-string>
tc_has_feature() {
  local feature="$1"
  local result
  result=$(sdb_shell "cat /etc/config/model-config.xml 2>/dev/null" \
    | grep -c "$feature" || echo 0)
  [ "$result" -gt 0 ]
}

# Check if a binary/tool exists on the device
tc_tool_exists() {
  local path="$1"
  local result
  result=$(sdb_shell "test -f '$path' && echo yes || echo no")
  [ "$(echo "$result" | tr -d '[:space:]')" = "yes" ]
}

# ═══════════════════════════════════════════════════════════════════
# Assertion Functions
# ═══════════════════════════════════════════════════════════════════

# PASS helper
_pass() {
  local desc="$1"
  echo -e "  ${GREEN}[PASS]${NC} $desc"
  ((TC_PASS++))
}

# FAIL helper
_fail() {
  local desc="$1"; shift
  echo -e "  ${RED}[FAIL]${NC} $desc"
  for detail in "$@"; do
    echo -e "       ${DIM}${detail}${NC}"
  done
  ((TC_FAIL++))
}

# SKIP helper
_skip() {
  local desc="$1" reason="$2"
  echo -e "  ${YELLOW}[SKIP]${NC} $desc ${DIM}(${reason})${NC}"
  ((TC_SKIP++))
}

# Assert that output contains a pattern (regex)
# Usage: assert_contains <description> <output> <pattern>
assert_contains() {
  local desc="$1" output="$2" pattern="$3"
  if echo "$output" | grep -qi "$pattern"; then
    _pass "$desc"
  else
    _fail "$desc" \
      "expected pattern: ${pattern}" \
      "actual (200 chars): ${output:0:200}"
  fi
}

# Assert that output does NOT contain a pattern
assert_not_contains() {
  local desc="$1" output="$2" pattern="$3"
  if echo "$output" | grep -qi "$pattern"; then
    _fail "$desc" \
      "unexpected pattern found: ${pattern}" \
      "actual (200 chars): ${output:0:200}"
  else
    _pass "$desc"
  fi
}

# Assert output is not empty
assert_not_empty() {
  local desc="$1" output="$2"
  if [ -n "$output" ]; then
    _pass "$desc"
  else
    _fail "$desc" "output was empty"
  fi
}

# Assert output is empty
assert_empty() {
  local desc="$1" output="$2"
  if [ -z "$output" ]; then
    _pass "$desc"
  else
    _fail "$desc" "expected empty, got: ${output:0:100}"
  fi
}

# Assert numeric value >= minimum
assert_ge() {
  local desc="$1" actual="$2" min="$3"
  if [ "$actual" -ge "$min" ] 2>/dev/null; then
    _pass "$desc (${actual} >= ${min})"
  else
    _fail "$desc" "expected >= ${min}, got: ${actual}"
  fi
}

# Assert numeric value <= maximum
assert_le() {
  local desc="$1" actual="$2" max="$3"
  if [ "$actual" -le "$max" ] 2>/dev/null; then
    _pass "$desc (${actual} <= ${max})"
  else
    _fail "$desc" "expected <= ${max}, got: ${actual}"
  fi
}

# Assert two values are equal
assert_eq() {
  local desc="$1" actual="$2" expected="$3"
  if [ "$actual" = "$expected" ]; then
    _pass "$desc"
  else
    _fail "$desc" "expected: ${expected}" "actual: ${actual}"
  fi
}

# Assert two values are not equal
assert_ne() {
  local desc="$1" actual="$2" unexpected="$3"
  if [ "$actual" != "$unexpected" ]; then
    _pass "$desc"
  else
    _fail "$desc" "value should not be: ${unexpected}"
  fi
}

# Assert command exit code is zero
assert_success() {
  local desc="$1"; shift
  if "$@" >/dev/null 2>&1; then
    _pass "$desc"
  else
    _fail "$desc" "command failed: $*"
  fi
}

# Assert a file exists on the device
assert_file_exists() {
  local desc="$1" path="$2"
  local exists
  exists=$(sdb_shell "test -f '$path' && echo yes || echo no")
  if [ "$(echo "$exists" | tr -d '[:space:]')" = "yes" ]; then
    _pass "$desc"
  else
    _fail "$desc" "file not found: ${path}"
  fi
}

# Assert a directory exists on the device
assert_dir_exists() {
  local desc="$1" path="$2"
  local exists
  exists=$(sdb_shell "test -d '$path' && echo yes || echo no")
  if [ "$(echo "$exists" | tr -d '[:space:]')" = "yes" ]; then
    _pass "$desc"
  else
    _fail "$desc" "directory not found: ${path}"
  fi
}

# Check if jq is available
_has_jq() {
  command -v jq &>/dev/null
}

# Assert JSON output has valid structure
# Usage: assert_json_valid <description> <json_output>
assert_json_valid() {
  local desc="$1" json="$2"
  if _has_jq; then
    if echo "$json" | jq empty 2>/dev/null; then
      _pass "$desc"
    else
      _fail "$desc" "invalid JSON: ${json:0:200}"
    fi
  else
    # Fallback: basic check for JSON-like structure
    if echo "$json" | grep -qE '^\s*[\{\[]'; then
      _pass "$desc (basic check)"
    else
      _fail "$desc" "does not look like JSON: ${json:0:200}"
    fi
  fi
}

# Assert JSON field matches expected value
# Usage: assert_json_eq <description> <json> <jq_expr> <expected>
assert_json_eq() {
  local desc="$1" json="$2" expr="$3" expected="$4"
  if ! _has_jq; then
    _skip "$desc" "jq not installed"
    return
  fi
  local actual
  actual=$(echo "$json" | jq -r "$expr" 2>/dev/null)
  if [ "$actual" = "$expected" ]; then
    _pass "$desc"
  else
    _fail "$desc" "jq($expr) expected: ${expected}, got: ${actual}"
  fi
}

# Assert JSON jq expression evaluates to true
# Usage: assert_json <description> <json> <jq_expr>
assert_json() {
  local desc="$1" json="$2" expr="$3"
  if ! _has_jq; then
    _skip "$desc" "jq not installed"
    return
  fi
  if echo "$json" | jq -e "$expr" >/dev/null 2>&1; then
    _pass "$desc"
  else
    _fail "$desc" "jq expression false: ${expr}" "json: ${json:0:300}"
  fi
}

# Assert JSON array length >= minimum
assert_json_array_ge() {
  local desc="$1" json="$2" expr="$3" min="$4"
  if ! _has_jq; then
    _skip "$desc" "jq not installed"
    return
  fi
  local count
  count=$(echo "$json" | jq "$expr | length" 2>/dev/null || echo 0)
  assert_ge "$desc" "$count" "$min"
}

# ═══════════════════════════════════════════════════════════════════
# Suite Lifecycle
# ═══════════════════════════════════════════════════════════════════

# Begin a named test suite
suite_begin() {
  TC_SUITE_NAME="$1"
  TC_PASS=0
  TC_FAIL=0
  TC_SKIP=0
  TC_START_TIME=$(date +%s)

  echo ""
  echo -e "${BOLD}══════════════════════════════════════════════════════${NC}"
  echo -e "${BOLD}  ${TC_SUITE_NAME}${NC}"
  echo -e "${BOLD}══════════════════════════════════════════════════════${NC}"
  echo ""
}

# Begin a named test section within a suite
section() {
  echo -e "\n${CYAN}[$1]${NC} $2"
}

# End the suite and print summary
suite_end() {
  local end_time
  end_time=$(date +%s)
  local elapsed=$((end_time - TC_START_TIME))
  local total=$((TC_PASS + TC_FAIL + TC_SKIP))

  echo ""
  echo -e "${BOLD}══════════════════════════════════════════════════════${NC}"
  if [ "$TC_FAIL" -eq 0 ]; then
    echo -e "  ${GREEN}${BOLD}ALL PASSED${NC}: ${TC_PASS} passed"
    [ "$TC_SKIP" -gt 0 ] && echo -e "  ${YELLOW}${TC_SKIP} skipped${NC}"
  else
    echo -e "  ${RED}${BOLD}FAILED${NC}: ${TC_PASS} passed, ${RED}${TC_FAIL} failed${NC}"
    [ "$TC_SKIP" -gt 0 ] && echo -e "  ${YELLOW}${TC_SKIP} skipped${NC}"
  fi
  echo -e "  ${DIM}Total: ${total} | Duration: ${elapsed}s${NC}"
  echo -e "${BOLD}══════════════════════════════════════════════════════${NC}"
  echo ""

  [ "$TC_FAIL" -eq 0 ] && return 0 || return 1
}

# ═══════════════════════════════════════════════════════════════════
# Argument Parsing (common to all test scripts)
# ═══════════════════════════════════════════════════════════════════

tc_parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      -d|--device) TC_DEVICE_SERIAL="$2"; shift 2 ;;
      -v|--verbose) TC_VERBOSE=1; shift ;;
      -t|--timeout) TC_TIMEOUT="$2"; shift 2 ;;
      -h|--help)
        echo "Usage: $(basename "$0") [-d <device-serial>] [-v] [-t <timeout>]"
        echo ""
        echo "Options:"
        echo "  -d, --device   Target device serial (from 'sdb devices')"
        echo "  -v, --verbose  Enable verbose output"
        echo "  -t, --timeout  Command timeout in seconds (default: 30)"
        exit 0
        ;;
      *) echo "Unknown option: $1"; exit 1 ;;
    esac
  done
}

# ═══════════════════════════════════════════════════════════════════
# Pre-flight checks
# ═══════════════════════════════════════════════════════════════════

# Verify sdb connection is alive
tc_preflight() {
  echo -e "${DIM}Pre-flight checks...${NC}"

  # Check sdb availability
  if ! command -v sdb &>/dev/null; then
    echo -e "${RED}Error: sdb not found in PATH${NC}"
    exit 1
  fi

  # Check device connectivity
  local status
  status=$(sdb_shell "echo ok" | tr -d '[:space:]')
  if [ "$status" != "ok" ]; then
    echo -e "${RED}Error: Cannot connect to device${NC}"
    echo "  Ensure 'sdb devices' shows a connected device."
    [ -n "$TC_DEVICE_SERIAL" ] && echo "  Requested serial: $TC_DEVICE_SERIAL"
    exit 1
  fi

  # Check jq availability (on host)
  if ! command -v jq &>/dev/null; then
    echo -e "${YELLOW}Warning: jq not found — JSON assertions will be limited${NC}"
  fi

  echo -e "${DIM}Pre-flight OK${NC}"
}

# ═══════════════════════════════════════════════════════════════════
# Logging helpers
# ═══════════════════════════════════════════════════════════════════

tc_log() {
  [ "$TC_VERBOSE" -eq 1 ] && echo -e "  ${DIM}[LOG] $*${NC}"
}

tc_warn() {
  echo -e "  ${YELLOW}[WARN]${NC} $*"
}

tc_info() {
  echo -e "  ${MAGENTA}[INFO]${NC} $*"
}
