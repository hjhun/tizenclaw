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

INSTALL_DIR="/usr/local/bin"
DATA_DIR="/opt/usr/data/tizenclaw"
LOG_DIR="/opt/usr/share/tizenclaw/logs"
SKILL_SRC="${PROJECT_DIR}/data/skills"
WEB_SRC="${PROJECT_DIR}/data/web"

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
LLM_CONFIG=""

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
      --status            Show current daemon status
      --log               Follow daemon log output
      --dry-run           Print commands without executing
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
      --status)         SHOW_STATUS=true; shift ;;
      --log)            FOLLOW_LOG=true; shift ;;
      --dry-run)        DRY_RUN=true; shift ;;
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
    echo -e "  ${YELLOW}[DRY-RUN]${NC} cargo ${cargo_args[*]}"
    ok "Build succeeded (dry-run)"
    return 0
  fi

  if cargo "${cargo_args[@]}"; then
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

  log "Running: cargo test --offline"
  cd "${PROJECT_DIR}"

  if [ "${DRY_RUN}" = true ]; then
    echo -e "  ${YELLOW}[DRY-RUN]${NC} cargo test --offline"
    return 0
  fi

  if cargo test --offline -- --test-threads=1 2>&1; then
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

  local build_dir="${PROJECT_DIR}/target/${BUILD_MODE}"

  for bin in "${PKG_NAME}" "${TOOL_EXECUTOR_NAME}" "${CLI_NAME}" "${WEB_DASHBOARD_NAME}"; do
    local bin_path="${build_dir}/${bin}"
    if [ ! -f "${bin_path}" ]; then
      fail "Binary not found: ${bin_path}"
    fi
    log "Installing ${bin} → ${INSTALL_DIR}/${bin}"
    run sudo install -m 755 "${bin_path}" "${INSTALL_DIR}/${bin}"
    ok "Installed: ${bin}"
  done

  # Create data and log directories
  log "Creating data directories..."
  run sudo mkdir -p "${DATA_DIR}" "${LOG_DIR}"
  run sudo chmod 755 "${DATA_DIR}" "${LOG_DIR}"
  ok "Directories ready"

  # Deploy skills data
  if [ -d "${SKILL_SRC}" ]; then
    log "Installing skills data → ${DATA_DIR}/skills"
    run sudo mkdir -p "${DATA_DIR}/skills"
    run sudo cp -r "${SKILL_SRC}/." "${DATA_DIR}/skills/"
    ok "Skills data installed"
  else
    warn "Skills source not found: ${SKILL_SRC}"
  fi

  # Deploy web dashboard
  if [ -d "${WEB_SRC}" ]; then
    log "Installing web dashboard → ${DATA_DIR}/web"
    run sudo mkdir -p "${DATA_DIR}/web"
    run sudo cp -r "${WEB_SRC}/." "${DATA_DIR}/web/"
    ok "Web dashboard installed"
  fi
}

# ─────────────────────────────────────────────
# Step 3: Run daemon
# ─────────────────────────────────────────────
stop_daemon() {
  if [ -f "${TOOL_EXECUTOR_PID_FILE}" ]; then
    local pid
    pid=$(cat "${TOOL_EXECUTOR_PID_FILE}" 2>/dev/null || true)
    if [ -n "${pid}" ] && kill -0 "${pid}" 2>/dev/null; then
      log "Stopping tizenclaw-tool-executor (pid ${pid})..."
      run kill "${pid}" || true
      sleep 1
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
}

show_status() {
  header "Daemon Status"

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

  # If a custom llm_config.json was specified, wire it up via TIZENCLAW_DATA_DIR
  if [ -n "${LLM_CONFIG}" ]; then
    if [ ! -f "${LLM_CONFIG}" ]; then
      fail "llm_config.json not found: ${LLM_CONFIG}"
    fi
    local tmp_data_dir="/tmp/tizenclaw-host-data"
    local tmp_cfg_dir="${tmp_data_dir}/config"
    log "Setting up custom LLM config → ${tmp_cfg_dir}/llm_config.json"
    mkdir -p "${tmp_cfg_dir}"
    ln -sf "${LLM_CONFIG}" "${tmp_cfg_dir}/llm_config.json"
    export TIZENCLAW_DATA_DIR="${tmp_data_dir}"
    ok "TIZENCLAW_DATA_DIR=${tmp_data_dir}"
  fi

  # Stop existing instance if running
  stop_daemon

  # Start tool-executor in background
  log "Starting tizenclaw-tool-executor..."
  if [ "${DRY_RUN}" = false ]; then
    "${INSTALL_DIR}/${TOOL_EXECUTOR_NAME}" &
    echo $! > "${TOOL_EXECUTOR_PID_FILE}"
    ok "tizenclaw-tool-executor started (pid $(cat "${TOOL_EXECUTOR_PID_FILE}"))"
  else
    echo -e "  ${YELLOW}[DRY-RUN]${NC} ${INSTALL_DIR}/${TOOL_EXECUTOR_NAME} &"
  fi

  # Start main daemon in background
  log "Starting tizenclaw daemon..."
  if [ "${DRY_RUN}" = false ]; then
    "${INSTALL_DIR}/${PKG_NAME}" &
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
    ok "Build complete. Binaries in: ${PROJECT_DIR}/target/${BUILD_MODE}/"
    exit 0
  fi

  do_install
  do_run
  show_summary
}

main "$@"
