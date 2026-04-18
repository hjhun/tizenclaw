#!/usr/bin/env bash
set -euo pipefail

# Hermetic smoke test for the source-checkout installer path.
#
# Exercises both the explicit (--local-checkout) and implicit (auto-detected)
# install modes. All output is directed to isolated temporary directories so
# the real ~/.tizenclaw, ~/.bashrc, and Cargo build cache are never touched.
#
# The test also proves the checkout path never falls back to bundle-download
# by shadowing curl with a failing stub before either install run. Any bundle
# download attempt after that point causes an immediate test failure.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
TMP_DIR=""

log()  { printf '[checkout-smoke] %s\n' "$*"; }
fail() { printf '[checkout-smoke][fail] %s\n' "$*" >&2; exit 1; }

cleanup() {
  if [[ -n "${TMP_DIR}" && -d "${TMP_DIR}" ]]; then
    for root in "${TMP_DIR}/install-explicit" "${TMP_DIR}/install-implicit"; do
      local hostctl="${root}/bin/tizenclaw-hostctl"
      if [[ -x "${hostctl}" ]]; then
        TIZENCLAW_INSTALL_ROOT="${root}" \
          "${hostctl}" --stop >/dev/null 2>&1 || true
      fi
      pkill -u "$(id -u)" -f "${root}/bin/" >/dev/null 2>&1 || true
    done
    sleep 0.2
    rm -rf "${TMP_DIR}"
  fi
}

# Returns 0 if the binary can be executed. Exit codes other than 126/127 are
# accepted because a binary may legitimately exit non-zero without a live daemon.
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
  local line cmdline pname
  while IFS= read -r line; do
    [[ -n "${line}" ]] || continue
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

# Shadow curl with a failing stub placed at the front of PATH.
# Any bundle-download attempt after this point will fail the test immediately,
# proving that the checkout install path never falls back to release downloads.
shadow_curl_to_fail() {
  local fake_bin_dir="$1"
  mkdir -p "${fake_bin_dir}"
  cat > "${fake_bin_dir}/curl" <<'STUB'
#!/usr/bin/env bash
printf '[checkout-smoke] ERROR: curl called — bundle download is forbidden in checkout mode\n' >&2
exit 1
STUB
  chmod +x "${fake_bin_dir}/curl"
  export PATH="${fake_bin_dir}:${PATH}"
  log "curl shadowed with a failing stub (any bundle download will now abort the test)"
}

# Run install.sh with full environment isolation.
# Extra install.sh flags are passed as positional arguments after the three
# required positional parameters (install_root, fake_home, build_root).
run_install() {
  local install_root="$1"
  local fake_home="$2"
  local build_root="$3"
  shift 3

  HOME="${fake_home}" \
  TIZENCLAW_INSTALL_ROOT="${install_root}" \
  TIZENCLAW_BASHRC_PATH="${fake_home}/.bashrc" \
  TIZENCLAW_SKIP_SERVICES="1" \
    bash "${PROJECT_DIR}/install.sh" \
      --skip-deps \
      --skip-setup \
      "$@" \
      -- --no-restart --build-root "${build_root}"
}

