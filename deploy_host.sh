#!/bin/bash
# TizenClaw Host Linux Build & Run Script
# Builds and runs TizenClaw natively on the host Linux (Ubuntu/WSL)
# without Tizen GBS — uses `cargo build` with vendored sources.
#
# Usage:
#   ./deploy_host.sh                   # Build (release) + install + run
#   ./deploy_host.sh -d, --debug       # Build in debug mode
#   ./deploy_host.sh -b, --build-only  # Build only, do not run
#   ./deploy_host.sh -s, --stop        # Stop running daemon
#   ./deploy_host.sh --status          # Show daemon status
#   ./deploy_host.sh --log             # Follow daemon logs
#   ./deploy_host.sh --dry-run         # Print commands without executing
#   ./deploy_host.sh --test            # Build + run cargo tests
#   ./deploy_host.sh -h, --help        # Show this help

set -euo pipefail

# ─────────────────────────────────────────────
# Constants
# ─────────────────────────────────────────────
PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
PKG_NAME="tizenclaw"
TOOL_EXECUTOR_NAME="tizenclaw-tool-executor"
CLI_NAME="tizenclaw-cli"
WEB_DASHBOARD_NAME="tizenclaw-web-dashboard"

HOST_BASE_DIR="${HOME}/.tizenclaw"
INSTALL_DIR="${HOST_BASE_DIR}/bin"
DATA_DIR="${HOST_BASE_DIR}"
BUILD_ROOT_DIR="${HOST_BASE_DIR}/build"
CARGO_TARGET_DIR_DEFAULT="${BUILD_ROOT_DIR}/cargo-target"
TOOLS_DIR="${DATA_DIR}/tools"
WORKSPACE_DIR="${DATA_DIR}/workspace"
LOG_DIR="${DATA_DIR}/logs"
CONFIG_DIR="${DATA_DIR}/config"
LEGACY_HOST_BASE_DIR="${HOME}/.local/share/tizenclaw"
LEGACY_HOST_BIN_DIR="${HOME}/.local/bin"
DOCS_SRC="${PROJECT_DIR}/data/docs"
EMBEDDED_TOOLS_SRC="${PROJECT_DIR}/tools/embedded"
WEB_SRC="${PROJECT_DIR}/data/web"
BUNDLED_CONFIG_DIR="${PROJECT_DIR}/data/config"
BASHRC_PATH="${HOME}/.bashrc"
PATH_EXPORT='export PATH="$HOME/.tizenclaw/bin:$PATH"'

PID_FILE="/tmp/tizenclaw-host.pid"
TOOL_EXECUTOR_PID_FILE="/tmp/tizenclaw-tool-executor-host.pid"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# ─────────────────────────────────────────────
# Defaults
# ─────────────────────────────────────────────
BUILD_MODE="release"
BUILD_ONLY=false
STOP_DAEMON=false
SHOW_STATUS=false
FOLLOW_LOG=false
DRY_RUN=false
RUN_TESTS=false
REMOVE_INSTALL=false
LLM_CONFIG=""
CARGO_TARGET_DIR_HOST="${CARGO_TARGET_DIR:-${CARGO_TARGET_DIR_DEFAULT}}"

# ─────────────────────────────────────────────
# Logging helpers
# ─────────────────────────────────────────────
log()    { echo -e "${CYAN}[HOST]${NC} $*"; }
ok()     { echo -e "${GREEN}[  OK  ]${NC} $*"; }
warn()   { echo -e "${YELLOW}[ WARN ]${NC} $*"; }
fail()   { echo -e "${RED}[ FAIL ]${NC} $*"; exit 1; }
header() {
  echo -e "\n${BOLD}══════════════════════════════════════════${NC}"
  echo -e "${BOLD}  $*${NC}"
  echo -e "${BOLD}══════════════════════════════════════════${NC}"
}

run() {
  if [ "${DRY_RUN}" = true ]; then
    echo -e "  ${YELLOW}[DRY-RUN]${NC} $*"
    return 0
  fi
  "$@"
}

