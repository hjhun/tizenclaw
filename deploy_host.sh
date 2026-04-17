#!/bin/bash
# TizenClaw Host Linux Build & Run Script
# Builds and runs TizenClaw natively on the host Linux (Ubuntu/WSL)
# without Tizen GBS — uses `cargo build` with vendored sources.
#
# Usage:
#   ./deploy_host.sh                   # Build (debug) + install + run
#   ./deploy_host.sh --release         # Build in release mode
#   ./deploy_host.sh -d, --debug       # Build in debug mode
#   ./deploy_host.sh -b, --build-only  # Build only, do not run
#   ./deploy_host.sh --no-restart      # Build + install only
#   ./deploy_host.sh --restart-only    # Restart using installed host files
#   ./deploy_host.sh -s, --stop        # Stop running daemon
#   ./deploy_host.sh --status          # Show daemon status
#   ./deploy_host.sh --log             # Follow daemon logs
#   ./deploy_host.sh --dry-run         # Print commands without executing
#   ./deploy_host.sh --test            # Build + run cargo tests
#   ./deploy_host.sh --devel           # Start daemon with autonomous devel mode
#   ./deploy_host.sh -h, --help        # Show this help

set -euo pipefail

# ─────────────────────────────────────────────
# Constants
# ─────────────────────────────────────────────
PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
ENTRYPOINT_NAME="${TIZENCLAW_HOST_ENTRYPOINT_NAME:-$(basename "$0")}"
PKG_NAME="tizenclaw"
TOOL_EXECUTOR_NAME="tizenclaw-tool-executor"
CLI_NAME="tizenclaw-cli"
TEST_TOOL_NAME="tizenclaw-tests"
WEB_DASHBOARD_NAME="tizenclaw-web-dashboard"
PLATFORM_PLUGIN_NAME="libtizenclaw_plugin.so"
METADATA_PLUGIN_PKG="tizenclaw-metadata-plugin"
HOST_DASHBOARD_PORT_DEFAULT=9091
DEVEL_BRANCH_PREFIX="devel"
DEVEL_OAUTH_REGRESSION_SCENARIO="tests/system/openai_oauth_regression.json"

HOST_BASE_DIR="${HOME}/.tizenclaw"
INSTALL_DIR="${HOST_BASE_DIR}/bin"
LIB_DIR="${HOST_BASE_DIR}/lib"
INCLUDE_DIR="${HOST_BASE_DIR}/include"
PKGCONFIG_DIR="${LIB_DIR}/pkgconfig"
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
IMG_SRC="${PROJECT_DIR}/data/img"
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
BUILD_MODE="${BUILD_MODE:-debug}"
BUILD_ONLY=false
NO_RESTART=false
STOP_DAEMON=false
RESTART_ONLY=false
SHOW_STATUS=false
FOLLOW_LOG=false
DRY_RUN=false
RUN_TESTS=false
REMOVE_INSTALL=false
DEVEL_MODE=false
LLM_CONFIG=""
CARGO_TARGET_DIR_HOST="${CARGO_TARGET_DIR:-${CARGO_TARGET_DIR_DEFAULT}}"
RUST_WORKSPACE_MANIFEST="${PROJECT_DIR}/rust/Cargo.toml"
RUST_WORKSPACE_TARGET_DIR="${BUILD_ROOT_DIR}/rust-cargo-target"

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

current_git_branch() {
  git -C "${PROJECT_DIR}" branch --show-current 2>/dev/null || true
}

require_devel_branch_for_devel_mode() {
  if [ "${DEVEL_MODE}" != true ]; then
    return 0
  fi

  local branch
  branch="$(current_git_branch)"
  if [ -z "${branch}" ]; then
    fail "--devel requires a checked out Git branch under ${PROJECT_DIR}"
  fi

  if [[ "${branch}" != "${DEVEL_BRANCH_PREFIX}"* ]]; then
    fail "--devel is allowed only on ${DEVEL_BRANCH_PREFIX}* branches (current: ${branch})"
  fi

  ok "Devel mode allowed on branch ${branch}"
}

