#!/bin/bash
# ==============================================================================
# TizenClaw Target Python Setup Script
#
# Downloads standard python3 packages from the repo configured in repo_config.ini
# and installs them on the connected Tizen device via SDB.
# Useful for devices lacking internet access or missing the python runtime.
# ==============================================================================

set -e

# ANSI Colors
CYAN='\033[0;36m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

echo -e "${CYAN}====================================================${NC}"
echo -e "${CYAN}      TizenClaw Target Python Environment Setup     ${NC}"
echo -e "${CYAN}====================================================${NC}"

# Check for SDB
if ! command -v sdb >/dev/null 2>&1; then
    echo -e "${RED}[ERROR] sdb is not in PATH. Please install Tizen Studio and add 'tools' to PATH.${NC}"
    exit 1
fi

# Ensure device is connected
DEVICE_COUNT=$(sdb devices | grep -v "List of devices attached" | grep -c "device" || true)
if [ "$DEVICE_COUNT" -eq 0 ]; then
    echo -e "${RED}[ERROR] No Tizen device/emulator connected. Run 'sdb devices'.${NC}"
    exit 1
fi

REMOTEARCH=$(sdb shell uname -m | tr -d '\r\n')
# Map x86_64 to x86_64, armv7l to armv7l, aarch64 to aarch64, i686 to i586
if [[ "$REMOTEARCH" == "i686" ]]; then
    ARCH="i586"
else
    ARCH="$REMOTEARCH"
fi
echo -e "${GREEN}[ OK ] Target Architecture detected: ${ARCH}${NC}"

# Read base repo URL from repo_config.ini
REPO_INI="repo_config.ini"
if [ ! -f "$REPO_INI" ]; then
    echo -e "${RED}[ERROR] ${REPO_INI} not found in current directory.${NC}"
    exit 1
fi

# Parse the base URL. e.g. base = https://download.tizen.org/.../packages/
BASE_REPO_URL=$(awk -F '=' '/^base *[=]/ {gsub(/[ \t]+/, "", $2); print $2}' "$REPO_INI")
if [ -z "$BASE_REPO_URL" ]; then
    echo -e "${RED}[ERROR] Could not find 'base' repository URL in ${REPO_INI}${NC}"
    exit 1
fi

# Ensure trailing slash
[[ "${BASE_REPO_URL}" != */ ]] && BASE_REPO_URL="${BASE_REPO_URL}/"
REPO_URL="${BASE_REPO_URL}${ARCH}/"

echo -e "${GREEN}[ OK ] Python repository: ${REPO_URL}${NC}"

OUTDIR="build/python_rpms_${ARCH}"
mkdir -p "$OUTDIR"

# Required Python core packages
PACKAGES=(
    "python3-base"
    "python3"
    "libpython"  # Sometimes named libpython3 or libpython3.x
)

echo -e "${CYAN}[DOWNLOAD] Fetching repository index...${NC}"
INDEX_CONTENT=$(curl -sL "$REPO_URL" || echo "")

if [ -z "$INDEX_CONTENT" ]; then
    echo -e "${RED}[ERROR] Failed to fetch repository index from ${REPO_URL}. Check internet connection.${NC}"
    exit 1
fi

DOWNLOADED_RPMS=()

for PKG in "${PACKAGES[@]}"; do
    echo -e "  -> Searching for ${PKG}..."
    # Heuristic parsing of directory index (works for standard Apache/Nginx autoindex)
    # Extracts exactly the href that starts with the package name.
    # Exclude -devel, -debuginfo, etc.
    MATCH=$(echo "$INDEX_CONTENT" | grep -oE "href=\"${PKG}-[0-9][^\"]+\.rpm\"" | grep -v "debuginfo" | grep -v "devel" | cut -d'"' -f2 | sort -V | tail -n 1)
    
    if [ -z "$MATCH" ]; then
        # Check libpython3 instead of libpython
        if [ "$PKG" = "libpython" ]; then
            MATCH=$(echo "$INDEX_CONTENT" | grep -oE "href=\"libpython3-[0-9][^\"]+\.rpm\"" | grep -v "debuginfo" | grep -v "devel" | cut -d'"' -f2 | sort -V | tail -n 1)
        fi
        
        if [ -z "$MATCH" ]; then
             echo -e "${YELLOW}[WARN] Could not find ${PKG} in remote repo.${NC}"
             continue
        fi
    fi
    
    FILE_URL="${REPO_URL}${MATCH}"
    echo -e "  ${GREEN}[FOUND]${NC} ${MATCH}"
    
    # Download if not present
    if [ ! -f "${OUTDIR}/${MATCH}" ]; then
        echo -e "  Downloading..."
        curl -SL -o "${OUTDIR}/${MATCH}" "${FILE_URL}"
    else
        echo -e "  Already downloaded."
    fi
    DOWNLOADED_RPMS+=("${OUTDIR}/${MATCH}")
done

if [ ${#DOWNLOADED_RPMS[@]} -eq 0 ]; then
    echo -e "${RED}[ERROR] No Python packages found/downloaded. Exiting.${NC}"
    exit 1
fi

echo -e "${CYAN}====================================================${NC}"
echo -e "${CYAN}      Deploying Packages to Target Device           ${NC}"
echo -e "${CYAN}====================================================${NC}"

sdb root on >/dev/null 2>&1
sdb shell "mkdir -p /tmp/python_setup"

for RPM_FILE in "${DOWNLOADED_RPMS[@]}"; do
    FNAME=$(basename "$RPM_FILE")
    echo -e "  Pushing ${FNAME}..."
    sdb push "$RPM_FILE" "/tmp/python_setup/${FNAME}" >/dev/null
done

echo -e "${CYAN}[INSTALL] Installing RPMs on the device...${NC}"
sdb shell "rpm -Uvh --force --nodeps /tmp/python_setup/*.rpm"

# Verify installation
VERSION=$(sdb shell "python3 --version 2>/dev/null" | tr -d '\r\n')
if [[ "$VERSION" == Python* ]]; then
    echo -e "${GREEN}[ SUCCESS ] Python Environment setup complete: ${VERSION}${NC}"
else
    echo -e "${RED}[ ERROR ] Python3 installation may have failed.${NC}"
fi

# Clean up remote
sdb shell "rm -rf /tmp/python_setup"

echo -e "${CYAN}====================================================${NC}"