process_report() {
  ps -eo pid,ppid,stat,cmd \
    | grep -E "(${INSTALL_DIR}/${PKG_NAME}|${INSTALL_DIR}/${TOOL_EXECUTOR_NAME}|${INSTALL_DIR}/${WEB_DASHBOARD_NAME}|(^|/| )${PKG_NAME}($| )|(^|/| )${TOOL_EXECUTOR_NAME}($| )|(^|/| )${WEB_DASHBOARD_NAME}($| ))" \
    | grep -v -E "grep -E|deploy_host.sh" || true
}

dashboard_port() {
  python3 - <<'PY' "${CONFIG_DIR}/channel_config.json"
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
port = 9090
try:
    if path.exists():
        data = json.loads(path.read_text(encoding="utf-8"))
        for channel in data.get("channels", []):
            if channel.get("name") == "web_dashboard":
                port = int(channel.get("settings", {}).get("port", 9090))
                break
except Exception:
    port = 9090
print(port)
PY
}

port_report() {
  local port="$1"
  ss -ltnp "( sport = :${port} )" 2>/dev/null | sed '1d' || true
}

warn_if_dashboard_port_busy() {
  local port="$1"
  local listeners
  listeners="$(port_report "${port}")"
  if [ -n "${listeners}" ]; then
    warn "Dashboard port ${port} is already in use before startup:"
    printf '%s\n' "${listeners}"
    warn "The dashboard may exit immediately until the port is freed or reconfigured."
    return 0
  fi
  ok "Dashboard port ${port} is available"
}

wait_for_process_name_exit() {
  local label="$1"
  local binary_name="$2"
  local timeout_secs="${3:-5}"
  local waited=0
  local current_uid
  current_uid="$(id -u)"

  while pgrep -u "${current_uid}" -x "${binary_name}" >/dev/null 2>&1 \
    || pgrep -u "${current_uid}" -f "${INSTALL_DIR}/${binary_name}([[:space:]]|$)" >/dev/null 2>&1; do
    if [ "${waited}" -ge "${timeout_secs}" ]; then
      warn "${label} still appears to be alive after ${timeout_secs}s"
      return 1
    fi
    sleep 1
    waited=$((waited + 1))
  done

  return 0
}

# ─────────────────────────────────────────────
# Usage
# ─────────────────────────────────────────────
usage() {
  cat <<EOF
${BOLD}TizenClaw Host Linux Build & Run${NC}

${CYAN}Usage:${NC}
  $(basename "$0") [options]

${CYAN}Options:${NC}
  -d, --debug             Build in debug mode (default: release)
  -b, --build-only        Build only, do not install or run
      --test              Build + run cargo tests (offline, vendored)
  -s, --stop              Stop the running host daemon
      --remove            Stop host processes and remove ~/.tizenclaw install
      --status            Show current daemon status
      --log               Follow daemon log output
      --dry-run           Print commands without executing
      --build-root <dir>  Override host Cargo target dir
      --llm-config <path> Use specified llm_config.json (sets TIZENCLAW_DATA_DIR)
  -h, --help              Show this help

${CYAN}Examples:${NC}
  $(basename "$0")                           # Release build + install + run
  $(basename "$0") -d                        # Debug build + install + run
  $(basename "$0") -b                        # Build only
  $(basename "$0") --test                    # Run unit/integration tests
  $(basename "$0") --status                  # Check daemon status
  $(basename "$0") --log                     # Tail daemon logs
  $(basename "$0") -s                        # Stop the daemon
  $(basename "$0") --remove                  # Remove host install and stop tools
  $(basename "$0") --build-root /tmp/tc-build  # Use external build root
  $(basename "$0") --llm-config /path/to/llm_config.json  # Use custom LLM config
EOF
  exit 0
}

