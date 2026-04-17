#!/usr/bin/env bash
set -euo pipefail

# Non-interactive installer smoke test for the host bundle path.
# Creates a local bundle (or reuses one passed via --bundle), installs it
# into an isolated temporary HOME, and verifies the expected runtime tree.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
BUNDLE_ARCHIVE=""
TMP_DIR=""

log()  { printf '[smoke] %s\n' "$*"; }
fail() { printf '[smoke][fail] %s\n' "$*" >&2; exit 1; }
cleanup() { [[ -n "${TMP_DIR}" ]] && rm -rf "${TMP_DIR}"; }

usage() {
  cat <<'EOF'
Host bundle installer smoke test

Usage:
  scripts/test_host_bundle_install.sh [--bundle <path>]

Options:
  --bundle <path>   Use an existing bundle archive instead of building one
  -h, --help        Show this help
EOF
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --bundle)
        [[ $# -lt 2 ]] && { echo "--bundle requires a value" >&2; exit 1; }
        BUNDLE_ARCHIVE="$(cd "$(dirname "$2")" && pwd)/$(basename "$2")"
        shift 2
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        echo "Unknown option: $1" >&2
        exit 1
        ;;
    esac
  done
}

# Returns 0 if the binary can be executed (exit codes other than 126/127 are
# accepted — the binary may legitimately exit non-zero without a live daemon).
assert_runnable() {
  local bin="$1"
  shift
  local rc=0
  "$bin" "$@" >/dev/null 2>&1 || rc=$?
  if [[ $rc -eq 126 || $rc -eq 127 ]]; then
    fail "Cannot execute: ${bin} (rc=${rc})"
  fi
}

main() {
  parse_args "$@"

  TMP_DIR="$(mktemp -d)"
  trap 'cleanup' EXIT
  local tmp_dir="${TMP_DIR}"

  if [[ -z "${BUNDLE_ARCHIVE}" ]]; then
    local dist_dir="${tmp_dir}/dist"
    mkdir -p "${dist_dir}"
    log "Building host bundle..."
    bash "${PROJECT_DIR}/scripts/create_host_release_bundle.sh" \
      --version "smoke-test" \
      --output-dir "${dist_dir}"
    BUNDLE_ARCHIVE="${dist_dir}/tizenclaw-host-bundle-smoke-test-linux-x86_64.tar.gz"
  fi

  [[ -f "${BUNDLE_ARCHIVE}" ]] \
    || fail "Bundle archive not found: ${BUNDLE_ARCHIVE}"

  local fake_home="${tmp_dir}/home"
  mkdir -p "${fake_home}"
  local install_root="${fake_home}/.tizenclaw"

  log "Installing bundle into ${install_root} ..."
  HOME="${fake_home}" \
  TIZENCLAW_INSTALL_ROOT="${install_root}" \
  TIZENCLAW_BASHRC_PATH="${fake_home}/.bashrc" \
  TIZENCLAW_SKIP_SERVICES="1" \
    bash "${PROJECT_DIR}/install.sh" \
      --asset-url "file://${BUNDLE_ARCHIVE}" \
      --skip-deps \
      --skip-setup

  log "Verifying installed binaries..."
  local required_bins=(
    "${install_root}/bin/tizenclaw"
    "${install_root}/bin/tizenclaw-cli"
    "${install_root}/bin/tizenclaw-tool-executor"
    "${install_root}/bin/tizenclaw-web-dashboard"
  )
  for b in "${required_bins[@]}"; do
    [[ -f "${b}" ]] || fail "Missing binary: ${b}"
    [[ -x "${b}" ]] || fail "Binary not executable: ${b}"
  done

  [[ -L "${install_root}/bin/tizenclaw-hostctl" || -f "${install_root}/bin/tizenclaw-hostctl" ]] \
    || fail "Missing tizenclaw-hostctl in bin/"

  log "Verifying bundle-manifest.json..."
  [[ -f "${install_root}/bundle-manifest.json" ]] \
    || fail "Missing bundle-manifest.json at install root"

  log "Verifying manage/deploy_host.sh..."
  [[ -f "${install_root}/manage/deploy_host.sh" ]] \
    || fail "Missing manage/deploy_host.sh"
  [[ -x "${install_root}/manage/deploy_host.sh" ]] \
    || fail "manage/deploy_host.sh is not executable"

  log "Verifying lib/ contains runtime libraries..."
  local lib_count
  lib_count="$(find "${install_root}/lib" -maxdepth 1 \( -name '*.so' -o -name '*.rlib' \) | wc -l)"
  [[ "${lib_count}" -gt 0 ]] \
    || fail "lib/ is empty — expected at least libtizenclaw.so or libtizenclaw.rlib"

  log "Verifying config/ is non-empty..."
  local config_count
  config_count="$(find "${install_root}/config" -maxdepth 1 -type f | wc -l)"
  [[ "${config_count}" -gt 0 ]] \
    || fail "config/ has no files — bundle must seed at least one config"

  log "Verifying data directories contain payload when source is non-empty..."
  _assert_nonempty_if_src_nonempty() {
    local label="$1"
    local src_dir="$2"
    local installed_dir="$3"
    if [[ -d "${src_dir}" ]] && [[ -n "$(find "${src_dir}" -mindepth 1 -maxdepth 2 -type f | head -1)" ]]; then
      local installed_count
      installed_count="$(find "${installed_dir}" -mindepth 1 -type f 2>/dev/null | wc -l)"
      [[ "${installed_count}" -gt 0 ]] \
        || fail "${label} is empty after install but source has content"
    fi
  }
  _assert_nonempty_if_src_nonempty "web/" "${PROJECT_DIR}/data/web" "${install_root}/web"
  _assert_nonempty_if_src_nonempty "docs/" "${PROJECT_DIR}/data/docs" "${install_root}/docs"
  _assert_nonempty_if_src_nonempty "embedded/" "${PROJECT_DIR}/tools/embedded" "${install_root}/embedded"

  log "Checking entrypoints are runnable..."
  assert_runnable "${install_root}/bin/tizenclaw-cli" --help
  assert_runnable "${install_root}/bin/tizenclaw-hostctl" --help

  log "Checking for stray processes from install root..."
  local stray=0
  for pname in tizenclaw tizenclaw-tool-executor tizenclaw-web-dashboard; do
    if pgrep -u "$(id -u)" -f "${install_root}/bin/${pname}" >/dev/null 2>&1; then
      printf '[smoke][fail] Stray process found: %s\n' "${pname}" >&2
      stray=1
    fi
  done
  [[ "${stray}" -eq 0 ]] || exit 1

  log "Installer smoke test PASSED"
}

main "$@"
