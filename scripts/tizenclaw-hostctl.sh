#!/usr/bin/env bash
# TizenClaw installed-bundle host control script.
#
# This script is intentionally minimal and SHOULD NOT attempt any source-tree
# operations (no cargo, no git, no repo-relative paths). It manages the
# lifecycle of an installed TizenClaw host bundle rooted at
# ${TIZENCLAW_INSTALL_ROOT:-${HOME}/.tizenclaw}.
#
# Supported actions:
#   --help                 Show this help
#   --status               Show daemon status
#   --restart-only         Stop + start the installed daemon
#   -s, --stop             Stop the running daemon
#   --log                  Follow daemon log output
#
# Anything that requires a source checkout (building, testing, installing,
# removing the tree) is explicitly rejected with a clear error directing the
# user to ./deploy_host.sh in a repository checkout.

set -euo pipefail

ENTRYPOINT_NAME="$(basename "$0")"

HOST_BASE_DIR="${TIZENCLAW_INSTALL_ROOT:-${HOME}/.tizenclaw}"
INSTALL_DIR="${HOST_BASE_DIR}/bin"
LIB_DIR="${HOST_BASE_DIR}/lib"
CONFIG_DIR="${HOST_BASE_DIR}/config"
LOG_DIR="${HOST_BASE_DIR}/logs"
RUN_DIR="${HOST_BASE_DIR}/run"
TOOLS_DIR="${HOST_BASE_DIR}/tools"

PKG_NAME="tizenclaw"
TOOL_EXECUTOR_NAME="tizenclaw-tool-executor"
CLI_NAME="tizenclaw-cli"
WEB_DASHBOARD_NAME="tizenclaw-web-dashboard"
TEST_TOOL_NAME="tizenclaw-tests"

PID_FILE="${RUN_DIR}/tizenclaw-host.pid"
TOOL_EXECUTOR_PID_FILE="${RUN_DIR}/tizenclaw-tool-executor-host.pid"

HOST_DASHBOARD_PORT_DEFAULT=9091

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

log()    { echo -e "${CYAN}[HOSTCTL]${NC} $*"; }
ok()     { echo -e "${GREEN}[  OK   ]${NC} $*"; }
warn()   { echo -e "${YELLOW}[ WARN  ]${NC} $*"; }
fail()   { echo -e "${RED}[ FAIL  ]${NC} $*" >&2; exit 1; }
header() {
  echo -e "\n${BOLD}══════════════════════════════════════════${NC}"
  echo -e "${BOLD}  $*${NC}"
  echo -e "${BOLD}══════════════════════════════════════════${NC}"
}

usage() {
  # Use printf %b so the ANSI escape sequences stored in BOLD/CYAN/NC are
  # interpreted instead of printed as literal "\033[...m" strings.
  printf '%b\n' "${BOLD}TizenClaw Installed Host Control${NC}"
  printf '\n'
  printf '%b\n' "${CYAN}Usage:${NC}"
  printf '  %s [action]\n' "${ENTRYPOINT_NAME}"
  printf '\n'
  printf '%b\n' "${CYAN}Actions:${NC}"
  printf '      --status          Show installed daemon status\n'
  printf '      --restart-only    Restart the installed daemon\n'
  printf '  -s, --stop            Stop the running daemon\n'
  printf '      --log             Follow daemon log output\n'
  printf '  -h, --help            Show this help\n'
  printf '\n'
  printf '%b\n' "${CYAN}Environment:${NC}"
  printf '  TIZENCLAW_INSTALL_ROOT   Override the install root (default ~/.tizenclaw)\n'
  printf '  TIZENCLAW_SOCKET_PATH    Override the daemon IPC socket path\n'
  printf '\n'
  printf '%b\n' "${CYAN}Notes:${NC}"
  printf '  This interface manages an already-installed TizenClaw bundle. To build,\n'
  printf '  test, or reinstall TizenClaw from source, use ./deploy_host.sh in a\n'
  printf '  repository checkout.\n'
}

reject_source_only() {
  local flag="$1"
  {
    printf "%s: option '%s' requires a source checkout and is not\n" \
      "${ENTRYPOINT_NAME}" "${flag}"
    printf 'available in the installed bundle control interface.\n\n'
    printf 'Use ./deploy_host.sh from a TizenClaw repository checkout for build, test,\n'
    printf 'install, or source-only workflows. The installed bundle interface supports\n'
    printf 'only: --help, --status, --restart-only, --stop, --log.\n'
  } >&2
  exit 64
}

require_installed_tree() {
  [[ -d "${HOST_BASE_DIR}" ]] \
    || fail "Install root not found: ${HOST_BASE_DIR} (set TIZENCLAW_INSTALL_ROOT if the bundle lives elsewhere)"
  [[ -d "${INSTALL_DIR}" ]] \
    || fail "Missing ${INSTALL_DIR} — reinstall the host bundle with install.sh"
}

