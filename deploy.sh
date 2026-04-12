#!/bin/bash
# TizenClaw Build, Deploy & Run Script
# Automates: gbs build → sdb push → rpm install → service restart
#
# Usage:
#   ./deploy.sh                    # Full pipeline (build + deploy)
#   ./deploy.sh -s                 # Skip build, deploy only
#   ./deploy.sh --dry-run          # Print commands without executing
#   ./deploy.sh -d <serial>        # Target a specific sdb device
#
# See ./scripts/deploy.sh --help for all options.

set -euo pipefail

# ─────────────────────────────────────────────
# Constants
# ─────────────────────────────────────────────
PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
PKG_NAME="tizenclaw"
GBS_BUILD_LOG="/tmp/gbs_build_output.log"

# Auto-detect sdb if not in PATH (non-interactive shell
# doesn't source ~/.bashrc)
if ! command -v sdb &>/dev/null; then
  for _sdb_candidate in \
      "${HOME}/tizen-studio/tools" \
      "${HOME}/tizen-studio/tools/emulator/bin" \
      "/opt/tizen-studio/tools" \
      "/usr/local/tizen-studio/tools"; do
    if [ -x "${_sdb_candidate}/sdb" ]; then
      export PATH="${_sdb_candidate}:${PATH}"
      break
    fi
  done
fi

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# Repo config
REPO_CONFIG="${PROJECT_DIR}/repo_config.ini"
REPO_BASE=""
REPO_PLATFORM=""
CONFIG_DEVICE_TARGET=""
CONFIG_DEVICE_ARCH=""
CONFIG_BUILD_PROFILE=""

# ─────────────────────────────────────────────
# Defaults
# ─────────────────────────────────────────────
ARCH=""
ARCH_EXPLICIT=false
NOINIT=false
INCREMENTAL=false
SKIP_BUILD=false
SKIP_DEPLOY=false
DRY_RUN=false
DEBUG_MODE=false
WITH_NGROK=false
WITH_CRUN=false
WITH_ASSETS=false
WITH_BRIDGE=false
DEVICE_SERIAL=""
REMOVE_PACKAGE=false

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
# Load repo_config.ini (base / platform URLs)
# ─────────────────────────────────────────────
load_repo_config() {
  if [ ! -f "${REPO_CONFIG}" ]; then
    warn "Repo config not found: ${REPO_CONFIG}"
    return 0
  fi

  local current_section=""
  while IFS= read -r raw_line; do
    local line="${raw_line%%#*}"
    line="$(echo "${line}" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
    [ -z "${line}" ] && continue

    if [[ "${line}" =~ ^\[(.+)\]$ ]]; then
      current_section="${BASH_REMATCH[1]}"
      continue
    fi

    if [[ "${line}" != *=* ]]; then
      continue
    fi

    local key="${line%%=*}"
    local val="${line#*=}"
    key="$(echo "${key}" | sed 's/[[:space:]]*$//')"
    val="$(echo "${val}" | sed 's/^[[:space:]]*//')"

    case "${current_section}:${key}" in
      repos:base) REPO_BASE="${val}" ;;
      repos:platform) REPO_PLATFORM="${val}" ;;
      device:target) CONFIG_DEVICE_TARGET="${val}" ;;
      device:architecture) CONFIG_DEVICE_ARCH="${val}" ;;
      build:profile) CONFIG_BUILD_PROFILE="${val}" ;;
    esac
  done < "${REPO_CONFIG}"

  if [ -n "${REPO_BASE}" ]; then
    ok "Repo base    : ${REPO_BASE}"
  fi
  if [ -n "${REPO_PLATFORM}" ]; then
    ok "Repo platform: ${REPO_PLATFORM}"
  fi
  if [ -n "${CONFIG_DEVICE_TARGET}" ]; then
    ok "Config target: ${CONFIG_DEVICE_TARGET}"
  fi
  if [ -n "${CONFIG_DEVICE_ARCH}" ]; then
    ok "Config arch  : ${CONFIG_DEVICE_ARCH}"
  fi
  if [ -n "${CONFIG_BUILD_PROFILE}" ]; then
    ok "Config profile: ${CONFIG_BUILD_PROFILE}"
  fi
}