process_report() {
  ps -eo pid,ppid,stat,cmd \
    | grep -E "(${INSTALL_DIR}/${PKG_NAME}|${INSTALL_DIR}/${TOOL_EXECUTOR_NAME}|${INSTALL_DIR}/${WEB_DASHBOARD_NAME}|(^|/| )${PKG_NAME}($| )|(^|/| )${TOOL_EXECUTOR_NAME}($| )|(^|/| )${WEB_DASHBOARD_NAME}($| ))" \
    | grep -v -E "grep -E|deploy_host.sh" || true
}

dashboard_port() {
  python3 - <<'PY' "${CONFIG_DIR}/channel_config.json" "${HOST_DASHBOARD_PORT_DEFAULT}"
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
default_port = int(sys.argv[2])
port = default_port
try:
    if path.exists():
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

normalize_host_dashboard_config() {
  local config_path="${CONFIG_DIR}/channel_config.json"
  log "Normalizing host dashboard port to ${HOST_DASHBOARD_PORT_DEFAULT}"
  if [ "${DRY_RUN}" = false ]; then
    python3 - <<'PY' "${config_path}" "${HOST_DASHBOARD_PORT_DEFAULT}"
import json, pathlib, sys

path = pathlib.Path(sys.argv[1])
port = int(sys.argv[2])

data = {"channels": []}
if path.exists():
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        data = {"channels": []}

channels = data.get("channels")
if not isinstance(channels, list):
    channels = []
    data["channels"] = channels

dashboard = None
for channel in channels:
    if isinstance(channel, dict) and channel.get("name") == "web_dashboard":
        dashboard = channel
        break

if dashboard is None:
    dashboard = {
        "name": "web_dashboard",
        "type": "web_dashboard",
        "enabled": False,
        "settings": {},
    }
    channels.append(dashboard)

settings = dashboard.get("settings")
if not isinstance(settings, dict):
    settings = {}
    dashboard["settings"] = settings

dashboard.setdefault("type", "web_dashboard")
dashboard["enabled"] = False
settings["port"] = port
settings.setdefault("localhost_only", False)

path.parent.mkdir(parents=True, exist_ok=True)
path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
PY
  else
    echo -e "  ${YELLOW}[DRY-RUN]${NC} normalize ${config_path} to port ${HOST_DASHBOARD_PORT_DEFAULT}"
  fi
  ok "Host dashboard config uses port ${HOST_DASHBOARD_PORT_DEFAULT}"
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
  ${ENTRYPOINT_NAME} [options]

${CYAN}Options:${NC}
      --release           Build in release mode
  -d, --debug             Build in debug mode (default)
  -b, --build-only        Build only, do not install or run
      --no-restart        Build and install only, do not restart the daemon
      --test              Build + run cargo tests (offline, vendored)
      --restart-only      Restart the installed host daemon only
  -s, --stop              Stop the running host daemon
      --remove            Stop host processes and remove ~/.tizenclaw install
      --status            Show current daemon status
      --log               Follow daemon log output
      --dry-run           Print commands without executing
      --devel             Start devel mode only on devel* branches
      --build-root <dir>  Override host Cargo target dir
      --llm-config <path> Use specified llm_config.json (sets TIZENCLAW_DATA_DIR)
  -h, --help              Show this help

${CYAN}Examples:${NC}
  ${ENTRYPOINT_NAME}                           # Debug build + install + run
  ${ENTRYPOINT_NAME} --release                 # Release build + install + run
  ${ENTRYPOINT_NAME} -d                        # Debug build + install + run
  ${ENTRYPOINT_NAME} -b                        # Build only
  ${ENTRYPOINT_NAME} --test                    # Run unit/integration tests
  ${ENTRYPOINT_NAME} --status                  # Check daemon status
  ${ENTRYPOINT_NAME} --log                     # Tail daemon logs
  ${ENTRYPOINT_NAME} --devel                   # Start devel scheduler on devel* branch
  ${ENTRYPOINT_NAME} -s                        # Stop the daemon
  ${ENTRYPOINT_NAME} --remove                  # Remove host install and stop tools
  ${ENTRYPOINT_NAME} --build-root /tmp/tc-build  # Use external build root
  ${ENTRYPOINT_NAME} --llm-config /path/to/llm_config.json  # Use custom LLM config
EOF
  exit 0
}

# ─────────────────────────────────────────────
# Argument parsing
# ─────────────────────────────────────────────
parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --release)         BUILD_MODE="release"; shift ;;
      -d|--debug)       BUILD_MODE="debug"; shift ;;
      -b|--build-only)  BUILD_ONLY=true; shift ;;
      --no-restart)     NO_RESTART=true; shift ;;
      --test)           RUN_TESTS=true; shift ;;
      --restart-only)   RESTART_ONLY=true; shift ;;
      -s|--stop)        STOP_DAEMON=true; shift ;;
      --remove)         REMOVE_INSTALL=true; shift ;;
      --status)         SHOW_STATUS=true; shift ;;
      --log)            FOLLOW_LOG=true; shift ;;
      --dry-run)        DRY_RUN=true; shift ;;
      --devel)          DEVEL_MODE=true; shift ;;
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
  log "No restart  : ${NO_RESTART}"
  log "Devel mode  : ${DEVEL_MODE}"
  log "Data dir    : ${DATA_DIR}"
  log "Build root  : ${CARGO_TARGET_DIR_HOST}"
  if [ -f "${RUST_WORKSPACE_MANIFEST}" ]; then
    log "Rust ws     : ${RUST_WORKSPACE_MANIFEST}"
  fi
}