dashboard_port() {
  local config_path="${CONFIG_DIR}/channel_config.json"
  if [[ ! -f "${config_path}" ]] || ! command -v python3 >/dev/null 2>&1; then
    echo "${HOST_DASHBOARD_PORT_DEFAULT}"
    return
  fi
  python3 - <<'PY' "${config_path}" "${HOST_DASHBOARD_PORT_DEFAULT}"
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
default_port = int(sys.argv[2])
port = default_port
try:
    data = json.loads(path.read_text(encoding="utf-8"))
    for channel in data.get("channels", []):
        if channel.get("name") == "web_dashboard":
            port = int(channel.get("settings", {}).get("port", default_port))
            break
except Exception:
    port = default_port
print(port)
PY
}

port_report() {
  local port="$1"
  ss -ltnp "( sport = :${port} )" 2>/dev/null | sed '1d' || true
}

wait_for_exit() {
  local binary_name="$1"
  local timeout_secs="${2:-5}"
  local waited=0
  local current_uid
  current_uid="$(id -u)"
  local match_pat="${INSTALL_DIR}/${binary_name}([[:space:]]|\$)"

  while pgrep -u "${current_uid}" -f "${match_pat}" >/dev/null 2>&1; do
    if [ "${waited}" -ge "${timeout_secs}" ]; then
      pkill -9 -u "${current_uid}" -f "${match_pat}" >/dev/null 2>&1 || true
      return 1
    fi
    sleep 1
    waited=$((waited + 1))
  done
  return 0
}

force_kill_by_pid() {
  local pid="$1"
  local label="$2"
  if [ -z "${pid}" ]; then
    return 0
  fi
  if kill -0 "${pid}" 2>/dev/null; then
    warn "${label} still running after graceful stop; sending SIGKILL to pid ${pid}"
    kill -9 "${pid}" 2>/dev/null || true
    sleep 1
  fi
}

stop_daemon() {
  header "Stop Installed Host Daemon"

  if [ -f "${TOOL_EXECUTOR_PID_FILE}" ]; then
    local pid
    pid=$(cat "${TOOL_EXECUTOR_PID_FILE}" 2>/dev/null || true)
    if [ -n "${pid}" ] && kill -0 "${pid}" 2>/dev/null; then
      log "Stopping ${TOOL_EXECUTOR_NAME} (pid ${pid})..."
      kill "${pid}" 2>/dev/null || true
      sleep 1
      force_kill_by_pid "${pid}" "${TOOL_EXECUTOR_NAME}"
    fi
    rm -f "${TOOL_EXECUTOR_PID_FILE}"
  fi

  if [ -f "${PID_FILE}" ]; then
    local pid
    pid=$(cat "${PID_FILE}" 2>/dev/null || true)
    if [ -n "${pid}" ] && kill -0 "${pid}" 2>/dev/null; then
      log "Stopping ${PKG_NAME} daemon (pid ${pid})..."
      kill "${pid}" 2>/dev/null || true
      sleep 1
      force_kill_by_pid "${pid}" "${PKG_NAME}"
    fi
    rm -f "${PID_FILE}"
  fi

  local current_uid
  current_uid="$(id -u)"
  for name in "${TOOL_EXECUTOR_NAME}" "${WEB_DASHBOARD_NAME}" "${PKG_NAME}" "${CLI_NAME}"; do
    local pat="${INSTALL_DIR}/${name}([[:space:]]|$)"
    if pgrep -u "${current_uid}" -f "${pat}" >/dev/null 2>&1; then
      pkill -u "${current_uid}" -f "${pat}" >/dev/null 2>&1 || true
    fi
  done

  wait_for_exit "${TOOL_EXECUTOR_NAME}" 5 || true
  wait_for_exit "${PKG_NAME}" 5 || true
  wait_for_exit "${WEB_DASHBOARD_NAME}" 5 || true

  ok "Stop sequence complete"
}

