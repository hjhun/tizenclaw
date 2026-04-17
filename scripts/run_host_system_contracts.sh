#!/bin/bash
# Run the offline system contract suite against a temporary isolated runtime
# root. Invoked automatically by ./deploy_host.sh --test after the
# cargo/parity/doc checks.
#
# Usage: scripts/run_host_system_contracts.sh --bin-dir <dir>
#   --bin-dir: directory containing tizenclaw, tizenclaw-tool-executor,
#              tizenclaw-web-dashboard, and tizenclaw-tests binaries
#
# Environment variables honoured:
#   TIZENCLAW_CONTRACT_TIMEOUT  seconds to wait for IPC readiness (default 20)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
SUITE_MANIFEST="${PROJECT_DIR}/tests/system/offline_suite.json"
IPC_TIMEOUT="${TIZENCLAW_CONTRACT_TIMEOUT:-20}"

# ─────────────────────────────────────────────
# Log helpers
# ─────────────────────────────────────────────
CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
RED='\033[0;31m'
BOLD='\033[1m'
NC='\033[0m'

log()  { echo -e "${CYAN}[CONTRACTS]${NC} $*"; }
ok()   { echo -e "${GREEN}[  OK  ]${NC} $*"; }
warn() { echo -e "${YELLOW}[ WARN ]${NC} $*"; }
err()  { echo -e "${RED}[ FAIL ]${NC} $*"; }