# ─────────────────────────────────────────────
# Auto-detect device architecture via sdb
# ─────────────────────────────────────────────
detect_arch() {
  # If user explicitly specified arch via -a, skip auto-detection
  if [ "${ARCH_EXPLICIT}" = true ]; then
    log "Using explicit architecture: ${ARCH}"
    return 0
  fi

  if [ -n "${CONFIG_DEVICE_ARCH}" ]; then
    ARCH="${CONFIG_DEVICE_ARCH}"
    ok "Using architecture from repo_config.ini: ${ARCH}"
    return 0
  fi

  log "Auto-detecting device architecture via sdb..."

  local sdb_cap_cmd=(sdb)
  if [ -n "${DEVICE_SERIAL}" ]; then
    sdb_cap_cmd=(sdb -s "${DEVICE_SERIAL}")
  fi

  local cpu_arch
  cpu_arch=$("${sdb_cap_cmd[@]}" capability 2>/dev/null | grep '^cpu_arch:' | cut -d':' -f2 || true)

  if [ -z "${cpu_arch}" ]; then
    warn "Could not detect device architecture. Falling back to x86_64"
    ARCH="x86_64"
    return 0
  fi

  # Map sdb cpu_arch to GBS-compatible architecture name
  case "${cpu_arch}" in
    armv7)   ARCH="armv7l" ;;
    *)       ARCH="${cpu_arch}" ;;
  esac

  ok "Detected device architecture: ${ARCH} (cpu_arch: ${cpu_arch})"
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
  -a, --arch <arch>     Build architecture (default: repo_config.ini or sdb)
  -n, --noinit          Skip build-env init (faster rebuild)
  -i, --incremental     Use --incremental and --skip-srcrpm for fast iterative build
  -s, --skip-build      Skip GBS build, deploy existing RPM
  -S, --skip-deploy     Skip device deployment, build only

      --with-assets     Also build and deploy tizenclaw-assets
      --with-bridge     Install TizenClawBridge WGT on the device
      --with-crun       Build crun and enable container execution mode
  -w, --with-ngrok      Auto-download and push ngrok binary to the device
  -d, --device <serial> Target a specific sdb device
      --remove          Stop services and uninstall TizenClaw from device
      --dry-run         Print commands without executing
  -h, --help            Show this help