# ─────────────────────────────────────────────
# Argument parsing
# ─────────────────────────────────────────────
parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      -d|--debug)       BUILD_MODE="debug"; shift ;;
      -b|--build-only)  BUILD_ONLY=true; shift ;;
      --test)           RUN_TESTS=true; shift ;;
      -s|--stop)        STOP_DAEMON=true; shift ;;
      --remove)         REMOVE_INSTALL=true; shift ;;
      --status)         SHOW_STATUS=true; shift ;;
      --log)            FOLLOW_LOG=true; shift ;;
      --dry-run)        DRY_RUN=true; shift ;;
      --build-root)
        [[ $# -lt 2 ]] && fail "--build-root requires a path argument"
        CARGO_TARGET_DIR_HOST="$(realpath -m "$2")"; shift 2 ;;
      --llm-config)
        [[ $# -lt 2 ]] && fail "--llm-config requires a path argument"
        LLM_CONFIG="$(realpath "$2")"; shift 2 ;;
      -h|--help)        usage ;;
      *) fail "Unknown option: $1 (use --help)" ;;
    esac
  done
}

# ─────────────────────────────────────────────
# Pre-flight checks
# ─────────────────────────────────────────────
check_prerequisites() {
  header "Pre-flight Checks"

  if ! command -v cargo &>/dev/null; then
    fail "cargo not found. Install Rust: https://rustup.rs"
  fi
  ok "cargo found: $(cargo --version)"

  local rust_ver
  rust_ver=$(rustc --version 2>/dev/null || echo "unknown")
  ok "rustc: ${rust_ver}"

  log "Build mode  : ${BUILD_MODE}"
  log "Project dir : ${PROJECT_DIR}"
  log "Build only  : ${BUILD_ONLY}"
  log "Data dir    : ${DATA_DIR}"
  log "Build root  : ${CARGO_TARGET_DIR_HOST}"
}

ensure_shell_path() {
  header "PATH Bootstrap"

  if [ ! -f "${BASHRC_PATH}" ]; then
    run touch "${BASHRC_PATH}"
  fi

  if grep -Fqx "${PATH_EXPORT}" "${BASHRC_PATH}" 2>/dev/null; then
    ok "~/.bashrc already contains host PATH export"
  else
    log "Appending host PATH export to ${BASHRC_PATH}"
    if [ "${DRY_RUN}" = true ]; then
      echo -e "  ${YELLOW}[DRY-RUN]${NC} printf '\\n%s\\n' '${PATH_EXPORT}' >> '${BASHRC_PATH}'"
    else
      printf '\n%s\n' "${PATH_EXPORT}" >> "${BASHRC_PATH}"
    fi
    ok "Added PATH export to ~/.bashrc"
  fi

  log "Sourcing ~/.bashrc for the current script shell"
  if [ "${DRY_RUN}" = false ]; then
    # shellcheck disable=SC1090
    source "${BASHRC_PATH}" || true
  else
    echo -e "  ${YELLOW}[DRY-RUN]${NC} source '${BASHRC_PATH}'"
  fi
}

migrate_legacy_host_install() {
  header "Legacy Host Migration"

  if [ -d "${LEGACY_HOST_BASE_DIR}" ] && [ ! -d "${HOST_BASE_DIR}" ]; then
    log "Migrating legacy host data ${LEGACY_HOST_BASE_DIR} → ${HOST_BASE_DIR}"
    run mkdir -p "${HOST_BASE_DIR}"
    run cp -a "${LEGACY_HOST_BASE_DIR}/." "${HOST_BASE_DIR}/"
    ok "Legacy host data migrated"
  else
    ok "No legacy host data migration needed"
  fi
}

cleanup_legacy_host_install() {
  header "Legacy Host Cleanup"

  if [ -d "${LEGACY_HOST_BASE_DIR}" ]; then
    log "Removing legacy host data tree ${LEGACY_HOST_BASE_DIR}"
    run rm -rf "${LEGACY_HOST_BASE_DIR}"
    ok "Removed legacy host data tree"
  else
    ok "No legacy host data tree found"
  fi

  for legacy_bin in "${PKG_NAME}" "${TOOL_EXECUTOR_NAME}" "${CLI_NAME}" "${WEB_DASHBOARD_NAME}"; do
    if [ -f "${LEGACY_HOST_BIN_DIR}/${legacy_bin}" ]; then
      log "Removing legacy binary ${LEGACY_HOST_BIN_DIR}/${legacy_bin}"
      run rm -f "${LEGACY_HOST_BIN_DIR}/${legacy_bin}"
    fi
  done
}