# ─────────────────────────────────────────────
# Argument parsing
# ─────────────────────────────────────────────
BIN_DIR=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bin-dir)
      [[ $# -lt 2 ]] && { err "--bin-dir requires a path argument"; exit 1; }
      BIN_DIR="$(realpath "$2")"; shift 2 ;;
    *)
      err "Unknown option: $1"; exit 1 ;;
  esac
done

if [[ -z "${BIN_DIR}" ]]; then
  err "--bin-dir is required"
  exit 1
fi

DAEMON_BIN="${BIN_DIR}/tizenclaw"
TOOL_EXECUTOR_BIN="${BIN_DIR}/tizenclaw-tool-executor"
TESTS_BIN="${BIN_DIR}/tizenclaw-tests"

for bin in "${DAEMON_BIN}" "${TOOL_EXECUTOR_BIN}" "${TESTS_BIN}"; do
  if [[ ! -x "${bin}" ]]; then
    err "Required binary not found or not executable: ${bin}"
    err "Run './deploy_host.sh --test' to build before running this script."
    exit 1
  fi
done

# ─────────────────────────────────────────────
# Isolated runtime root
# ─────────────────────────────────────────────
TEST_ROOT="$(mktemp -d)"
TEST_SOCKET="${TEST_ROOT}/tizenclaw.sock"
DAEMON_PID_FILE="${TEST_ROOT}/tizenclaw.pid"
TOOL_EXECUTOR_PID_FILE="${TEST_ROOT}/tool-executor.pid"
DAEMON_LOG="${TEST_ROOT}/tizenclaw.log"
TOOL_EXECUTOR_LOG="${TEST_ROOT}/tool-executor.log"

_kill_pid_file() {
  local pid_file="$1"
  local label="$2"
  if [[ ! -f "${pid_file}" ]]; then
    return 0
  fi
  local pid
  pid="$(cat "${pid_file}" 2>/dev/null || true)"
  if [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null; then
    kill "${pid}" 2>/dev/null || true
    local waited=0
    while kill -0 "${pid}" 2>/dev/null && [[ "${waited}" -lt 5 ]]; do
      sleep 0.5
      waited=$((waited + 1))
    done
    if kill -0 "${pid}" 2>/dev/null; then
      warn "${label} (pid ${pid}) still alive; sending SIGKILL"
      kill -9 "${pid}" 2>/dev/null || true
    fi
  fi
  rm -f "${pid_file}"
}

cleanup() {
  log "Cleaning up isolated test environment..."
  _kill_pid_file "${TOOL_EXECUTOR_PID_FILE}" "tizenclaw-tool-executor"
  _kill_pid_file "${DAEMON_PID_FILE}" "tizenclaw"
  rm -rf "${TEST_ROOT}"
  log "Test environment removed"
}

trap cleanup EXIT INT TERM

# ─────────────────────────────────────────────
# Minimal directory structure and config
# ─────────────────────────────────────────────
log "Setting up isolated runtime root: ${TEST_ROOT}"
mkdir -p \
  "${TEST_ROOT}/config" \
  "${TEST_ROOT}/logs" \
  "${TEST_ROOT}/web" \
  "${TEST_ROOT}/workspace/skills" \
  "${TEST_ROOT}/tools" \
  "${TEST_ROOT}/plugins" \
  "${TEST_ROOT}/embedded"

# Seed a minimal channel_config.json with the web_dashboard disabled so the
# daemon does not try to bind a port automatically on startup. The
# dashboard_runtime_contract and channel_registry scenarios start it explicitly
# through IPC.
cat > "${TEST_ROOT}/config/channel_config.json" <<'JSON'
{
  "channels": [
    {
      "name": "web_dashboard",
      "type": "web_dashboard",
      "enabled": false,
      "settings": {
        "port": 9091,
        "localhost_only": false
      }
    }
  ]
}
JSON

# ─────────────────────────────────────────────
# Pre-flight: warn if the dashboard test port is busy
# ─────────────────────────────────────────────
# dashboard_runtime_contract.json calls dashboard.start with port 9191.
# Warn early rather than let the scenario fail with a cryptic error.
if ss -ltnp "( sport = :9191 )" 2>/dev/null | grep -q ':9191'; then
  warn "Port 9191 is already in use on this host."
  warn "dashboard_runtime_contract.json starts the web dashboard on port 9191."
  warn "That scenario may fail if the port cannot be claimed."
fi

# ─────────────────────────────────────────────
# Export isolation environment
# ─────────────────────────────────────────────
export TIZENCLAW_DATA_DIR="${TEST_ROOT}"
export TIZENCLAW_SOCKET_PATH="${TEST_SOCKET}"
# Put the bin dir first so the daemon finds tizenclaw-web-dashboard next to itself.
export PATH="${BIN_DIR}:${PATH}"

# ─────────────────────────────────────────────
# Start companion processes
# ─────────────────────────────────────────────
log "Starting tizenclaw-tool-executor (isolated)..."
setsid "${TOOL_EXECUTOR_BIN}" \
  >> "${TOOL_EXECUTOR_LOG}" 2>&1 < /dev/null &
echo $! > "${TOOL_EXECUTOR_PID_FILE}"
ok "tizenclaw-tool-executor started (pid $(cat "${TOOL_EXECUTOR_PID_FILE}"))"

log "Starting tizenclaw daemon (isolated)..."
setsid "${DAEMON_BIN}" \
  >> "${DAEMON_LOG}" 2>&1 < /dev/null &
echo $! > "${DAEMON_PID_FILE}"
ok "tizenclaw daemon started (pid $(cat "${DAEMON_PID_FILE}"))"

# ─────────────────────────────────────────────
# Wait for IPC readiness
# ─────────────────────────────────────────────
log "Waiting up to ${IPC_TIMEOUT}s for daemon IPC to become ready..."
DEADLINE=$((SECONDS + IPC_TIMEOUT))
IPC_READY=false

while [[ "${SECONDS}" -lt "${DEADLINE}" ]]; do
  if "${TESTS_BIN}" call \
      --method ping \
      --socket-path "${TEST_SOCKET}" \
      >/dev/null 2>&1; then
    IPC_READY=true
    break
  fi

  # Bail early if the daemon process exited unexpectedly.
  DAEMON_PID="$(cat "${DAEMON_PID_FILE}" 2>/dev/null || true)"
  if [[ -n "${DAEMON_PID}" ]] && ! kill -0 "${DAEMON_PID}" 2>/dev/null; then
    warn "Daemon process exited before IPC became ready."
    break
  fi

  sleep 0.5
done

if [[ "${IPC_READY}" != "true" ]]; then
  err "Daemon IPC did not become ready within ${IPC_TIMEOUT}s"
  if [[ -f "${DAEMON_LOG}" ]]; then
    warn "Last 30 lines of daemon log (${DAEMON_LOG}):"
    tail -30 "${DAEMON_LOG}" 2>/dev/null || true
  fi
  exit 1
fi

ok "Daemon IPC is ready (${TEST_SOCKET})"

# ─────────────────────────────────────────────
# Read suite manifest and run scenarios
# ─────────────────────────────────────────────
if [[ ! -f "${SUITE_MANIFEST}" ]]; then
  err "Suite manifest not found: ${SUITE_MANIFEST}"
  exit 1
fi

# Extract scenario paths from the manifest JSON using python3.
mapfile -t SCENARIOS < <(python3 - <<PYEOF
import json, sys
try:
    data = json.loads(open("${SUITE_MANIFEST}").read())
    for s in data.get("scenarios", []):
        print(s)
except Exception as exc:
    sys.stderr.write("Failed to parse suite manifest: {}\n".format(exc))
    sys.exit(1)
PYEOF
)

FAILED=0
PASSED=0
FAILED_NAMES=()

echo ""
echo -e "${BOLD}── Offline System Contract Suite ──────────────────────────────${NC}"

for scenario in "${SCENARIOS[@]}"; do
  scenario_path="${PROJECT_DIR}/${scenario}"

  if [[ ! -f "${scenario_path}" ]]; then
    err "Scenario file not found: ${scenario_path}"
    FAILED=$((FAILED + 1))
    FAILED_NAMES+=("${scenario}")
    continue
  fi

  echo ""
  log "Running: ${scenario}"
  if "${TESTS_BIN}" scenario \
      --file "${scenario_path}" \
      --socket-path "${TEST_SOCKET}" 2>&1; then
    ok "PASS: ${scenario}"
    PASSED=$((PASSED + 1))
  else
    err "FAIL: ${scenario}"
    FAILED=$((FAILED + 1))
    FAILED_NAMES+=("${scenario}")
  fi
done

# ─────────────────────────────────────────────
# Results
# ─────────────────────────────────────────────
echo ""
echo -e "${BOLD}── Suite Results ────────────────────────────────────────────────${NC}"
echo -e "  Passed : ${PASSED}"
echo -e "  Failed : ${FAILED}"

if [[ "${FAILED}" -gt 0 ]]; then
  echo -e "${RED}  Failed scenarios:${NC}"
  for name in "${FAILED_NAMES[@]}"; do
    echo -e "    - ${name}"
  done
  echo ""
  err "Offline contract suite: ${FAILED} scenario(s) failed"
  exit 1
fi

echo ""
ok "All ${PASSED} offline contract scenario(s) passed"