run_rust_workspace_build() {
  local cargo_args=("build" "--manifest-path" "${RUST_WORKSPACE_MANIFEST}" "--workspace" "--offline" "--locked")
  local retry_args=("build" "--manifest-path" "${RUST_WORKSPACE_MANIFEST}" "--workspace")
  if [ "${BUILD_MODE}" = "release" ]; then
    cargo_args+=("--release")
  fi

  if [ ! -f "${RUST_WORKSPACE_MANIFEST}" ]; then
    return 0
  fi

  log "Running canonical rust workspace build: cargo ${cargo_args[*]}"
  if [ "${DRY_RUN}" = true ]; then
    echo -e "  ${YELLOW}[DRY-RUN]${NC} export CARGO_TARGET_DIR='${RUST_WORKSPACE_TARGET_DIR}'"
    echo -e "  ${YELLOW}[DRY-RUN]${NC} cargo ${cargo_args[*]}"
    return 0
  fi

  mkdir -p "${RUST_WORKSPACE_TARGET_DIR}"
  if CARGO_TARGET_DIR="${RUST_WORKSPACE_TARGET_DIR}" cargo "${cargo_args[@]}"; then
    ok "Canonical rust workspace build succeeded (${BUILD_MODE})"
  elif run_rust_workspace_without_vendor "${retry_args[@]}"; then
    warn "Canonical rust workspace build required network-backed dependency resolution"
    ok "Canonical rust workspace build succeeded (${BUILD_MODE})"
  else
    fail "Canonical rust workspace build failed"
  fi
}

