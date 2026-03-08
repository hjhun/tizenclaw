#!/bin/bash
# TizenClaw Build, Deploy & Run Script
# Automates: gbs build → sdb push → rpm install → service restart
#
# Usage:
#   ./scripts/deploy.sh                    # Full pipeline (build + deploy)
#   ./scripts/deploy.sh -s                 # Skip build, deploy only
#   ./scripts/deploy.sh -n                 # Use --noinit for faster rebuild
#   ./scripts/deploy.sh --dry-run          # Print commands without executing
#   ./scripts/deploy.sh -d <serial>        # Target a specific sdb device
#
# See ./scripts/deploy.sh --help for all options.

set -euo pipefail

# ─────────────────────────────────────────────
# Constants
# ─────────────────────────────────────────────
PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
GBS_ROOT="${HOME}/GBS-ROOT"
PKG_NAME="tizenclaw"

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
ARCH="x86_64"
NOINIT=false
SKIP_BUILD=false
DRY_RUN=false
DEVICE_SERIAL=""

# ─────────────────────────────────────────────
# Logging helpers
# ─────────────────────────────────────────────
log()    { echo -e "${CYAN}[DEPLOY]${NC} $*"; }
ok()     { echo -e "${GREEN}[  OK  ]${NC} $*"; }
warn()   { echo -e "${YELLOW}[ WARN ]${NC} $*"; }
fail()   { echo -e "${RED}[ FAIL ]${NC} $*"; exit 1; }
header() { echo -e "\n${BOLD}══════════════════════════════════════════${NC}"; echo -e "${BOLD}  $*${NC}"; echo -e "${BOLD}══════════════════════════════════════════${NC}"; }

# ─────────────────────────────────────────────
# sdb wrapper (supports -s <serial>)
# ─────────────────────────────────────────────
sdb_cmd() {
  if [ -n "${DEVICE_SERIAL}" ]; then
    sdb -s "${DEVICE_SERIAL}" "$@"
  else
    sdb "$@"
  fi
}

sdb_shell() {
  sdb_cmd shell "$@"
}

# ─────────────────────────────────────────────
# Dry-run wrapper
# ─────────────────────────────────────────────
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
${BOLD}TizenClaw Build, Deploy & Run${NC}

${CYAN}Usage:${NC}
  $(basename "$0") [options]

${CYAN}Options:${NC}
  -a, --arch <arch>     Build architecture (default: x86_64)
  -n, --noinit          Skip build-env init (faster rebuild)
  -s, --skip-build      Skip GBS build, deploy existing RPM
  -d, --device <serial> Target a specific sdb device
      --dry-run         Print commands without executing
  -h, --help            Show this help

${CYAN}Examples:${NC}
  $(basename "$0")                     # Full build + deploy + run
  $(basename "$0") -n                  # Quick rebuild + deploy + run
  $(basename "$0") -s                  # Deploy existing RPM + run
  $(basename "$0") --dry-run           # Preview all steps
  $(basename "$0") -a aarch64          # Build for ARM64 target
  $(basename "$0") -d emulator-26101   # Target specific device
EOF
  exit 0
}

# ─────────────────────────────────────────────
# Argument parsing
# ─────────────────────────────────────────────
parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      -a|--arch)       ARCH="$2"; shift 2 ;;
      -n|--noinit)     NOINIT=true; shift ;;
      -s|--skip-build) SKIP_BUILD=true; shift ;;
      -d|--device)     DEVICE_SERIAL="$2"; shift 2 ;;
      --dry-run)       DRY_RUN=true; shift ;;
      -h|--help)       usage ;;
      *)               fail "Unknown option: $1 (use --help)" ;;
    esac
  done
}

