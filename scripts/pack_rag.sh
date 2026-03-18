#!/bin/bash
# pack_rag.sh — Compress rag/ data into zip archives for RPM packaging.
#
# Usage:
#   ./scripts/pack_rag.sh
#
# This script creates data/rag/web.zip from rag/web/ directory.
# Run this after generate_rag_web.py and before gbs build.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

RAG_SRC="${PROJECT_DIR}/rag/web"
RAG_DEST="${PROJECT_DIR}/data/rag"
ZIP_FILE="${RAG_DEST}/web.zip"

# Colors
GREEN='\033[0;32m'
CYAN='\033[0;36m'
RED='\033[0;31m'
NC='\033[0m'

log() { echo -e "${CYAN}[RAG-PACK]${NC} $*"; }
ok()  { echo -e "${GREEN}[  OK  ]${NC} $*"; }
fail() { echo -e "${RED}[ FAIL ]${NC} $*"; exit 1; }

# Check source exists
if [ ! -d "${RAG_SRC}" ]; then
  fail "RAG source not found: ${RAG_SRC}"
  echo "  Run: python3 scripts/generate_rag_web.py"
  exit 1
fi

# Create output dir
mkdir -p "${RAG_DEST}"

# Remove old zip
rm -f "${ZIP_FILE}"

# Create zip using Python (no external zip dependency needed)
log "Compressing ${RAG_SRC} → ${ZIP_FILE}..."
python3 -c "
import zipfile, os, sys

src = '${RAG_SRC}'
dst = '${ZIP_FILE}'
count = 0
with zipfile.ZipFile(dst, 'w', zipfile.ZIP_DEFLATED) as zf:
    for root, dirs, files in os.walk(src):
        for f in sorted(files):
            full = os.path.join(root, f)
            arc = os.path.relpath(full, src)
            zf.write(full, arc)
            count += 1
print(count)
"

ZIP_SIZE=$(du -h "${ZIP_FILE}" | cut -f1)
ok "Created: ${ZIP_FILE} (${ZIP_SIZE})"
