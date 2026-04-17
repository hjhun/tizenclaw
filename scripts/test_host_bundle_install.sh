#!/usr/bin/env bash
set -euo pipefail

# Non-interactive installer smoke test for the host bundle path.
# Creates a local bundle (or reuses one passed via --bundle), installs it
# into an isolated temporary HOME, verifies the installed tree, and
# exercises tizenclaw-hostctl lifecycle (restart-only -> status -> stop)
# against the installed bundle without any repository-checkout access.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
BUNDLE_ARCHIVE=""
TMP_DIR=""

log()  { printf '[smoke] %s\n' "$*"; }
fail() { printf '[smoke][fail] %s\n' "$*" >&2; exit 1; }

cleanup() {
  if [[ -n "${TMP_DIR}" && -d "${TMP_DIR}" ]]; then
    # Best-effort: always try to stop stray services before removing the tree.
    local hostctl="${TMP_DIR}/home/.tizenclaw/bin/tizenclaw-hostctl"
    if [[ -x "${hostctl}" ]]; then
      HOME="${TMP_DIR}/home" \
      TIZENCLAW_INSTALL_ROOT="${TMP_DIR}/home/.tizenclaw" \
        "${hostctl}" --stop >/dev/null 2>&1 || true
    fi
    rm -rf "${TMP_DIR}"
  fi
}

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

any_stray_for_root() {
  local root="$1"
  local current_uid
  current_uid="$(id -u)"
  # Use bash-level cmdline matching so that control scripts such as
  # tizenclaw-hostctl are not falsely treated as daemon processes.  pgrep ERE
  # suffix patterns like ([[:space:]]|$) behave inconsistently across procps
  # versions; reading /proc/<pid>/cmdline in bash is authoritative.
  # A process is stray only when its cmdline STARTS WITH the daemon binary
  # path — i.e. the daemon exec'd directly, not an interpreter running the
  # hostctl script with the binary path appearing later as an argument.
  local line pid cmdline pname
  while IFS= read -r line; do
    [[ -n "${line}" ]] || continue
    pid="${line%% *}"
    cmdline="${line#* }"
    for pname in tizenclaw tizenclaw-tool-executor tizenclaw-web-dashboard; do
      if [[ "${cmdline}" == "${root}/bin/${pname}" ]] \
         || [[ "${cmdline}" == "${root}/bin/${pname} "* ]]; then
        return 0
      fi
    done
  done < <(pgrep -u "${current_uid}" -af "${root}/bin/" 2>/dev/null || true)
  return 1
}