# ─────────────────────────────────────────────
# Step 1: Build
# ─────────────────────────────────────────────
do_build() {
  header "Step 1/3: Cargo Build (Host — Generic Linux)"

  local cargo_args=("build" "--offline")
  if [ "${BUILD_MODE}" = "release" ]; then
    cargo_args+=("--release")
  fi

  # Build daemon + tool-executor + CLI + web-dashboard
  cargo_args+=(
    "-p" "${PKG_NAME}"
    "-p" "${TOOL_EXECUTOR_NAME}"
    "-p" "${CLI_NAME}"
    "-p" "${WEB_DASHBOARD_NAME}"
  )

  log "Running: cargo ${cargo_args[*]}"
  cd "${PROJECT_DIR}"

  if [ "${DRY_RUN}" = true ]; then
    echo -e "  ${YELLOW}[DRY-RUN]${NC} export CARGO_TARGET_DIR='${CARGO_TARGET_DIR_HOST}'"
    echo -e "  ${YELLOW}[DRY-RUN]${NC} cargo ${cargo_args[*]}"
    ok "Build succeeded (dry-run)"
    return 0
  fi

  mkdir -p "${CARGO_TARGET_DIR_HOST}"

  if CARGO_TARGET_DIR="${CARGO_TARGET_DIR_HOST}" cargo "${cargo_args[@]}"; then
    ok "Cargo build succeeded (${BUILD_MODE})"
  else
    fail "Cargo build failed"
  fi
}

# ─────────────────────────────────────────────
# Step 1 (alt): Run tests
# ─────────────────────────────────────────────
do_test() {
  header "Running Tests (Host — Generic Linux)"

  log "Stopping running host processes before test cycle"
  stop_daemon
  if [ "${DRY_RUN}" = false ]; then
    process_report || true
  fi

  log "Running: cargo test --offline"
  cd "${PROJECT_DIR}"

  if [ "${DRY_RUN}" = true ]; then
    echo -e "  ${YELLOW}[DRY-RUN]${NC} export CARGO_TARGET_DIR='${CARGO_TARGET_DIR_HOST}'"
    echo -e "  ${YELLOW}[DRY-RUN]${NC} cargo test --offline"
    return 0
  fi

  mkdir -p "${CARGO_TARGET_DIR_HOST}"

  if CARGO_TARGET_DIR="${CARGO_TARGET_DIR_HOST}" cargo test --offline -- --test-threads=1 2>&1; then
    ok "All tests passed"
  else
    warn "Some tests failed (see output above)"
  fi
}