run_rust_workspace_tests() {
  local cargo_args=("test" "--manifest-path" "${RUST_WORKSPACE_MANIFEST}" "--workspace" "--offline" "--locked" "--" "--test-threads=1")
  local retry_args=("test" "--manifest-path" "${RUST_WORKSPACE_MANIFEST}" "--workspace" "--" "--test-threads=1")

  if [ ! -f "${RUST_WORKSPACE_MANIFEST}" ]; then
    return 0
  fi

  log "Running canonical rust workspace tests: cargo ${cargo_args[*]}"
  if [ "${DRY_RUN}" = true ]; then
    echo -e "  ${YELLOW}[DRY-RUN]${NC} export CARGO_TARGET_DIR='${RUST_WORKSPACE_TARGET_DIR}'"
    echo -e "  ${YELLOW}[DRY-RUN]${NC} cargo ${cargo_args[*]}"
    return 0
  fi

  mkdir -p "${RUST_WORKSPACE_TARGET_DIR}"
  if CARGO_TARGET_DIR="${RUST_WORKSPACE_TARGET_DIR}" cargo "${cargo_args[@]}" 2>&1; then
    ok "Canonical rust workspace tests passed"
  elif run_rust_workspace_without_vendor "${retry_args[@]}"; then
    warn "Canonical rust workspace tests required network-backed dependency resolution"
    ok "Canonical rust workspace tests passed"
  else
    fail "Canonical rust workspace tests failed"
  fi
}

