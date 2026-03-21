#!/bin/bash
# ═══════════════════════════════════════════════════════════════════
# TizenClaw Test Suite — Master Runner
#
# Discovers and runs all test scripts under tests/, aggregating
# results into a final summary report.
#
# Usage:
#   ./tests/run_all.sh                         # Run all suites
#   ./tests/run_all.sh -d <serial>             # Target specific device
#   ./tests/run_all.sh -s cli_tools            # Run only cli_tools suite
#   ./tests/run_all.sh -s service,mcp          # Run specific suites
#   ./tests/run_all.sh --list                  # List available suites
#
# Exit codes:
#   0 = all suites passed
#   1 = one or more suites failed
# ═══════════════════════════════════════════════════════════════════

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# ─── Colors ───────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m'

# ─── Config ──────────────────────────────────────────────────────
DEVICE_SERIAL=""
VERBOSE=0
TIMEOUT=30
SELECTED_SUITES=""
LIST_ONLY=0

# ─── Suite order ─────────────────────────────────────────────────
SUITE_ORDER=(
  "service"
  "cli_tools"
  "embedded_tools"
  "llm_integration"
  "mcp"
  "regression"
)

# ─── Argument parsing ───────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    -d|--device) DEVICE_SERIAL="$2"; shift 2 ;;
    -s|--suite) SELECTED_SUITES="$2"; shift 2 ;;
    -v|--verbose) VERBOSE=1; shift ;;
    -t|--timeout) TIMEOUT="$2"; shift 2 ;;
    --list) LIST_ONLY=1; shift ;;
    -h|--help)
      echo "TizenClaw Test Automation Runner"
      echo ""
      echo "Usage: $0 [options]"
      echo ""
      echo "Options:"
      echo "  -d, --device <serial>     Target device serial"
      echo "  -s, --suite <names>       Comma-separated suite names to run"
      echo "  -v, --verbose             Enable verbose output"
      echo "  -t, --timeout <seconds>   Per-command timeout (default: 30)"
      echo "  --list                    List available test suites"
      echo "  -h, --help               Show this help"
      echo ""
      echo "Available suites: ${SUITE_ORDER[*]}"
      exit 0
      ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

# ─── List mode ───────────────────────────────────────────────────
if [ "$LIST_ONLY" -eq 1 ]; then
  echo -e "${BOLD}Available Test Suites:${NC}"
  echo ""
  for suite in "${SUITE_ORDER[@]}"; do
    suite_dir="${SCRIPT_DIR}/${suite}"
    if [ -d "$suite_dir" ]; then
      count=$(find "$suite_dir" -name "test_*.sh" -type f | wc -l)
      echo -e "  ${CYAN}${suite}${NC} (${count} test files)"
      find "$suite_dir" -name "test_*.sh" -type f | sort | while read -r f; do
        echo -e "    ${DIM}$(basename "$f")${NC}"
      done
    fi
  done
  exit 0
fi

# ─── Build suite list ────────────────────────────────────────────
if [ -n "$SELECTED_SUITES" ]; then
  IFS=',' read -ra RUN_SUITES <<< "$SELECTED_SUITES"
else
  RUN_SUITES=("${SUITE_ORDER[@]}")
fi

# ─── Forward common args ────────────────────────────────────────
FORWARD_ARGS=""
[ -n "$DEVICE_SERIAL" ] && FORWARD_ARGS="$FORWARD_ARGS -d $DEVICE_SERIAL"
[ "$VERBOSE" -eq 1 ] && FORWARD_ARGS="$FORWARD_ARGS -v"
[ "$TIMEOUT" -ne 30 ] && FORWARD_ARGS="$FORWARD_ARGS -t $TIMEOUT"

# ═══════════════════════════════════════════════════════════════════
# Main Execution
# ═══════════════════════════════════════════════════════════════════

TOTAL_SUITES=0
PASSED_SUITES=0
FAILED_SUITES=0
SKIPPED_SUITES=0
SUITE_RESULTS=()
OVERALL_START=$(date +%s)

echo ""
echo -e "${BOLD}╔══════════════════════════════════════════════════════╗${NC}"
echo -e "${BOLD}║       TizenClaw Automated Test Suite                ║${NC}"
echo -e "${BOLD}║       $(date '+%Y-%m-%d %H:%M:%S')                        ║${NC}"
echo -e "${BOLD}╚══════════════════════════════════════════════════════╝${NC}"
echo ""