# ─────────────────────────────────────────────
# Step 2: Install binaries and data
# ─────────────────────────────────────────────
do_install() {
  header "Step 2/3: Install Binaries"

  local build_dir="${CARGO_TARGET_DIR_HOST}/${BUILD_MODE}"

  migrate_legacy_host_install

  log "Preparing host install tree under ${DATA_DIR}"
  run mkdir -p "${INSTALL_DIR}" "${CONFIG_DIR}" "${TOOLS_DIR}/cli" \
    "${WORKSPACE_DIR}/skills" "${TOOLS_DIR}" "${DATA_DIR}/embedded" "${DATA_DIR}/web" \
    "${DATA_DIR}/workflows" "${DATA_DIR}/pipelines" "${DATA_DIR}/codes" \
    "${DATA_DIR}/memory" "${DATA_DIR}/plugins" "${LOG_DIR}"

  if [ -d "${TOOLS_DIR}/skills" ] && [ ! -e "${WORKSPACE_DIR}/skills" ]; then
    log "Migrating legacy skills dir → ${WORKSPACE_DIR}/skills"
    run mv "${TOOLS_DIR}/skills" "${WORKSPACE_DIR}/skills"
  fi
  if [ "${DRY_RUN}" = false ]; then
    run mkdir -p "${WORKSPACE_DIR}/skills"
    if [ -L "${TOOLS_DIR}/skills" ] || [ -d "${TOOLS_DIR}/skills" ] || [ -f "${TOOLS_DIR}/skills" ]; then
      run rm -rf "${TOOLS_DIR}/skills"
    fi
    run ln -s "${WORKSPACE_DIR}/skills" "${TOOLS_DIR}/skills"
  else
    echo -e "  ${YELLOW}[DRY-RUN]${NC} mkdir -p '${WORKSPACE_DIR}/skills'"
    echo -e "  ${YELLOW}[DRY-RUN]${NC} ln -s '${WORKSPACE_DIR}/skills' '${TOOLS_DIR}/skills'"
  fi

  for bin in "${PKG_NAME}" "${TOOL_EXECUTOR_NAME}" "${CLI_NAME}" "${WEB_DASHBOARD_NAME}"; do
    local bin_path="${build_dir}/${bin}"
    if [ "${DRY_RUN}" = false ] && [ ! -f "${bin_path}" ]; then
      fail "Binary not found: ${bin_path}"
    fi
    log "Installing ${bin} → ${INSTALL_DIR}/${bin}"
    run install -m 755 "${bin_path}" "${INSTALL_DIR}/${bin}"
    ok "Installed: ${bin}"
  done

  # Deploy web dashboard
  if [ -d "${WEB_SRC}" ]; then
    log "Installing web dashboard → ${DATA_DIR}/web"
    run cp -r "${WEB_SRC}/." "${DATA_DIR}/web/"
    ok "Web dashboard installed"
  fi

  if [ -d "${DOCS_SRC}" ]; then
    log "Installing docs → ${DATA_DIR}/docs"
    run mkdir -p "${DATA_DIR}/docs"
    run cp -r "${DOCS_SRC}/." "${DATA_DIR}/docs/"
    ok "Docs installed"
  fi

  if [ -d "${BUNDLED_CONFIG_DIR}" ]; then
    log "Seeding default config files into ${CONFIG_DIR} when missing"
    while IFS= read -r config_path; do
      local file_name
      file_name="$(basename "${config_path}")"
      local target_path="${CONFIG_DIR}/${file_name}"
      if [ ! -f "${target_path}" ]; then
        run install -m 644 "${config_path}" "${target_path}"
      fi
    done < <(find "${BUNDLED_CONFIG_DIR}" -maxdepth 1 -type f ! -name '*.sample' | sort)
    ok "Default config seeding complete"
  fi

  if [ -d "${EMBEDDED_TOOLS_SRC}" ]; then
    log "Installing embedded tool descriptors → ${DATA_DIR}/embedded"
    run cp -r "${EMBEDDED_TOOLS_SRC}/." "${DATA_DIR}/embedded/"
    ok "Embedded tool descriptors installed"
  fi

  ensure_shell_path
  cleanup_legacy_host_install
}