run_rust_workspace_without_vendor() {
  local cargo_config="${PROJECT_DIR}/.cargo/config.toml"
  local cargo_config_backup="${PROJECT_DIR}/.cargo/config.toml.deploy_host_backup"

  if [ ! -f "${cargo_config}" ]; then
    CARGO_TARGET_DIR="${RUST_WORKSPACE_TARGET_DIR}" cargo "$@"
    return $?
  fi

  mv "${cargo_config}" "${cargo_config_backup}"
  if CARGO_TARGET_DIR="${RUST_WORKSPACE_TARGET_DIR}" cargo "$@"; then
    mv "${cargo_config_backup}" "${cargo_config}"
    return 0
  else
    local status=$?
    mv "${cargo_config_backup}" "${cargo_config}"
    return "${status}"
  fi
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

  for legacy_bin in "${PKG_NAME}" "${TOOL_EXECUTOR_NAME}" "${CLI_NAME}" "${TEST_TOOL_NAME}" "${WEB_DASHBOARD_NAME}"; do
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

  local cargo_args=("build" "--workspace" "--offline" "--locked")
  if [ "${BUILD_MODE}" = "release" ]; then
    cargo_args+=("--release")
  fi

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

  run_rust_workspace_build
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

  log "Running: cargo test --workspace --offline --locked"
  cd "${PROJECT_DIR}"

  if [ "${DRY_RUN}" = true ]; then
    echo -e "  ${YELLOW}[DRY-RUN]${NC} export CARGO_TARGET_DIR='${CARGO_TARGET_DIR_HOST}'"
    echo -e "  ${YELLOW}[DRY-RUN]${NC} cargo test --workspace --offline --locked"
    return 0
  fi

  mkdir -p "${CARGO_TARGET_DIR_HOST}"

  if CARGO_TARGET_DIR="${CARGO_TARGET_DIR_HOST}" cargo test --workspace --offline --locked -- --test-threads=1 2>&1; then
    ok "All tests passed"
  else
    fail "Some tests failed (see output above)"
  fi

  run_rust_workspace_tests

  log "Running reconstruction parity harness"
  if bash "${PROJECT_DIR}/rust/scripts/run_mock_parity_harness.sh"; then
    ok "Mock parity harness passed"
  else
    fail "Mock parity harness failed"
  fi

  log "Running documentation-driven architecture verification"
  if python3 "${PROJECT_DIR}/scripts/verify_doc_architecture.py"; then
    ok "Documentation-driven verification passed"
  else
    fail "Documentation-driven verification failed"
  fi

  log "Running offline system contract suite"
  local bin_dir="${CARGO_TARGET_DIR_HOST}/debug"
  if [ "${BUILD_MODE}" = "release" ]; then
    bin_dir="${CARGO_TARGET_DIR_HOST}/release"
  fi
  if bash "${PROJECT_DIR}/scripts/run_host_system_contracts.sh" \
      --bin-dir "${bin_dir}"; then
    ok "Offline system contract suite passed"
  else
    fail "Offline system contract suite failed (see output above)"
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
  run mkdir -p "${INSTALL_DIR}" "${LIB_DIR}" "${INCLUDE_DIR}/tizenclaw" \
    "${INCLUDE_DIR}/tizenclaw/core" "${PKGCONFIG_DIR}" "${CONFIG_DIR}" "${TOOLS_DIR}/cli" \
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

  for bin in "${PKG_NAME}" "${TOOL_EXECUTOR_NAME}" "${CLI_NAME}" "${TEST_TOOL_NAME}" "${WEB_DASHBOARD_NAME}"; do
    local bin_path="${build_dir}/${bin}"
    if [ "${DRY_RUN}" = false ] && [ ! -f "${bin_path}" ]; then
      fail "Binary not found: ${bin_path}"
    fi
    log "Installing ${bin} → ${INSTALL_DIR}/${bin}"
    run install -m 755 "${bin_path}" "${INSTALL_DIR}/${bin}"
    ok "Installed: ${bin}"
  done

  local lib_candidates=(
    "libtizenclaw.so"
    "libtizenclaw.rlib"
    "libtizenclaw_core.so"
    "libtizenclaw_core.rlib"
    "${PLATFORM_PLUGIN_NAME}"
  )
  for lib_name in "${lib_candidates[@]}"; do
    local lib_path="${build_dir}/${lib_name}"
    if [ ! -f "${lib_path}" ]; then
      continue
    fi
    log "Installing ${lib_name} → ${LIB_DIR}/${lib_name}"
    run install -m 755 "${lib_path}" "${LIB_DIR}/${lib_name}"
    ok "Installed library: ${lib_name}"
  done

  local platform_plugin_path="${build_dir}/${PLATFORM_PLUGIN_NAME}"
  if [ "${DRY_RUN}" = false ] && [ -f "${platform_plugin_path}" ]; then
    log "Installing platform plugin → ${DATA_DIR}/plugins/${PLATFORM_PLUGIN_NAME}"
    run install -m 755 "${platform_plugin_path}" \
      "${DATA_DIR}/plugins/${PLATFORM_PLUGIN_NAME}"
    ok "Installed platform plugin"
  fi

  log "Installing public headers → ${INCLUDE_DIR}/tizenclaw"
  run install -m 644 "${PROJECT_DIR}/src/libtizenclaw/include/tizenclaw.h" \
    "${INCLUDE_DIR}/tizenclaw/tizenclaw.h"
  run install -m 644 "${PROJECT_DIR}/src/libtizenclaw-core/include/tizenclaw_error.h" \
    "${INCLUDE_DIR}/tizenclaw/tizenclaw_error.h"
  run install -m 644 "${PROJECT_DIR}/src/libtizenclaw-core/include/tizenclaw_channel.h" \
    "${INCLUDE_DIR}/tizenclaw/core/tizenclaw_channel.h"
  run install -m 644 "${PROJECT_DIR}/src/libtizenclaw-core/include/tizenclaw_llm_backend.h" \
    "${INCLUDE_DIR}/tizenclaw/core/tizenclaw_llm_backend.h"
  run install -m 644 "${PROJECT_DIR}/src/libtizenclaw-core/include/tizenclaw_curl.h" \
    "${INCLUDE_DIR}/tizenclaw/core/tizenclaw_curl.h"
  ok "Headers installed"

  log "Generating host pkg-config metadata"
  if [ "${DRY_RUN}" = false ]; then
    cat > "${PKGCONFIG_DIR}/tizenclaw.pc" <<EOF
prefix=${HOST_BASE_DIR}
libdir=\${prefix}/lib
includedir=\${prefix}/include

Name: tizenclaw
Description: TizenClaw Agent C API library
Version: 1.0.0
Libs: -L\${libdir} -Wl,-rpath,\${libdir} -ltizenclaw
Cflags: -I\${includedir} -I\${includedir}/tizenclaw
EOF

    cat > "${PKGCONFIG_DIR}/tizenclaw-core.pc" <<EOF
prefix=${HOST_BASE_DIR}
libdir=\${prefix}/lib
includedir=\${prefix}/include

Name: tizenclaw-core
Description: TizenClaw Plugin SDK
Version: 1.0.0
Libs: -L\${libdir} -Wl,-rpath,\${libdir} -ltizenclaw_core
Cflags: -I\${includedir}/tizenclaw/core -I\${includedir}/tizenclaw
Requires: tizenclaw, libcurl
EOF
  else
    echo -e "  ${YELLOW}[DRY-RUN]${NC} write ${PKGCONFIG_DIR}/tizenclaw.pc"
    echo -e "  ${YELLOW}[DRY-RUN]${NC} write ${PKGCONFIG_DIR}/tizenclaw-core.pc"
  fi
  ok "pkg-config metadata installed"

  # Deploy web dashboard
  if [ -d "${WEB_SRC}" ]; then
    log "Installing web dashboard → ${DATA_DIR}/web"
    run cp -r "${WEB_SRC}/." "${DATA_DIR}/web/"
    ok "Web dashboard installed"
  fi

  if [ -f "${IMG_SRC}/tizenclaw.svg" ]; then
    log "Installing shared dashboard logo → ${DATA_DIR}/web/img"
    run mkdir -p "${DATA_DIR}/web/img"
    run install -m 644 "${IMG_SRC}/tizenclaw.svg" \
      "${DATA_DIR}/web/img/tizenclaw.svg"
    ok "Shared dashboard logo installed"
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

  normalize_host_dashboard_config

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

  if pgrep -f "${INSTALL_DIR}/${TOOL_EXECUTOR_NAME}" &>/dev/null; then
    run pkill -f "${INSTALL_DIR}/${TOOL_EXECUTOR_NAME}" || true
  fi
  if pgrep -f "${INSTALL_DIR}/${CLI_NAME}" &>/dev/null; then
    run pkill -f "${INSTALL_DIR}/${CLI_NAME}" || true
  fi
  if pgrep -f "${INSTALL_DIR}/${WEB_DASHBOARD_NAME}" &>/dev/null; then
    run pkill -f "${INSTALL_DIR}/${WEB_DASHBOARD_NAME}" || true
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

  for legacy_bin in "${PKG_NAME}" "${TOOL_EXECUTOR_NAME}" "${CLI_NAME}" "${TEST_TOOL_NAME}" "${WEB_DASHBOARD_NAME}"; do
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

  require_devel_branch_for_devel_mode

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
  export LD_LIBRARY_PATH="${LIB_DIR}${LD_LIBRARY_PATH:+:${LD_LIBRARY_PATH}}"
  export PKG_CONFIG_PATH="${PKGCONFIG_DIR}${PKG_CONFIG_PATH:+:${PKG_CONFIG_PATH}}"

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
  local daemon_args=()
  if [ "${DEVEL_MODE}" = true ]; then
    daemon_args+=("--devel")
  fi
  if [ "${DRY_RUN}" = false ]; then
    setsid "${INSTALL_DIR}/${PKG_NAME}" "${daemon_args[@]}" \
      >> "${LOG_DIR}/tizenclaw.stdout.log" 2>&1 < /dev/null &
    echo $! > "${PID_FILE}"
    sleep 1
    if kill -0 "$(cat "${PID_FILE}")" 2>/dev/null; then
      ok "tizenclaw daemon started (pid $(cat "${PID_FILE}"))"
    else
      fail "tizenclaw daemon failed to start — check logs: ${LOG_DIR}/tizenclaw.log"
    fi
  else
    echo -e "  ${YELLOW}[DRY-RUN]${NC} ${INSTALL_DIR}/${PKG_NAME} ${daemon_args[*]} &"
  fi
}

