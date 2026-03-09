#!/bin/bash
# Build the Tizen Knowledge RAG database.
#
# Usage:
#   ./tools/build_knowledge_db.sh [options]
#
# Environment:
#   GEMINI_API_KEY   - Required API key
#   TIZEN_DOCS_PATH  - Path to tizen-docs repo
#                      (default: ~/samba/github/tizen-docs)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

TIZEN_DOCS="${TIZEN_DOCS_PATH:-$HOME/samba/github/tizen-docs}"
OUTPUT="${PROJECT_DIR}/data/rag/tizen_knowledge.db"

if [ -z "$GEMINI_API_KEY" ]; then
    echo "ERROR: Set GEMINI_API_KEY environment variable"
    exit 1
fi

python3 "$SCRIPT_DIR/build_knowledge_db.py" \
    --tizen-docs "$TIZEN_DOCS" \
    --output "$OUTPUT" \
    --api-key "$GEMINI_API_KEY" \
    --skip-guides \
    --skip-man \
    "$@"

echo ""
echo "Database built at: $OUTPUT"
echo "To deploy, copy to device:"
echo "  sdb push $OUTPUT /opt/usr/share/tizenclaw/rag/tizen_knowledge.db"