# ─────────────────────────────────────────────
# Step 3: Run daemon
# ─────────────────────────────────────────────
stop_daemon() {
  force_kill_by_pid() {
    local pid="$1"
    local label="$2"
    if [ -z "${pid}" ]; then
      return 0
    fi
    if kill -0 "${pid}" 2>/dev/null; then
      warn "${label} still running after graceful stop; sending SIGKILL to pid ${pid}"
      run kill -9 "${pid}" || true
      sleep 1
    fi
  }

  if [ -f "${TOOL_EXECUTOR_PID_FILE}" ]; then
    local pid
    pid=$(cat "${TOOL_EXECUTOR_PID_FILE}" 2>/dev/null || true)
    if [ -n "${pid}" ] && kill -0 "${pid}" 2>/dev/null; then
      log "Stopping tizenclaw-tool-executor (pid ${pid})..."
      run kill "${pid}" || true
      sleep 1
      force_kill_by_pid "${pid}" "tizenclaw-tool-executor"
    fi
    rm -f "${TOOL_EXECUTOR_PID_FILE}"
  fi

  if [ -f "${PID_FILE}" ]; then
    local pid
    pid=$(cat "${PID_FILE}" 2>/dev/null || true)
    if [ -n "${pid}" ] && kill -0 "${pid}" 2>/dev/null; then
      log "Stopping tizenclaw daemon (pid ${pid})..."
      run kill "${pid}" || true
      sleep 1
      force_kill_by_pid "${pid}" "tizenclaw"
    fi
    rm -f "${PID_FILE}"
    ok "Daemon stopped"
  else
    warn "No PID file found at ${PID_FILE}. Daemon may not be running."
    # Try by name as fallback
    if pgrep -x "${PKG_NAME}" &>/dev/null; then
      run pkill -x "${PKG_NAME}" || true
      ok "Daemon killed by name"
    fi
  fi

  if pgrep -f "${INSTALL_DIR}/${TOOL_EXECUTOR_NAME}" &>/dev/null; then
    run pkill -f "${INSTALL_DIR}/${TOOL_EXECUTOR_NAME}" || true
  fi
  if pgrep -f "${INSTALL_DIR}/${PKG_NAME}" &>/dev/null; then
    run pkill -f "${INSTALL_DIR}/${PKG_NAME}" || true
  fi
  if pgrep -f "${INSTALL_DIR}/${CLI_NAME}" &>/dev/null; then
    run pkill -f "${INSTALL_DIR}/${CLI_NAME}" || true
  fi
  if pgrep -f "${INSTALL_DIR}/${WEB_DASHBOARD_NAME}" &>/dev/null; then
    run pkill -f "${INSTALL_DIR}/${WEB_DASHBOARD_NAME}" || true
  fi

  if pgrep -x "${TOOL_EXECUTOR_NAME}" &>/dev/null; then
    run pkill -x "${TOOL_EXECUTOR_NAME}" || true
  fi
  if pgrep -x "${CLI_NAME}" &>/dev/null; then
    run pkill -x "${CLI_NAME}" || true
  fi
  if pgrep -x "${WEB_DASHBOARD_NAME}" &>/dev/null; then
    run pkill -x "${WEB_DASHBOARD_NAME}" || true
  fi

  wait_for_process_name_exit "tizenclaw-tool-executor" "${TOOL_EXECUTOR_NAME}" 5 || true
  wait_for_process_name_exit "tizenclaw" "${PKG_NAME}" 5 || true
  wait_for_process_name_exit "tizenclaw-web-dashboard" "${WEB_DASHBOARD_NAME}" 5 || true

  if [ "${DRY_RUN}" = false ]; then
    local remaining
    remaining="$(process_report)"
    if [ -n "${remaining}" ]; then
      warn "Remaining host process entries detected after stop:"
      printf '%s\n' "${remaining}"
    else
      ok "All known host processes were stopped"
    fi
  fi
}