${CYAN}Examples:${NC}
  $(basename "$0")                     # Full build + deploy + run
  $(basename "$0") -n                  # Quick rebuild + deploy + run
  $(basename "$0") -i -n               # Fastest iterative rebuild + deploy + run
  $(basename "$0") -s                  # Deploy existing RPM + run

  $(basename "$0") --with-assets       # Build + deploy including tizenclaw-assets
  $(basename "$0") --with-bridge       # Deploy and install TizenClawBridge WGT
  $(basename "$0") -w                  # Deploy and install ngrok binary
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
      -a|--arch)       ARCH="$2"; ARCH_EXPLICIT=true; shift 2 ;;
      -n|--noinit)     NOINIT=true; shift ;;
      -i|--incremental) INCREMENTAL=true; shift ;;
      -s|--skip-build) SKIP_BUILD=true; shift ;;
      -S|--skip-deploy) SKIP_DEPLOY=true; shift ;;

      --with-assets)   WITH_ASSETS=true; shift ;;
      --with-bridge)   WITH_BRIDGE=true; shift ;;
      --with-crun)     WITH_CRUN=true; shift ;;
      -w|--with-ngrok) WITH_NGROK=true; shift ;;
      -d|--device)     DEVICE_SERIAL="$2"; shift 2 ;;
      --remove)        REMOVE_PACKAGE=true; shift ;;
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

  # Check sdb
  if ! command -v sdb &>/dev/null; then
    if [ "${DRY_RUN}" = true ]; then
      warn "sdb not found (ignored in dry-run)"
    else
      fail "sdb not found. Install Tizen Studio or add sdb to PATH.\n       Searched:\n         ~/tizen-studio/tools/\n         /opt/tizen-studio/tools/"
    fi
  else
    ok "sdb found: $(command -v sdb)"
  fi

  log "Architecture : ${ARCH}"
  log "Project dir  : ${PROJECT_DIR}"
  log "Skip build   : ${SKIP_BUILD}"
  log "Incremental  : ${INCREMENTAL}"
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

  if [ "${INCREMENTAL}" = true ]; then
    gbs_args+=("--incremental" "--skip-srcrpm")
    log "Using --incremental & --skip-srcrpm (fast iterative build)"
  fi

  if [ "${NOINIT}" = true ]; then
    gbs_args+=("--noinit")
    log "Using --noinit (skipping build-env initialization)"
  fi

  if [ "${WITH_CRUN}" = true ]; then
    gbs_args+=("--define" "with_crun 1")
    log "Building WITH crun support (container mode)"
  else
    log "Building WITHOUT crun (default native debug mode)"
  fi

  log "Running: gbs build ${gbs_args[*]}"
  if [ -n "${CONFIG_BUILD_PROFILE}" ]; then
    log "Configured build profile hint: ${CONFIG_BUILD_PROFILE}"
  fi
  cd "${PROJECT_DIR}"

  if [ "${DRY_RUN}" = true ]; then
    echo -e "  ${YELLOW}[DRY-RUN]${NC} gbs build ${gbs_args[*]}"
    ok "GBS build succeeded"
    return 0
  fi

  # Run gbs build and capture output for RPMS path extraction
  if gbs build "${gbs_args[@]}" 2>&1 | tee "${GBS_BUILD_LOG}"; then
    ok "GBS build succeeded"
  else
    fail "GBS build failed. Check the build log: ${GBS_BUILD_LOG}"
  fi

  # Extract RPMS directory from gbs build output
  RPMS_DIR=$(grep -A1 'generated RPM packages can be found from local repo:' "${GBS_BUILD_LOG}" \
    | tail -1 | sed 's/^[[:space:]]*//')

  if [ -n "${RPMS_DIR}" ]; then
    ok "RPMS directory: ${RPMS_DIR}"
  else
    warn "Could not parse RPMS path from build output"
  fi
}

# ─────────────────────────────────────────────
# Step 1.5: Build tizenclaw-assets (if present)
# ─────────────────────────────────────────────

# ─────────────────────────────────────────────
# Step 2: Find the built RPM
# ─────────────────────────────────────────────
RPM_FILES=()
RPMS_DIR=""