# ─────────────────────────────────────────────
# Step 0: Pre-flight checks
# ─────────────────────────────────────────────
check_prerequisites() {
  header "Pre-flight Checks"

  if [ "${SKIP_BUILD}" = false ]; then
    if ! command -v gbs &>/dev/null; then
      if [ "${DRY_RUN}" = true ]; then
        warn "gbs not found (ignored in dry-run)"
      else
        fail "gbs not found. Install Tizen GBS first."
      fi
    else
      ok "gbs found"
    fi
  fi

  if ! command -v sdb &>/dev/null; then
    if [ "${DRY_RUN}" = true ]; then
      warn "sdb not found (ignored in dry-run)"
    else
      fail "sdb not found. Install Tizen sdb first."
    fi
  else
    ok "sdb found"
  fi

  log "Architecture : ${ARCH}"
  log "Project dir  : ${PROJECT_DIR}"
  log "Skip build   : ${SKIP_BUILD}"
  log "No-init      : ${NOINIT}"
  log "Dry-run      : ${DRY_RUN}"
  if [ -n "${DEVICE_SERIAL}" ]; then
    log "Device       : ${DEVICE_SERIAL}"
  fi
}

# ─────────────────────────────────────────────
# Step 1: GBS Build
# ─────────────────────────────────────────────
do_build() {
  if [ "${SKIP_BUILD}" = true ]; then
    log "Skipping build (--skip-build)"
    return 0
  fi

  header "Step 1/4: GBS Build"

  local gbs_args=("-A" "${ARCH}" "--include-all")
  if [ "${NOINIT}" = true ]; then
    gbs_args+=("--noinit")
    log "Using --noinit (skipping build-env initialization)"
  fi

  log "Running: gbs build ${gbs_args[*]}"
  cd "${PROJECT_DIR}"

  if run gbs build "${gbs_args[@]}"; then
    ok "GBS build succeeded"
  else
    fail "GBS build failed. Check logs: ${GBS_ROOT}/local/repos/tizen/${ARCH}/logs/fail/"
  fi
}

# ─────────────────────────────────────────────
# Step 2: Find the built RPM
# ─────────────────────────────────────────────
RPM_FILE=""

find_rpm() {
  header "Step 2/4: Locating RPM"

  local rpms_dir="${GBS_ROOT}/local/repos/tizen/${ARCH}/RPMS"

  if [ "${DRY_RUN}" = true ]; then
    RPM_FILE="${rpms_dir}/${PKG_NAME}-1.0.0-1.${ARCH}.rpm"
    log "[DRY-RUN] Assuming RPM: ${RPM_FILE}"
    return 0
  fi

  if [ ! -d "${rpms_dir}" ]; then
    fail "RPMS directory not found: ${rpms_dir}\n       Have you run a GBS build first?"
  fi

  # Find the main RPM (exclude -unittests, -debuginfo, -debugsource)
  RPM_FILE=$(find "${rpms_dir}" -maxdepth 1 \
    -name "${PKG_NAME}-[0-9]*.${ARCH}.rpm" \
    ! -name "*-unittests-*" \
    ! -name "*-debuginfo-*" \
    ! -name "*-debugsource-*" \
    -printf '%T@ %p\n' 2>/dev/null \
    | sort -rn | head -1 | cut -d' ' -f2-)

  if [ -z "${RPM_FILE}" ]; then
    fail "No ${PKG_NAME} RPM found in ${rpms_dir}/\n       Run a build first or remove --skip-build"
  fi

  local rpm_size
  rpm_size=$(du -h "${RPM_FILE}" | cut -f1)
  local rpm_time
  rpm_time=$(stat -c '%y' "${RPM_FILE}" | cut -d'.' -f1)

  ok "Found: $(basename "${RPM_FILE}")"
  log "  Size : ${rpm_size}"
  log "  Built: ${rpm_time}"
}

