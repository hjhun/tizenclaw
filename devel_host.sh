#!/bin/bash
# Preferred host-development entry point.
# Keeps the existing deploy_host.sh implementation for compatibility.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
export TIZENCLAW_HOST_ENTRYPOINT_NAME="$(basename "$0")"
exec "${SCRIPT_DIR}/deploy_host.sh" "$@"