main() {
  parse_args "$@"

  TMP_DIR="$(mktemp -d)"
  trap 'cleanup' EXIT

  if [[ -z "${BUNDLE_ARCHIVE}" ]]; then
    local dist_dir="${TMP_DIR}/dist"
    mkdir -p "${dist_dir}"
    log "Building host bundle..."
    bash "${PROJECT_DIR}/scripts/create_host_release_bundle.sh" \
      --version "smoke-test" \
      --output-dir "${dist_dir}"
    BUNDLE_ARCHIVE="${dist_dir}/tizenclaw-host-bundle-smoke-test-linux-x86_64.tar.gz"
  fi

  [[ -f "${BUNDLE_ARCHIVE}" ]] \
    || fail "Bundle archive not found: ${BUNDLE_ARCHIVE}"

  log "Inspecting bundle archive contents..."
  local bundle_listing
  bundle_listing="$(tar -tzf "${BUNDLE_ARCHIVE}")"
  if grep -Fq 'manage/deploy_host.sh' <<< "${bundle_listing}"; then
    fail "Bundle still contains manage/deploy_host.sh — the source deploy script must not be packaged"
  fi
  grep -Fq 'manage/tizenclaw-hostctl.sh' <<< "${bundle_listing}" \
    || fail "Bundle is missing manage/tizenclaw-hostctl.sh"

  local fake_home="${TMP_DIR}/home"
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

  log "Verifying manage/tizenclaw-hostctl.sh..."
  local managed_ctl="${install_root}/manage/tizenclaw-hostctl.sh"
  [[ -f "${managed_ctl}" ]] \
    || fail "Missing ${managed_ctl}"
  [[ -x "${managed_ctl}" ]] \
    || fail "${managed_ctl} is not executable"

  if [[ -e "${install_root}/manage/deploy_host.sh" ]]; then
    fail "Installed tree must not contain manage/deploy_host.sh (found $(ls -l "${install_root}/manage/deploy_host.sh"))"
  fi

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

  log "Verifying data directories contain payload when bundle ships them..."
  # Compare against the bundle archive, not the source checkout, so this check
  # does not create a back-channel through which repo-path dependencies in the
  # installed scripts could slip past the "no checkout available" criterion.
  for _dir_prefix in "web/" "docs/" "embedded/"; do
    if grep -qF "${_dir_prefix}" <<< "${bundle_listing}"; then
      local _dir_name="${_dir_prefix%/}"
      local _installed_count
      _installed_count="$(find "${install_root}/${_dir_name}" -mindepth 1 -type f 2>/dev/null | wc -l)"
      [[ "${_installed_count}" -gt 0 ]] \
        || fail "${_dir_name}/ is empty after install but bundle contained files"
    fi
  done

  log "Checking tizenclaw-cli --help is runnable..."
  assert_runnable "${install_root}/bin/tizenclaw-cli" --help

  # Build a sanitized environment for hostctl runs. Masking HOME, PATH, and
  # cwd to the installed tree ensures hostctl cannot accidentally reach any
  # source checkout (no git, no cargo, no cwd-relative repo paths).
  local hostctl="${install_root}/bin/tizenclaw-hostctl"
  run_hostctl() {
    (
      cd "${fake_home}"
      env -i \
        HOME="${fake_home}" \
        PATH="${install_root}/bin:/usr/bin:/bin" \
        TIZENCLAW_INSTALL_ROOT="${install_root}" \
        "${hostctl}" "$@"
    )
  }

  log "Checking tizenclaw-hostctl --help works in the isolated bundle..."
  run_hostctl --help >/dev/null \
    || fail "tizenclaw-hostctl --help failed"

  log "Asserting source-only flags fail fast with a clear error..."
  local source_only_flags=(--release --debug -d --build-only -b --no-restart \
    --test --remove --dry-run --devel --build-root --llm-config)
  for flag in "${source_only_flags[@]}"; do
    local stderr_capture
    stderr_capture="$(run_hostctl "${flag}" 2>&1 >/dev/null || true)"
    local rc=0
    run_hostctl "${flag}" >/dev/null 2>&1 || rc=$?
    if [[ "${rc}" -eq 0 ]]; then
      fail "hostctl ${flag} must fail, but exited 0"
    fi
    if ! grep -Fq "requires a source checkout" <<< "${stderr_capture}"; then
      fail "hostctl ${flag} rejection message missing 'requires a source checkout': ${stderr_capture}"
    fi
  done

  log "Asserting hostctl does not depend on repo-relative access..."
  # Scan the script content for obviously forbidden operations. We allow the
  # rejection messages to mention './deploy_host.sh' for user guidance.
  if grep -Eq '^[^#]*\bcargo\b' "${managed_ctl}"; then
    fail "hostctl script contains a cargo invocation"
  fi
  if grep -Eq '^[^#]*\bgit\b' "${managed_ctl}"; then
    fail "hostctl script contains a git invocation"
  fi

  log "Starting installed daemon via tizenclaw-hostctl --restart-only..."
  if ! run_hostctl --restart-only; then
    fail "tizenclaw-hostctl --restart-only failed"
  fi

  log "Waiting for daemon PID to stabilize..."
  local pid_file="${install_root}/run/tizenclaw-host.pid"
  local tool_pid_file="${install_root}/run/tizenclaw-tool-executor-host.pid"
  local deadline=$((SECONDS + 5))
  while [[ "${SECONDS}" -lt "${deadline}" ]]; do
    if [[ -f "${pid_file}" && -f "${tool_pid_file}" ]]; then
      local pid tool_pid
      pid="$(cat "${pid_file}" 2>/dev/null || true)"
      tool_pid="$(cat "${tool_pid_file}" 2>/dev/null || true)"
      if [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null \
        && [[ -n "${tool_pid}" ]] && kill -0 "${tool_pid}" 2>/dev/null; then
        break
      fi
    fi
    sleep 0.2
  done
  [[ -f "${pid_file}" ]] || fail "Daemon PID file not written at ${pid_file}"
  local pid
  pid="$(cat "${pid_file}" 2>/dev/null || true)"
  [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null \
    || fail "Daemon process ${pid:-(empty)} is not alive after --restart-only"

  log "Running tizenclaw-hostctl --status against the live daemon..."
  local status_output
  status_output="$(run_hostctl --status 2>&1 || true)"
  if ! grep -Fq "${PKG_NAME:-tizenclaw} is running" <<< "${status_output}" \
    && ! grep -Fq "tizenclaw is running" <<< "${status_output}"; then
    printf '%s\n' "${status_output}" >&2
    fail "--status did not confirm a running tizenclaw process"
  fi

  log "Checking tizenclaw-hostctl --log stays alive against the installed log..."
  mkdir -p "${install_root}/logs"
  touch "${install_root}/logs/tizenclaw.log"
  local log_check_pid=""
  run_hostctl --log >/dev/null 2>&1 &
  log_check_pid=$!
  sleep 0.5
  if ! kill -0 "${log_check_pid}" 2>/dev/null; then
    fail "tizenclaw-hostctl --log exited prematurely — expected tail -f to remain running"
  fi
  kill "${log_check_pid}" 2>/dev/null || true
  # The subshell wrapper may have spawned a child bash running tizenclaw-hostctl
  # that is not a direct child of log_check_pid after exec; kill it explicitly.
  pkill -u "$(id -u)" -f "${install_root}/bin/tizenclaw-hostctl" 2>/dev/null || true
  wait "${log_check_pid}" 2>/dev/null || true
  # Reap any remaining hostctl --log children before the stray-process check.
  # SIGTERM via pkill is asynchronous; poll until the process is gone, then
  # escalate to SIGKILL if it lingers.
  local _log_kill_deadline=$((SECONDS + 5))
  while pgrep -u "$(id -u)" -f "${install_root}/bin/tizenclaw-hostctl" >/dev/null 2>&1; do
    if [[ "${SECONDS}" -ge "${_log_kill_deadline}" ]]; then
      pkill -9 -u "$(id -u)" -f "${install_root}/bin/tizenclaw-hostctl" 2>/dev/null || true
      sleep 0.2
      break
    fi
    sleep 0.1
  done

  log "Stopping installed daemon via tizenclaw-hostctl --stop..."
  run_hostctl --stop \
    || fail "tizenclaw-hostctl --stop failed"

  log "Verifying daemon processes have exited..."
  local wait_deadline=$((SECONDS + 10))
  while [[ "${SECONDS}" -lt "${wait_deadline}" ]]; do
    if ! any_stray_for_root "${install_root}"; then
      break
    fi
    sleep 0.2
  done
  if any_stray_for_root "${install_root}"; then
    printf '[smoke][fail] Stray processes remain for %s\n' "${install_root}" >&2
    pgrep -u "$(id -u)" -af "${install_root}/bin/" >&2 || true
    exit 1
  fi

  [[ ! -f "${pid_file}" ]] \
    || fail "Daemon PID file should be removed after --stop: ${pid_file}"
  [[ ! -f "${tool_pid_file}" ]] \
    || fail "Tool-executor PID file should be removed after --stop: ${tool_pid_file}"

  log "Installer smoke test PASSED"
}

main "$@"