wait_for_ipc_ready() {
  header "IPC Readiness Check"

  local test_cmd=("${INSTALL_DIR}/${TEST_TOOL_NAME}" "call" "--method" "ping")
  local socket_path="${TIZENCLAW_SOCKET_PATH:-}"
  local deadline=$((SECONDS + 5))
  local socket_seen=false

  if [ "${DRY_RUN}" = true ]; then
    echo -e "  ${YELLOW}[DRY-RUN]${NC} wait for daemon IPC readiness"
    return 0
  fi

  while [ "${SECONDS}" -lt "${deadline}" ]; do
    if [ -n "${socket_path}" ] && [ -S "${socket_path}" ]; then
      socket_seen=true
    fi

    if "${test_cmd[@]}" >/dev/null 2>&1; then
      if [ -n "${socket_path}" ]; then
        ok "Daemon IPC is ready at ${socket_path}"
      elif [ "${socket_seen}" = true ]; then
        ok "Daemon IPC is ready"
      else
        ok "Daemon IPC is ready via abstract socket"
      fi
      return 0
    fi

    sleep 0.2
  done

  fail "Timed out waiting for daemon IPC readiness"
}

run_devel_entry_tests() {
  if [ "${DEVEL_MODE}" != true ]; then
    return 0
  fi

  header "Devel Entry Regression Check"
  log "Running: ${INSTALL_DIR}/${TEST_TOOL_NAME} scenario --file ${DEVEL_OAUTH_REGRESSION_SCENARIO}"

  if [ "${DRY_RUN}" = true ]; then
    echo -e "  ${YELLOW}[DRY-RUN]${NC} ${INSTALL_DIR}/${TEST_TOOL_NAME} scenario --file ${DEVEL_OAUTH_REGRESSION_SCENARIO}"
    ok "Skipped devel entry regression check (dry-run)"
    return 0
  fi

  # Devel mode should fail fast when the linked Codex OAuth cache regresses.
  # Give the freshly spawned daemon a short readiness window so we validate
  # the live service rather than racing the socket startup.
  local attempt
  local max_attempts=10
  for attempt in $(seq 1 "${max_attempts}"); do
    if "${INSTALL_DIR}/${TEST_TOOL_NAME}" scenario --file "${DEVEL_OAUTH_REGRESSION_SCENARIO}"; then
      ok "Devel entry OAuth regression check passed"
      return 0
    fi
    if [ "${attempt}" -lt "${max_attempts}" ]; then
      warn "Devel regression check hit a startup race; retrying (${attempt}/${max_attempts})"
      sleep 1
    fi
  done

  fail "Devel entry OAuth regression check failed"
}

