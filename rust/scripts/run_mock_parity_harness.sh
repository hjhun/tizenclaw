#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUTPUT_DIR="${ROOT_DIR}/.tmp/mock-parity"

usage() {
  cat <<'EOF'
Run the reconstruction parity harness.

Usage:
  bash rust/scripts/run_mock_parity_harness.sh [--output-dir <dir>] [--json]
EOF
}

PRINT_JSON=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output-dir)
      [[ $# -lt 2 ]] && { echo "missing value for --output-dir" >&2; exit 2; }
      OUTPUT_DIR="$2"
      shift 2
      ;;
    --json)
      PRINT_JSON=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      exit 2
      ;;
  esac
done

mkdir -p "${OUTPUT_DIR}"

PYTHONPATH="${ROOT_DIR}:${PYTHONPATH:-}" python3 - <<'PY' "${ROOT_DIR}" "${OUTPUT_DIR}/python-runtime-summary.json"
from __future__ import annotations

import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
output = Path(sys.argv[2])

from src.runtime import build_runtime_summary

summary = build_runtime_summary(root)
output.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(output)
PY

REPORT_PATH="${OUTPUT_DIR}/parity-report.json"
python3 "${ROOT_DIR}/rust/scripts/run_mock_parity_diff.py" \
  --root "${ROOT_DIR}" \
  --output "${REPORT_PATH}" \
  --pretty

if [[ "${PRINT_JSON}" == true ]]; then
  cat "${REPORT_PATH}"
else
  python3 - <<'PY' "${REPORT_PATH}"
from __future__ import annotations

import json
import sys
from pathlib import Path

report = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
status = report["status"].upper()
errors = report["errors"]
print(f"[mock-parity] {status}: report={sys.argv[1]}")
if errors:
    for error in errors:
        print(f"[mock-parity] error: {error}")
else:
    summary = report["summary"]
    print(
        "[mock-parity] rust_members=%d rust_surfaces=%d runtime_modules=%d"
        % (
            len(summary["workspace_members"]),
            len(summary["rust_surfaces"]),
            len(summary["runtime_module_map"]),
        )
    )
PY
fi
