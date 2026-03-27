#!/bin/bash
# TizenClaw Full Verification Test Runner
# Runs all automated test suites after deployment.
#
# Called by: deploy.sh -T (--full-test)
#
# Usage:
#   ./tests/verification/run_all.sh [-d <device-serial>]

set -uo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

PROJECT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
DEVICE_SERIAL=""
SUITE_PASS=0
SUITE_FAIL=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    -d|--device) DEVICE_SERIAL="$2"; shift 2 ;;
    -h|--help) echo "Usage: $0 [-d <device-serial>]"; exit 0 ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

if ! command -v sdb &>/dev/null; then
  for _c in "${HOME}/tizen-studio/tools" "/opt/tizen-studio/tools"; do
    [ -x "${_c}/sdb" ] && export PATH="${_c}:${PATH}" && break
  done
fi

sdb_cmd() {
  if [ -n "${DEVICE_SERIAL}" ]; then sdb -s "${DEVICE_SERIAL}" "$@"
  else sdb "$@"; fi
}
sdb_shell() { sdb_cmd shell "$@" 2>/dev/null; }

run_suite() {
  local name="$1" script="$2"
  echo -e "\n${CYAN}── Suite: ${name}${NC}"
  if [ ! -f "${script}" ]; then
    echo -e "  ${YELLOW}[SKIP]${NC} ${script} not found"; return; fi
  local args=()
  [ -n "${DEVICE_SERIAL}" ] && args+=("-d" "${DEVICE_SERIAL}")
  if bash "${script}" "${args[@]+"${args[@]}"}"; then
    echo -e "  ${GREEN}[PASS]${NC} ${name}"; ((SUITE_PASS++))
  else
    echo -e "  ${RED}[FAIL]${NC} ${name}"; ((SUITE_FAIL++))
  fi
}

run_device_tests() {
  echo -e "\n${CYAN}── Suite: Device Binary Checks${NC}"
  local pass=0 fail=0

  for bin in tizenclaw tizenclaw-cli tizenclaw-tool-executor; do
    local e; e=$(sdb_shell "test -f /usr/bin/${bin} && echo yes || echo no")
    if [ "${e}" = "yes" ]; then
      echo -e "  ${GREEN}[PASS]${NC} /usr/bin/${bin}"; ((pass++))
    else
      echo -e "  ${RED}[FAIL]${NC} /usr/bin/${bin} missing"; ((fail++))
    fi
  done

  for lib in libtizenclaw.so libtizenclaw_core.so; do
    local e; e=$(sdb_shell "test -f /usr/lib64/${lib} && echo yes || echo no")
    if [ "${e}" = "yes" ]; then
      echo -e "  ${GREEN}[PASS]${NC} /usr/lib64/${lib}"; ((pass++))
    else
      echo -e "  ${RED}[FAIL]${NC} /usr/lib64/${lib} missing"; ((fail++))
    fi
  done

  local st; st=$(sdb_shell systemctl is-active tizenclaw 2>/dev/null | tr -d '[:space:]')
  if [ "${st}" = "active" ]; then
    echo -e "  ${GREEN}[PASS]${NC} tizenclaw.service active"; ((pass++))
  else
    echo -e "  ${RED}[FAIL]${NC} tizenclaw.service ${st}"; ((fail++))
  fi

  local ss; ss=$(sdb_shell systemctl is-active tizenclaw-tool-executor.socket 2>/dev/null | tr -d '[:space:]')
  if [ "${ss}" = "active" ] || [ "${ss}" = "listening" ]; then
    echo -e "  ${GREEN}[PASS]${NC} tool-executor.socket ${ss}"; ((pass++))
  else
    echo -e "  ${RED}[FAIL]${NC} tool-executor.socket ${ss}"; ((fail++))
  fi

  if [ "${fail}" -eq 0 ]; then
    echo -e "  ${GREEN}Device checks: ${pass} passed${NC}"; ((SUITE_PASS++))
  else
    echo -e "  ${RED}Device checks: ${pass} passed, ${fail} failed${NC}"; ((SUITE_FAIL++))
  fi
}

echo -e "\n${BOLD}══════════════════════════════════════════${NC}"
echo -e "${BOLD}  TizenClaw Full Verification Suite${NC}"
echo -e "${BOLD}══════════════════════════════════════════${NC}"

run_device_tests
run_suite "E2E Smoke Test" "${PROJECT_DIR}/tests/e2e/test_smoke.sh"
run_suite "MCP Protocol Test" "${PROJECT_DIR}/tests/e2e/test_mcp.sh"

echo -e "\n${BOLD}══════════════════════════════════════════${NC}"
TOTAL=$((SUITE_PASS + SUITE_FAIL))
if [ "${SUITE_FAIL}" -eq 0 ]; then
  echo -e "  ${GREEN}${BOLD}ALL SUITES PASSED${NC}: ${SUITE_PASS}/${TOTAL}"
else
  echo -e "  ${RED}${BOLD}FAILED${NC}: ${SUITE_PASS} passed, ${SUITE_FAIL} failed"
fi
echo -e "${BOLD}══════════════════════════════════════════${NC}\n"

[ "${SUITE_FAIL}" -eq 0 ] && exit 0 || exit 1