start_daemon() {
  header "Start Installed Host Daemon"

  require_installed_tree
  [[ -x "${INSTALL_DIR}/${PKG_NAME}" ]] \
    || fail "Daemon binary not found or not executable: ${INSTALL_DIR}/${PKG_NAME}"
  [[ -x "${INSTALL_DIR}/${TOOL_EXECUTOR_NAME}" ]] \
    || fail "Tool executor not found or not executable: ${INSTALL_DIR}/${TOOL_EXECUTOR_NAME}"

  mkdir -p "${LOG_DIR}" "${RUN_DIR}"

  export TIZENCLAW_DATA_DIR="${HOST_BASE_DIR}"
  export TIZENCLAW_TOOLS_DIR="${TOOLS_DIR}"
  export PATH="${INSTALL_DIR}:${PATH}"
  export LD_LIBRARY_PATH="${LIB_DIR}${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}"

  log "Starting ${TOOL_EXECUTOR_NAME}..."
  setsid "${INSTALL_DIR}/${TOOL_EXECUTOR_NAME}" \
    >> "${LOG_DIR}/tizenclaw-tool-executor.log" 2>&1 < /dev/null &
  echo $! > "${TOOL_EXECUTOR_PID_FILE}"
  ok "${TOOL_EXECUTOR_NAME} started (pid $(cat "${TOOL_EXECUTOR_PID_FILE}"))"

  log "Starting ${PKG_NAME} daemon..."
  setsid "${INSTALL_DIR}/${PKG_NAME}" \
    >> "${LOG_DIR}/tizenclaw.stdout.log" 2>&1 < /dev/null &
  echo $! > "${PID_FILE}"
  sleep 1
  if kill -0 "$(cat "${PID_FILE}")" 2>/dev/null; then
    ok "${PKG_NAME} daemon started (pid $(cat "${PID_FILE}"))"
  else
    fail "${PKG_NAME} daemon failed to start — inspect ${LOG_DIR}/tizenclaw.stdout.log"
  fi
}

wait_for_ipc_ready() {
  local test_bin="${INSTALL_DIR}/${TEST_TOOL_NAME}"
  if [[ ! -x "${test_bin}" ]]; then
    return 0
  fi
  local deadline=$((SECONDS + 5))
  while [ "${SECONDS}" -lt "${deadline}" ]; do
    if "${test_bin}" call --method ping >/dev/null 2>&1; then
      ok "Daemon IPC is ready"
      return 0
    fi
    sleep 0.2
  done
  warn "Daemon IPC did not respond to ping within 5s"
  return 1
}

show_status() {
  header "Installed Daemon Status"

  require_installed_tree

  local port
  port="$(dashboard_port)"

  if [ -f "${PID_FILE}" ]; then
    local pid
    pid=$(cat "${PID_FILE}" 2>/dev/null || true)
    if [ -n "${pid}" ] && kill -0 "${pid}" 2>/dev/null; then
      ok "${PKG_NAME} is running (pid ${pid})"
    else
      warn "PID file exists but process ${pid} is not running"
    fi
  else
    warn "${PKG_NAME} is not running (no PID file)"
  fi

  if [ -f "${TOOL_EXECUTOR_PID_FILE}" ]; then
    local pid
    pid=$(cat "${TOOL_EXECUTOR_PID_FILE}" 2>/dev/null || true)
    if [ -n "${pid}" ] && kill -0 "${pid}" 2>/dev/null; then
      ok "${TOOL_EXECUTOR_NAME} is running (pid ${pid})"
    else
      warn "tool-executor PID file exists but process is not running"
    fi
  else
    warn "${TOOL_EXECUTOR_NAME} is not running (no PID file)"
  fi

  if pgrep -u "$(id -u)" -f "${INSTALL_DIR}/${WEB_DASHBOARD_NAME}([[:space:]]|$)" >/dev/null 2>&1; then
    ok "${WEB_DASHBOARD_NAME} is running"
  else
    warn "${WEB_DASHBOARD_NAME} is not running"
  fi

  local listeners
  listeners="$(port_report "${port}")"
  if [ -n "${listeners}" ]; then
    log "Port ${port} listeners:"
    printf '%s\n' "${listeners}"
  else
    log "Port ${port} has no active listeners"
  fi

  if [ -f "${LOG_DIR}/tizenclaw.log" ]; then
    echo ""
    log "Recent logs (last 20 lines):"
    tail -20 "${LOG_DIR}/tizenclaw.log" 2>/dev/null || true
  fi
}

follow_log() {
  local log_file="${LOG_DIR}/tizenclaw.log"
  [[ -f "${log_file}" ]] || fail "Log file not found: ${log_file}"
  log "Following log: ${log_file} (Ctrl+C to stop)"
  tail -f "${log_file}"
}

main() {
  if [[ $# -eq 0 ]]; then
    usage
    exit 0
  fi

  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    --status)
      [[ $# -eq 1 ]] || fail "--status takes no additional arguments"
      show_status
      ;;
    --log)
      [[ $# -eq 1 ]] || fail "--log takes no additional arguments"
      follow_log
      ;;
    -s|--stop)
      [[ $# -eq 1 ]] || fail "$1 takes no additional arguments"
      require_installed_tree
      stop_daemon
      ;;
    --restart-only)
      [[ $# -eq 1 ]] || fail "--restart-only takes no additional arguments"
      stop_daemon
      start_daemon
      wait_for_ipc_ready || true
      ;;
    --release|--debug|-d|--build-only|-b|--no-restart|--test|--remove\
      |--dry-run|--devel|--build-root|--llm-config)
      reject_source_only "$1"
      ;;
    *)
      fail "Unknown option: $1 (use --help)"
      ;;
  esac
}

main "$@"
