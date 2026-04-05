#!/bin/bash
# TizenClaw CI Build Script
# Runs gbs build and verifies the result.
#
# Usage:
#   ./scripts/ci_build.sh          # Default x86_64
#   ./scripts/ci_build.sh aarch64  # ARM64 build

set -euo pipefail

ARCH="${1:-x86_64}"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
GBS_ROOT="${HOME}/GBS-ROOT"
REPO_DIR="${GBS_ROOT}/local/repos/tizen/${ARCH}"
LOGS_SUCCESS="${REPO_DIR}/logs/success"
LOGS_FAIL="${REPO_DIR}/logs/fail"
RPMS_DIR="${REPO_DIR}/RPMS"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
NC='\033[0m'

log()  { echo -e "${CYAN}[CI]${NC} $*"; }
ok()   { echo -e "${GREEN}[OK]${NC} $*"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
fail() { echo -e "${RED}[FAIL]${NC} $*"; exit 1; }

# --------------------------------------------------
# Step 1: Pre-flight checks
# --------------------------------------------------
log "Architecture: ${ARCH}"
log "Project dir:  ${PROJECT_DIR}"

if ! command -v gbs &>/dev/null; then
  fail "gbs not found. Install Tizen GBS first."
fi

# --------------------------------------------------
# Step 2: Optional lint (non-blocking)
# --------------------------------------------------
log "Running optional lint checks..."

if command -v cppcheck &>/dev/null; then
  log "  cppcheck..."
  if cppcheck --enable=warning,style \
    --std=c++17 \
    --suppress=missingIncludeSystem \
    --suppress=unmatchedSuppression \
    --quiet \
    -I "${PROJECT_DIR}/inc" \
    -I "${PROJECT_DIR}/inc/nlohmann" \
    "${PROJECT_DIR}/src/" \
    "${PROJECT_DIR}/inc/" 2>&1; then
    ok "  cppcheck passed"
  else
    warn "  cppcheck found issues (non-blocking)"
  fi
else
  warn "  cppcheck not installed, skipping"
fi

if command -v ruff &>/dev/null; then
  log "  ruff (Python)..."
  if ruff check "${PROJECT_DIR}/workspace/skills/" 2>&1; then
    ok "  ruff passed"
  else
    warn "  ruff found issues (non-blocking)"
  fi
else
  warn "  ruff not installed, skipping"
fi

# --------------------------------------------------
# Step 3: GBS Build
# --------------------------------------------------
log "Starting gbs build -A ${ARCH} --include-all"
echo ""

cd "${PROJECT_DIR}"
if gbs build -A "${ARCH}" --include-all; then
  ok "GBS build succeeded"
else
  fail "GBS build failed. Check logs: ${LOGS_FAIL}/"
fi

# --------------------------------------------------
# Step 4: Verify results
# --------------------------------------------------
echo ""

# Check for success logs
if [ -d "${LOGS_SUCCESS}" ]; then
  LATEST_LOG=$(ls -t "${LOGS_SUCCESS}/" 2>/dev/null \
    | head -1)
  if [ -n "${LATEST_LOG}" ]; then
    ok "Build log: ${LOGS_SUCCESS}/${LATEST_LOG}"
  fi
fi

# Check for RPMs
RPM_FILE=$(find "${RPMS_DIR}" -name "tizenclaw-*.rpm" \
  -newer "${PROJECT_DIR}/CMakeLists.txt" \
  2>/dev/null | head -1)

if [ -n "${RPM_FILE}" ]; then
  ok "RPM: ${RPM_FILE}"
  RPM_SIZE=$(du -h "${RPM_FILE}" | cut -f1)
  log "RPM size: ${RPM_SIZE}"
else
  warn "No recent RPM found in ${RPMS_DIR}/"
fi

# Check for unit test RPM
UNITTEST_RPM=$(find "${RPMS_DIR}" \
  -name "tizenclaw-unittests-*.rpm" \
  -newer "${PROJECT_DIR}/CMakeLists.txt" \
  2>/dev/null | head -1)

if [ -n "${UNITTEST_RPM}" ]; then
  ok "Test RPM: ${UNITTEST_RPM}"
fi

echo ""
ok "Build complete! Next steps:"
log "  1. Deploy: /deploy_to_emulator"
log "  2. Logs:   sdb shell journalctl -u tizenclaw -f"