ensure_existing_install() {
  if [ ! -x "${INSTALL_DIR}/${PKG_NAME}" ]; then
    fail "Installed host binary not found: ${INSTALL_DIR}/${PKG_NAME}"
  fi
  if [ ! -x "${INSTALL_DIR}/${TOOL_EXECUTOR_NAME}" ]; then
    fail "Installed tool executor not found: ${INSTALL_DIR}/${TOOL_EXECUTOR_NAME}"
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
  local host_dashboard_port
  host_dashboard_port="$(dashboard_port)"
  log "Useful commands:"
  log "  Logs (follow)  : ./${ENTRYPOINT_NAME} --log"
  log "  Status         : ./${ENTRYPOINT_NAME} --status"
  log "  Stop           : ./${ENTRYPOINT_NAME} --stop"
  log "  Remove         : ./${ENTRYPOINT_NAME} --remove"
  log "  Devel          : ./${ENTRYPOINT_NAME} --devel"
  log "  CLI test       : tizenclaw-cli 'hello'"
  log "  System test    : tizenclaw-tests scenario --file tests/system/basic_ipc_smoke.json"
  log "  Dashboard URL  : http://localhost:${host_dashboard_port}"
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

  if [ "${RESTART_ONLY}" = true ]; then
    ensure_existing_install
    do_run
    show_summary
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
  if [ "${NO_RESTART}" = true ]; then
    ok "Build and install complete. Daemon restart skipped."
    exit 0
  fi
  do_run
  wait_for_ipc_ready
  run_devel_entry_tests
  show_summary
}

main "$@"