remove_installation() {
  header "Remove Host Installation"

  stop_daemon

  if [ -d "${DATA_DIR}" ]; then
    log "Removing ${DATA_DIR}"
    run rm -rf "${DATA_DIR}"
    ok "Removed host data tree"
  else
    warn "Host data tree not found: ${DATA_DIR}"
  fi

  if [ -d "${LEGACY_HOST_BASE_DIR}" ]; then
    log "Removing legacy host data tree ${LEGACY_HOST_BASE_DIR}"
    run rm -rf "${LEGACY_HOST_BASE_DIR}"
    ok "Removed legacy host data tree"
  fi

  if [ -d "${BUILD_ROOT_DIR}" ]; then
    log "Removing host build tree ${BUILD_ROOT_DIR}"
    run rm -rf "${BUILD_ROOT_DIR}"
    ok "Removed host build tree"
  fi

  for legacy_bin in "${PKG_NAME}" "${TOOL_EXECUTOR_NAME}" "${CLI_NAME}" "${WEB_DASHBOARD_NAME}"; do
    if [ -f "${LEGACY_HOST_BIN_DIR}/${legacy_bin}" ]; then
      log "Removing legacy binary ${LEGACY_HOST_BIN_DIR}/${legacy_bin}"
      run rm -f "${LEGACY_HOST_BIN_DIR}/${legacy_bin}"
    fi
  done

  if [ -f "${BASHRC_PATH}" ] && grep -Fqx "${PATH_EXPORT}" "${BASHRC_PATH}" 2>/dev/null; then
    log "Removing PATH export from ${BASHRC_PATH}"
    if [ "${DRY_RUN}" = false ]; then
      grep -Fvx "${PATH_EXPORT}" "${BASHRC_PATH}" > "${BASHRC_PATH}.tmp" || true
      mv "${BASHRC_PATH}.tmp" "${BASHRC_PATH}"
      # shellcheck disable=SC1090
      source "${BASHRC_PATH}" || true
    else
      echo -e "  ${YELLOW}[DRY-RUN]${NC} remove '${PATH_EXPORT}' from '${BASHRC_PATH}'"
    fi
    ok "Removed PATH export from ~/.bashrc"
  fi
}

show_status() {
  header "Daemon Status"
  local host_dashboard_port
  host_dashboard_port="$(dashboard_port)"

  if [ -f "${PID_FILE}" ]; then
    local pid
    pid=$(cat "${PID_FILE}" 2>/dev/null || true)
    if [ -n "${pid}" ] && kill -0 "${pid}" 2>/dev/null; then
      ok "tizenclaw is running (pid ${pid})"
    else
      warn "PID file exists but process ${pid} is not running"
    fi
  else
    warn "tizenclaw is not running (no PID file)"
  fi

  if [ -f "${TOOL_EXECUTOR_PID_FILE}" ]; then
    local pid
    pid=$(cat "${TOOL_EXECUTOR_PID_FILE}" 2>/dev/null || true)
    if [ -n "${pid}" ] && kill -0 "${pid}" 2>/dev/null; then
      ok "tizenclaw-tool-executor is running (pid ${pid})"
    else
      warn "tool-executor PID file exists but process is not running"
    fi
  fi

  if pgrep -f "${INSTALL_DIR}/${WEB_DASHBOARD_NAME}" >/dev/null 2>&1 || pgrep -x "${WEB_DASHBOARD_NAME}" >/dev/null 2>&1; then
    ok "tizenclaw-web-dashboard is running"
  else
    warn "tizenclaw-web-dashboard is not running"
  fi

  local dashboard_listeners
  dashboard_listeners="$(port_report "${host_dashboard_port}")"
  if [ -n "${dashboard_listeners}" ]; then
    log "Port ${host_dashboard_port} listeners:"
    printf '%s\n' "${dashboard_listeners}"
  else
    log "Port ${host_dashboard_port} has no active listeners"
  fi

  local dashboard_zombies
  dashboard_zombies="$(ps -eo pid,ppid,stat,cmd | grep '\[tizenclaw-web-d\] <defunct>' | grep -v grep || true)"
  if [ -n "${dashboard_zombies}" ]; then
    warn "Detected defunct dashboard process entries:"
    printf '%s\n' "${dashboard_zombies}"
  fi

  if [ -f "${LOG_DIR}/tizenclaw.log" ]; then
    echo ""
    log "Recent logs (last 20 lines):"
    tail -20 "${LOG_DIR}/tizenclaw.log" 2>/dev/null || true
  fi
}

follow_log() {
  local log_file="${LOG_DIR}/tizenclaw.log"
  if [ ! -f "${log_file}" ]; then
    fail "Log file not found: ${log_file}"
  fi
  log "Following log: ${log_file} (Ctrl+C to stop)"
  tail -f "${log_file}"
}