verify_installed_tree() {
  local install_root="$1"
  local label="$2"

  log "[${label}] Verifying required binaries..."
  local b
  for b in \
      "${install_root}/bin/tizenclaw" \
      "${install_root}/bin/tizenclaw-cli" \
      "${install_root}/bin/tizenclaw-tool-executor" \
      "${install_root}/bin/tizenclaw-web-dashboard"; do
    [[ -f "${b}" ]] || fail "[${label}] Missing: ${b}"
    [[ -x "${b}" ]] || fail "[${label}] Not executable: ${b}"
  done

  [[ -L "${install_root}/bin/tizenclaw-hostctl" \
     || -f "${install_root}/bin/tizenclaw-hostctl" ]] \
    || fail "[${label}] Missing ${install_root}/bin/tizenclaw-hostctl"

  log "[${label}] Verifying config/ is seeded..."
  local config_count
  config_count="$(find "${install_root}/config" -maxdepth 1 -type f 2>/dev/null | wc -l)"
  [[ "${config_count}" -gt 0 ]] \
    || fail "[${label}] config/ contains no files after install"

  log "[${label}] Verifying optional data dirs (docs/, web/, embedded/)..."
  local src_path installed_path count
  for mapping in \
      "${PROJECT_DIR}/data/docs:${install_root}/docs" \
      "${PROJECT_DIR}/data/web:${install_root}/web" \
      "${PROJECT_DIR}/tools/embedded:${install_root}/embedded"; do
    src_path="${mapping%%:*}"
    installed_path="${mapping##*:}"
    if [[ -d "${src_path}" ]] \
       && [[ -n "$(find "${src_path}" -maxdepth 2 -type f 2>/dev/null | head -1)" ]]; then
      count="$(find "${installed_path}" -mindepth 1 -type f 2>/dev/null | wc -l)"
      [[ "${count}" -gt 0 ]] \
        || fail "[${label}] ${installed_path} is empty after install but source has files in ${src_path}"
    fi
  done

  log "[${label}] Checking tizenclaw-cli --help is runnable..."
  assert_runnable "${install_root}/bin/tizenclaw-cli" --help

  log "[${label}] Checking tizenclaw-hostctl --help is runnable..."
  assert_runnable "${install_root}/bin/tizenclaw-hostctl" --help
}

main() {
  TMP_DIR="$(mktemp -d)"
  trap 'cleanup' EXIT

  local build_root="${TMP_DIR}/build"
  local fake_home_explicit="${TMP_DIR}/home-explicit"
  local install_root_explicit="${fake_home_explicit}/.tizenclaw"
  local fake_home_implicit="${TMP_DIR}/home-implicit"
  local install_root_implicit="${fake_home_implicit}/.tizenclaw"
  local fake_curl_dir="${TMP_DIR}/fake-bin"

  mkdir -p "${fake_home_explicit}" "${fake_home_implicit}" "${build_root}"

  # Shadow curl before any install run. With --skip-deps, install.sh never
  # calls apt-get or rustup (which are the only legitimate curl consumers).
  # If the checkout path accidentally attempts a bundle download, the failing
  # stub will abort the test immediately.
  shadow_curl_to_fail "${fake_curl_dir}"

  # ── Test 1: explicit --local-checkout ─────────────────────────────────────
  log "=== Test 1: explicit --local-checkout ==="
  run_install \
    "${install_root_explicit}" \
    "${fake_home_explicit}" \
    "${build_root}" \
    --local-checkout \
    || fail "install.sh --local-checkout exited non-zero"

  verify_installed_tree "${install_root_explicit}" "explicit"
  log "Test 1 PASSED"

  # ── Test 2: implicit auto-detection ───────────────────────────────────────
  # install.sh is invoked via its absolute path (bash <path>), so SCRIPT_DIR
  # inside install.sh resolves to PROJECT_DIR. auto_select_local_checkout then
  # finds deploy_host.sh and .git there and activates --local-checkout without
  # the caller passing the flag. The second run reuses cached Cargo artifacts
  # from build_root so it completes quickly.
  log "=== Test 2: implicit auto-detection (no --local-checkout flag) ==="
  run_install \
    "${install_root_implicit}" \
    "${fake_home_implicit}" \
    "${build_root}" \
    || fail "install.sh (implicit auto-detection) exited non-zero"

  verify_installed_tree "${install_root_implicit}" "implicit"
  log "Test 2 PASSED"

  # ── Stray process check ───────────────────────────────────────────────────
  log "Verifying no stray daemon processes remain..."
  if any_stray_for_root "${install_root_explicit}"; then
    fail "Stray processes remain for explicit install root (${install_root_explicit})"
  fi
  if any_stray_for_root "${install_root_implicit}"; then
    fail "Stray processes remain for implicit install root (${install_root_implicit})"
  fi

  log "Source-checkout installer smoke test PASSED"
}

main "$@"