find_rpm() {
  header "Step 2/4: Locating RPM"

  if [ "${DRY_RUN}" = true ]; then
    if [ -z "${RPMS_DIR}" ]; then
      RPMS_DIR="${HOME}/GBS-ROOT/local/repos/tizen/${ARCH}/RPMS"
    fi
    RPM_FILES=("${RPMS_DIR}/${PKG_NAME}-1.0.0-1.${ARCH}.rpm")
    log "[DRY-RUN] Assuming RPMs: ${RPM_FILES[*]}"
    return 0
  fi

  # If RPMS_DIR was not set by do_build (e.g. --skip-build or --dry-run),
  # try to find it from the last build log or fall back to searching GBS-ROOT
  if [ -z "${RPMS_DIR}" ]; then
    # Try last build log first
    if [ -f "${GBS_BUILD_LOG}" ]; then
      RPMS_DIR=$(
        grep -A1 'generated RPM packages can be found from local repo:' "${GBS_BUILD_LOG}" \
          | tail -1 | sed 's/^[[:space:]]*//' || true
      )
    fi

    # Fall back to searching under ~/GBS-ROOT
    if [ -z "${RPMS_DIR}" ]; then
      local gbs_root="${HOME}/GBS-ROOT"
      RPMS_DIR=$(find "${gbs_root}" -type d -path "*/${ARCH}/RPMS" 2>/dev/null | head -1 || true)
    fi
  fi

  if [ -z "${RPMS_DIR}" ] || [ ! -d "${RPMS_DIR}" ]; then
    fail "RPMS directory not found: ${RPMS_DIR:-unknown}\n       Have you run a GBS build first?"
  fi

  log "Searching in: ${RPMS_DIR}"

  # Find all matching RPMs (exclude unittests, debuginfo, debugsource, devel)
  if [ "${DEBUG_MODE}" = true ]; then
    log "Debug mode enabled: Including debuginfo packages"
    mapfile -t RPM_FILES < <(find "${RPMS_DIR}" -maxdepth 1 \
      -name "${PKG_NAME}*.rpm" \
      ! -name "*-devel-*" \
      ! -name "*-unittests-*" \
      2>/dev/null | sort)
  else
    mapfile -t RPM_FILES < <(find "${RPMS_DIR}" -maxdepth 1 \
      -name "${PKG_NAME}*.rpm" \
      ! -name "*-devel-*" \
      ! -name "*-unittests-*" \
      ! -name "*-debuginfo-*" \
      ! -name "*-debugsource-*" \
      2>/dev/null | sort)
  fi

  if [ ${#RPM_FILES[@]} -eq 0 ]; then
    fail "No ${PKG_NAME} RPMs found in ${RPMS_DIR}/\n       Run a build first or remove --skip-build"
  fi

  for rpm in "${RPM_FILES[@]}"; do
    local rpm_size=$(du -h "${rpm}" | cut -f1)
    ok "Found: $(basename "${rpm}") (${rpm_size})"
  done
}

# ─────────────────────────────────────────────
# Step 3: Deploy via sdb
# ─────────────────────────────────────────────
do_deploy() {
  if [ "${SKIP_DEPLOY}" = true ]; then
    log "Skipping deployment (--skip-deploy)"
    return 0
  fi

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

  if [ -z "${DEVICE_SERIAL}" ]; then
    DEVICE_SERIAL="${TIZENCLAW_DEVICE:-${CONFIG_DEVICE_TARGET}}"
  fi

  # 3-2. Root access
  log "Acquiring root access..."
  run sdb_cmd root on
  ok "Root access granted"

  # 3-3. Remount filesystem
  log "Remounting root filesystem as read-write..."
  run sdb_shell mount -o remount,rw /
  ok "Filesystem remounted (rw)"

  # 3-4. Push and Install RPMs
  for rpm in "${RPM_FILES[@]}"; do
    local rpm_basename=$(basename "${rpm}")
    local install_output=""
    local install_status=0
    log "Pushing ${rpm_basename} to device:/tmp/"
    run sdb_cmd push "${rpm}" /tmp/
    ok "RPM transferred: ${rpm_basename}"

    log "Installing ${rpm_basename}..."
    if [ "${DRY_RUN}" = true ]; then
      echo -e "  ${YELLOW}[DRY-RUN]${NC} sdb shell pkgcmd -i -q -t rpm -p /tmp/${rpm_basename}"
      ok "RPM installed: ${rpm_basename}"
    else
      install_output="$(sdb_shell pkgcmd -i -q -t rpm -p "/tmp/${rpm_basename}" 2>&1)" || install_status=$?
      printf '%s\n' "${install_output}"

      if [ "${install_status}" -eq 0 ] && grep -q 'key\[end\] val\[ok\]' <<<"${install_output}"; then
        ok "RPM installed via pkgcmd: ${rpm_basename}"
      else
        warn "pkgcmd did not confirm RPM installation. Falling back to rpm -Uvh."
        sdb_shell rpm -Uvh --replacepkgs --replacefiles --force "/tmp/${rpm_basename}" \
          || fail "RPM installation failed for ${rpm_basename}"
        ok "RPM installed via rpm fallback: ${rpm_basename}"
      fi
    fi

    log "Cleaning up /tmp/${rpm_basename}..."
    run sdb_shell rm -f "/tmp/${rpm_basename}"

    # Register the webview app to the Tizen app framework if it was just installed
    if [[ "${rpm_basename}" == *"tizenclaw-webview"* ]]; then
      log "Preloading registry for org.tizen.tizenclew-webview..."
      run sdb_shell tpk-backend --preload -y org.tizen.tizenclew-webview
      ok "App registered to registry"
    fi
  done

  ok "All RPMs processed"

  # 3-5.5. Install Web Dashboard frontend files
  log "Installing Web Dashboard frontend..."
  local web_src="${PROJECT_DIR}/data/web"
  local shared_img_src="${PROJECT_DIR}/data/img"
  local web_dst="/opt/usr/share/tizenclaw/web"
  if [ -d "${web_src}" ]; then
    if [ "${DRY_RUN}" = false ]; then
      sdb_shell "mkdir -p ${web_dst}/img ${web_dst}/sdk ${web_dst}/apps"
      # Push each file individually (sdb push doesn't handle recursive dirs)
      for f in "${web_src}"/*; do
        if [ -f "$f" ]; then
          run sdb_cmd push "$f" "${web_dst}/$(basename "$f")"
        fi
      done
      # Push subdirectories
      if [ -d "${web_src}/img" ]; then
        for f in "${web_src}/img"/*; do
          [ -f "$f" ] && run sdb_cmd push "$f" "${web_dst}/img/$(basename "$f")"
        done
      fi
      if [ -f "${shared_img_src}/tizenclaw.svg" ]; then
        run sdb_cmd push "${shared_img_src}/tizenclaw.svg" \
          "${web_dst}/img/tizenclaw.svg"
      fi
      if [ -d "${web_src}/sdk" ]; then
        for f in "${web_src}/sdk"/*; do
          [ -f "$f" ] && run sdb_cmd push "$f" "${web_dst}/sdk/$(basename "$f")"
        done
      fi
      ok "Web Dashboard frontend installed to ${web_dst}"
    else
      log "[DRY-RUN] Push data/web/* -> ${web_dst}/"
    fi
  else
    warn "Web Dashboard source not found: ${web_src}"
  fi

  # 3-6. Auto-download and install ngrok if requested
  if [ "${WITH_NGROK}" = true ]; then
    log "Auto-installing ngrok..."
    local ngrok_arch
    case "${ARCH}" in
      x86_64)  ngrok_arch="amd64" ;;
      aarch64) ngrok_arch="arm64" ;;
      armv7l)  ngrok_arch="arm" ;;
      *)       fail "Unsupported architecture for ngrok: ${ARCH}" ;;
    esac

    local ngrok_url="https://bin.equinox.io/c/bNyj1mQVY4c/ngrok-v3-stable-linux-${ngrok_arch}.tgz"
    local local_tgz="/tmp/ngrok-${ngrok_arch}.tgz"

    if [ "${DRY_RUN}" = false ]; then
      log "Downloading ${ngrok_url}..."
      curl -sL "${ngrok_url}" -o "${local_tgz}" || fail "Failed to download ngrok"
      
      log "Extracting ngrok..."
      tar -xzf "${local_tgz}" -C /tmp || fail "Failed to extract ngrok"
      
      log "Pushing ngrok to device:/usr/bin/ngrok..."
      run sdb_cmd push /tmp/ngrok /tmp/ngrok
      run sdb_shell mv /tmp/ngrok /usr/bin/ngrok
      run sdb_shell chmod +x /usr/bin/ngrok
      
      log "Cleaning up local /tmp files..."
      rm -f "${local_tgz}" /tmp/ngrok
      ok "ngrok installed to /usr/bin/ngrok"
    else
      log "[DRY-RUN] Download ${ngrok_url} and push to /usr/bin/ngrok"
    fi
  fi

  # 3-7. Install TizenClaw Bridge WGT (only with --with-bridge)
  if [ "${WITH_BRIDGE}" = true ]; then
    local wgt_file="${PROJECT_DIR}/data/wgt/TizenClawBridge.wgt"
    if [ -f "${wgt_file}" ]; then
      log "Installing TizenClaw Bridge WGT..."
      run sdb_cmd push "${wgt_file}" /tmp/TizenClawBridge.wgt
      run sdb_shell pkgcmd -i -t wgt -p /tmp/TizenClawBridge.wgt -q 2>/dev/null || \
        run sdb_shell pkgcmd -i -t wgt -p /tmp/TizenClawBridge.wgt -f -q 2>/dev/null || true
      run sdb_shell rm -f /tmp/TizenClawBridge.wgt
      ok "Bridge WGT installed"
        warn "Bridge WGT not found: ${wgt_file}"
    fi
  fi
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

  # 4-2. Enable and start socket units (on-demand activation)
  log "Enabling socket units..."
  run sdb_shell systemctl enable tizenclaw-tool-executor.socket 2>/dev/null || true
  ok "Socket units enabled"

  log "Restarting tizenclaw-tool-executor socket..."
  run sdb_shell systemctl restart tizenclaw-tool-executor.socket || true
  ok "Tool executor socket restarted"

  # Stop existing service instances (will be socket-activated on demand)
  run sdb_shell systemctl stop tizenclaw-tool-executor 2>/dev/null || true

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
    echo ""
    sdb_shell systemctl status tizenclaw-tool-executor.socket --no-pager || true
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

remove_from_device() {
  header "Remove TizenClaw From Device"

  log "Checking device connectivity..."
  if [ "${DRY_RUN}" = false ]; then
    sdb_cmd devices >/dev/null 2>&1 || fail "sdb devices failed"
  fi

  log "Acquiring root access..."
  run sdb_cmd root on
  run sdb_shell mount -o remount,rw /

  log "Stopping TizenClaw services..."
  run sdb_shell systemctl stop tizenclaw 2>/dev/null || true
  run sdb_shell systemctl stop tizenclaw-tool-executor.service 2>/dev/null || true
  run sdb_shell systemctl stop tizenclaw-tool-executor.socket 2>/dev/null || true
  run sdb_shell systemctl disable tizenclaw-tool-executor.socket 2>/dev/null || true

  log "Removing RPM package..."
  run sdb_shell rpm -e "${PKG_NAME}" 2>/dev/null || true

  log "Cleaning package-owned runtime paths..."
  run sdb_shell rm -rf /opt/usr/share/tizenclaw/tools 2>/dev/null || true
  run sdb_shell rm -rf /opt/usr/share/tizen-tools 2>/dev/null || true

  ok "TizenClaw removal command completed"
}

cleanup_legacy_paths_on_device() {
  header "Legacy Path Cleanup"

  log "Removing legacy tool path..."
  run sdb_shell rm -rf /opt/usr/share/tizen-tools 2>/dev/null || true

  ok "Legacy device paths cleaned"
}



# ─────────────────────────────────────────────
# Summary
# ─────────────────────────────────────────────
show_summary() {
  echo ""
  header "Deploy Complete!"
  ok "TizenClaw has been deployed and started successfully."
  mkdir -p "${PROJECT_DIR}/.tmp"
  touch "${PROJECT_DIR}/.tmp/.deploy_success"
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
  load_repo_config
  if [ "${REMOVE_PACKAGE}" = true ]; then
    detect_arch
    check_prerequisites
    remove_from_device
    exit 0
  fi
  if [ -z "${DEVICE_SERIAL}" ]; then
    DEVICE_SERIAL="${TIZENCLAW_DEVICE:-${CONFIG_DEVICE_TARGET}}"
  fi
  detect_arch
  check_prerequisites
  do_build
  find_rpm
  do_deploy
  cleanup_legacy_paths_on_device
  do_restart_and_run
  show_summary
}

main "$@"