do_run() {
  header "Step 3/3: Start Host Daemon"
  local host_dashboard_port
  host_dashboard_port="$(dashboard_port)"

  # If a custom llm_config.json was specified, wire it up via TIZENCLAW_DATA_DIR
  if [ -n "${LLM_CONFIG}" ]; then
    if [ ! -f "${LLM_CONFIG}" ]; then
      fail "llm_config.json not found: ${LLM_CONFIG}"
    fi
    log "Linking custom LLM config → ${CONFIG_DIR}/llm_config.json"
    mkdir -p "${CONFIG_DIR}"
    ln -sf "${LLM_CONFIG}" "${CONFIG_DIR}/llm_config.json"
  fi
  export TIZENCLAW_DATA_DIR="${DATA_DIR}"
  export TIZENCLAW_TOOLS_DIR="${TOOLS_DIR}"
  export PATH="${INSTALL_DIR}:${PATH}"

  # Stop existing instance if running
  stop_daemon
  if [ "${DRY_RUN}" = false ]; then
    process_report || true
  fi
  warn_if_dashboard_port_busy "${host_dashboard_port}"

  # Start tool-executor in background
  log "Starting tizenclaw-tool-executor..."
  if [ "${DRY_RUN}" = false ]; then
    setsid "${INSTALL_DIR}/${TOOL_EXECUTOR_NAME}" \
      >> "${LOG_DIR}/tizenclaw-tool-executor.log" 2>&1 < /dev/null &
    echo $! > "${TOOL_EXECUTOR_PID_FILE}"
    ok "tizenclaw-tool-executor started (pid $(cat "${TOOL_EXECUTOR_PID_FILE}"))"
  else
    echo -e "  ${YELLOW}[DRY-RUN]${NC} ${INSTALL_DIR}/${TOOL_EXECUTOR_NAME} &"
  fi

  # Start main daemon in background
  log "Starting tizenclaw daemon..."
  if [ "${DRY_RUN}" = false ]; then
    setsid "${INSTALL_DIR}/${PKG_NAME}" \
      >> "${LOG_DIR}/tizenclaw.stdout.log" 2>&1 < /dev/null &
    echo $! > "${PID_FILE}"
    sleep 1
    if kill -0 "$(cat "${PID_FILE}")" 2>/dev/null; then
      ok "tizenclaw daemon started (pid $(cat "${PID_FILE}"))"
    else
      fail "tizenclaw daemon failed to start — check logs: ${LOG_DIR}/tizenclaw.log"
    fi
  else
    echo -e "  ${YELLOW}[DRY-RUN]${NC} ${INSTALL_DIR}/${PKG_NAME} &"
  fi
}

# ─────────────────────────────────────────────
# Summary
# ─────────────────────────────────────────────
show_summary() {
  echo ""
  header "Host Deploy Complete!"
  ok "TizenClaw is running on host Linux (Generic Linux mode)."
  echo ""
  log "Useful commands:"
  log "  Logs (follow)  : ./deploy_host.sh --log"
  log "  Status         : ./deploy_host.sh --status"
  log "  Stop           : ./deploy_host.sh --stop"
  log "  Remove         : ./deploy_host.sh --remove"
  log "  CLI test       : tizenclaw-cli 'hello'"
  echo ""
}

# ─────────────────────────────────────────────
# Main
# ─────────────────────────────────────────────
main() {
  parse_args "$@"

  # Simple actions that don't need a build
  if [ "${STOP_DAEMON}" = true ]; then
    stop_daemon
    exit 0
  fi

  if [ "${REMOVE_INSTALL}" = true ]; then
    remove_installation
    exit 0
  fi

  if [ "${SHOW_STATUS}" = true ]; then
    show_status
    exit 0
  fi

  if [ "${FOLLOW_LOG}" = true ]; then
    follow_log
    exit 0
  fi

  # Test mode
  if [ "${RUN_TESTS}" = true ]; then
    check_prerequisites
    do_test
    exit 0
  fi

  # Standard build (+ optional run)
  check_prerequisites
  do_build

  if [ "${BUILD_ONLY}" = true ]; then
    ok "Build complete. Binaries in: ${CARGO_TARGET_DIR_HOST}/${BUILD_MODE}/"
    exit 0
  fi

  do_install
  do_run
  show_summary
}

main "$@"