# ─────────────────────────────────────────────
# Step 3: Deploy via sdb
# ─────────────────────────────────────────────
do_deploy() {
  header "Step 3/4: Deploy to Device"

  # 3-1. Check device connectivity
  log "Checking device connectivity..."
  if [ "${DRY_RUN}" = false ]; then
    local device_list
    device_list=$(sdb devices 2>/dev/null | tail -n +2 | grep -v "^$" || true)

    if [ -z "${device_list}" ]; then
      fail "No sdb devices connected.\n       Start a Tizen Emulator or connect a device."
    fi

    local device_count
    device_count=$(echo "${device_list}" | wc -l)

    if [ "${device_count}" -gt 1 ] && [ -z "${DEVICE_SERIAL}" ]; then
      warn "Multiple devices detected. Use -d <serial> to specify one."
      echo "${device_list}"
      fail "Ambiguous target device"
    fi

    ok "Device connected"
    echo "  ${device_list}"
  else
    log "[DRY-RUN] sdb devices"
  fi

  # 3-2. Root access
  log "Acquiring root access..."
  run sdb_cmd root on
  ok "Root access granted"

  # 3-3. Remount filesystem
  log "Remounting root filesystem as read-write..."
  run sdb_shell mount -o remount,rw /
  ok "Filesystem remounted (rw)"

  # 3-4. Push RPM
  local rpm_basename
  rpm_basename=$(basename "${RPM_FILE}")
  log "Pushing ${rpm_basename} to device:/tmp/"
  run sdb_cmd push "${RPM_FILE}" /tmp/
  ok "RPM transferred"

  # 3-5. Install RPM
  log "Installing RPM..."
  run sdb_shell rpm -Uvh --force "/tmp/${rpm_basename}"
  ok "RPM installed"

  # 3-6. Cleanup remote RPM
  log "Cleaning up /tmp/${rpm_basename}..."
  run sdb_shell rm -f "/tmp/${rpm_basename}"
  ok "Cleanup done"
}

# ─────────────────────────────────────────────
# Step 4: Restart service & verify
# ─────────────────────────────────────────────
do_restart_and_run() {
  header "Step 4/4: Restart & Run TizenClaw"

  # 4-1. Daemon reload
  log "Reloading systemd daemon..."
  run sdb_shell systemctl daemon-reload
  ok "Daemon reloaded"

  # 4-2. Restart service
  log "Restarting tizenclaw service..."
  run sdb_shell systemctl restart tizenclaw
  ok "Service restarted"

  # 4-3. Wait briefly for startup
  if [ "${DRY_RUN}" = false ]; then
    sleep 2
  fi

  # 4-4. Check service status
  log "Checking service status..."
  echo ""
  if [ "${DRY_RUN}" = false ]; then
    sdb_shell systemctl status tizenclaw -l --no-pager || true
  else
    log "[DRY-RUN] sdb shell systemctl status tizenclaw -l"
  fi

  echo ""

  # 4-5. Show recent logs
  log "Recent journal logs:"
  echo ""
  if [ "${DRY_RUN}" = false ]; then
    sdb_shell journalctl -u tizenclaw -n 20 --no-pager 2>/dev/null || true
  else
    log "[DRY-RUN] sdb shell journalctl -u tizenclaw -n 20 --no-pager"
  fi
}

# ─────────────────────────────────────────────
# Summary
# ─────────────────────────────────────────────
show_summary() {
  echo ""
  header "Deploy Complete!"
  ok "TizenClaw has been deployed and started successfully."
  echo ""
  log "Useful commands:"
  log "  Logs (follow):  sdb shell journalctl -u tizenclaw -f"
  log "  Status:         sdb shell systemctl status tizenclaw -l"
  log "  Stop:           sdb shell systemctl stop tizenclaw"
  log "  Restart:        sdb shell systemctl restart tizenclaw"
  echo ""
}

# ─────────────────────────────────────────────
# Main
# ─────────────────────────────────────────────
main() {
  parse_args "$@"
  check_prerequisites
  do_build
  find_rpm
  do_deploy
  do_restart_and_run
  show_summary
}

main "$@"
