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

# Parse the base and platform URLs
BASE_REPO_URL=$(awk -F '=' '/^base *[=]/ {gsub(/[ \t]+/, "", $2); print $2}' "$REPO_INI")
PLATFORM_REPO_URL=$(awk -F '=' '/^platform *[=]/ {gsub(/[ \t]+/, "", $2); print $2}' "$REPO_INI")

if [ -z "$BASE_REPO_URL" ]; then
    echo -e "${RED}[ERROR] Could not find 'base' repository URL in ${REPO_INI}${NC}"
    exit 1
fi

# Ensure trailing slashes
[[ "${BASE_REPO_URL}" != */ ]] && BASE_REPO_URL="${BASE_REPO_URL}/"
[[ -n "${PLATFORM_REPO_URL}" && "${PLATFORM_REPO_URL}" != */ ]] && PLATFORM_REPO_URL="${PLATFORM_REPO_URL}/"

REPO_URLS=("${BASE_REPO_URL}${ARCH}/" "${PLATFORM_REPO_URL}${ARCH}/")

echo -e "${GREEN}[ OK ] Target Repositories detected.${NC}"

OUTDIR="build/python_rpms_${ARCH}"
mkdir -p "$OUTDIR"

# Required Python core packages (and sqlite3 for TizenClaw embeddings)
PACKAGES=(
    "python3-base"
    "python3"
    "libpython"
    "python3-sqlite"
)

echo -e "${CYAN}[DOWNLOAD] Fetching repository indices...${NC}"

INDEX_CONTENTS=()
for URL in "${REPO_URLS[@]}"; do
    if [ -n "$URL" ]; then
        echo -e "  Fetching index from ${URL}..."
        IDX=$(curl -sL "$URL" || echo "")
        INDEX_CONTENTS+=("$IDX")
    else
        INDEX_CONTENTS+=("")
    fi
done

DOWNLOADED_RPMS=()

for PKG in "${PACKAGES[@]}"; do
    echo -e "  -> Searching for ${PKG}..."
    MATCH=""
    MATCH_URL=""
    
    for i in "${!REPO_URLS[@]}"; do
        URL="${REPO_URLS[$i]}"
        IDX="${INDEX_CONTENTS[$i]}"
        
        if [ -z "$IDX" ]; then continue; fi
        
        # Search exact match
        TMP_MATCH=$(echo "$IDX" | grep -oE "href=\"${PKG}-[0-9][^\"]+\.rpm\"" | grep -v "debuginfo" | grep -v "devel" | cut -d'"' -f2 | sort -V | tail -n 1)
        
        if [ -z "$TMP_MATCH" ] && [ "$PKG" = "libpython" ]; then
            TMP_MATCH=$(echo "$IDX" | grep -oE "href=\"libpython3-[0-9][^\"]+\.rpm\"" | grep -v "debuginfo" | grep -v "devel" | cut -d'"' -f2 | sort -V | tail -n 1)
        fi
        
        if [ -n "$TMP_MATCH" ]; then
            MATCH="$TMP_MATCH"
            MATCH_URL="${URL}${MATCH}"
            break # Stop searching if found in first repo (priority base -> platform)
        fi
    done
    
    if [ -z "$MATCH" ]; then
         echo -e "${YELLOW}[WARN] Could not find ${PKG} in remote repos.${NC}"
         continue
    fi
    
    echo -e "  ${GREEN}[FOUND]${NC} ${MATCH}"
    
    # Download if not present
    if [ ! -f "${OUTDIR}/${MATCH}" ]; then
        echo -e "  Downloading..."
        curl -SL -o "${OUTDIR}/${MATCH}" "${MATCH_URL}"
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