# Pre-flight: check sdb connection
echo -e "${DIM}Checking device connection...${NC}"
if [ -n "$DEVICE_SERIAL" ]; then
  SDB_STATUS=$(sdb -s "$DEVICE_SERIAL" shell "echo ok" 2>/dev/null | tr -d '[:space:]')
else
  SDB_STATUS=$(sdb shell "echo ok" 2>/dev/null | tr -d '[:space:]')
fi

if [ "$SDB_STATUS" != "ok" ]; then
  echo -e "${RED}Error: Cannot connect to device.${NC}"
  echo "  Ensure a device is connected via 'sdb devices'."
  exit 1
fi
echo -e "${GREEN}Device connected.${NC}"
echo ""

# ─── Run each suite ─────────────────────────────────────────────
for suite in "${RUN_SUITES[@]}"; do
  suite_dir="${SCRIPT_DIR}/${suite}"

  if [ ! -d "$suite_dir" ]; then
    echo -e "${YELLOW}[SKIP]${NC} Suite '${suite}' — directory not found"
    ((SKIPPED_SUITES++))
    SUITE_RESULTS+=("SKIP:${suite}")
    continue
  fi

  test_files=$(find "$suite_dir" -name "test_*.sh" -type f | sort)
  if [ -z "$test_files" ]; then
    echo -e "${YELLOW}[SKIP]${NC} Suite '${suite}' — no test files"
    ((SKIPPED_SUITES++))
    SUITE_RESULTS+=("SKIP:${suite}")
    continue
  fi

  echo -e "${MAGENTA}━━━ Suite: ${suite} ━━━${NC}"
  suite_failed=0
  ((TOTAL_SUITES++))

  for test_file in $(find "$suite_dir" -name "test_*.sh" -type f | sort); do
    echo -e "${DIM}Running $(basename "$test_file")...${NC}"
    # shellcheck disable=SC2086
    if bash "$test_file" $FORWARD_ARGS; then
      : # passed
    else
      suite_failed=1
    fi
  done

  if [ "$suite_failed" -eq 0 ]; then
    echo -e "${GREEN}[PASS]${NC} Suite '${suite}' completed"
    ((PASSED_SUITES++))
    SUITE_RESULTS+=("PASS:${suite}")
  else
    echo -e "${RED}[FAIL]${NC} Suite '${suite}' had failures"
    ((FAILED_SUITES++))
    SUITE_RESULTS+=("FAIL:${suite}")
  fi
  echo ""
done

# ═══════════════════════════════════════════════════════════════════
# Final Summary
# ═══════════════════════════════════════════════════════════════════
OVERALL_END=$(date +%s)
OVERALL_ELAPSED=$((OVERALL_END - OVERALL_START))

echo -e "${BOLD}╔══════════════════════════════════════════════════════╗${NC}"
echo -e "${BOLD}║                  FINAL REPORT                       ║${NC}"
echo -e "${BOLD}╠══════════════════════════════════════════════════════╣${NC}"

for result in "${SUITE_RESULTS[@]}"; do
  status="${result%%:*}"
  name="${result#*:}"
  case "$status" in
    PASS) echo -e "${BOLD}║${NC}  ${GREEN}✓ ${name}${NC}" ;;
    FAIL) echo -e "${BOLD}║${NC}  ${RED}✗ ${name}${NC}" ;;
    SKIP) echo -e "${BOLD}║${NC}  ${YELLOW}○ ${name} (skipped)${NC}" ;;
  esac
done

echo -e "${BOLD}╠══════════════════════════════════════════════════════╣${NC}"

if [ "$FAILED_SUITES" -eq 0 ]; then
  echo -e "${BOLD}║${NC}  ${GREEN}${BOLD}ALL SUITES PASSED${NC}"
else
  echo -e "${BOLD}║${NC}  ${RED}${BOLD}${FAILED_SUITES} SUITE(S) FAILED${NC}"
fi

echo -e "${BOLD}║${NC}  ${DIM}Passed: ${PASSED_SUITES} | Failed: ${FAILED_SUITES} | Skipped: ${SKIPPED_SUITES}${NC}"
echo -e "${BOLD}║${NC}  ${DIM}Total duration: ${OVERALL_ELAPSED}s${NC}"
echo -e "${BOLD}╚══════════════════════════════════════════════════════╝${NC}"
echo ""

[ "$FAILED_SUITES" -eq 0 ] && exit 0 || exit 1
