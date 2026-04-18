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
INSTALL_ROOT_EXPLICIT=""
INSTALL_ROOT_IMPLICIT=""
INSTALL_ROOT_WORKTREE=""
HOOK_BACKUP_PATH=""   # set when an existing pre-commit hook is stashed

log()  { printf '[checkout-smoke] %s\n' "$*"; }
fail() { printf '[checkout-smoke][fail] %s\n' "$*" >&2; exit 1; }

cleanup() {
  if [[ -n "${TMP_DIR}" && -d "${TMP_DIR}" ]]; then
    # Remove any temporary worktrees before wiping TMP_DIR so git bookkeeping
    # stays consistent even if the test failed mid-flight.
    local wt
    for wt in "${TMP_DIR}/worktree" "${TMP_DIR}/worktree-hooks"; do
      if [[ -d "${wt}" ]]; then
        git -C "${PROJECT_DIR}" worktree remove --force "${wt}" 2>/dev/null || true
      fi
    done

    # Restore any pre-commit hook that Test 4 temporarily removed.
    local common_dir hook_file
    common_dir="$(git -C "${PROJECT_DIR}" rev-parse --git-common-dir 2>/dev/null || true)"
    if [[ -n "${common_dir}" ]]; then
      [[ "${common_dir}" == /* ]] || common_dir="${PROJECT_DIR}/${common_dir}"
      hook_file="${common_dir}/hooks/pre-commit"
      # Remove any test-installed hook first so we start clean.
      rm -f "${hook_file}"
      if [[ -n "${HOOK_BACKUP_PATH}" && -f "${HOOK_BACKUP_PATH}" ]]; then
        cp -p "${HOOK_BACKUP_PATH}" "${hook_file}"
      fi
    fi

    local root
    for root in "${INSTALL_ROOT_EXPLICIT}" "${INSTALL_ROOT_IMPLICIT}" "${INSTALL_ROOT_WORKTREE}"; do
      [[ -n "${root}" ]] || continue
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

# Verifies a binary executes cleanly. Exit codes 0-125 are accepted (a binary
# may legitimately exit non-zero without a live daemon). Exit codes 126 and 127
# mean the kernel could not start the binary. Exit codes >= 128 are shell-mapped
# signals (SIGSEGV=139, SIGABRT=134, etc.) and indicate a crash.
assert_runnable() {
  local bin="$1"
  shift
  local rc=0
  "$bin" "$@" >/dev/null 2>&1 || rc=$?
  if [[ $rc -eq 126 || $rc -eq 127 ]]; then
    fail "Cannot execute: ${bin} (rc=${rc})"
  fi
  if [[ $rc -gt 127 ]]; then
    fail "Binary terminated by signal: ${bin} (rc=${rc})"
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

# Run install.sh found in <script_dir> with full environment isolation.
# Extra install.sh flags are passed as positional arguments after the four
# required positional parameters (script_dir, install_root, fake_home, build_root).
run_install_from_dir() {
  local script_dir="$1"
  local install_root="$2"
  local fake_home="$3"
  local build_root="$4"
  shift 4

  # Preserve real Rust toolchain locations so rustup shims remain
  # functional after HOME is redirected to the isolated temp directory.
  local real_cargo_home="${CARGO_HOME:-${HOME}/.cargo}"
  local real_rustup_home="${RUSTUP_HOME:-${HOME}/.rustup}"

  HOME="${fake_home}" \
  CARGO_HOME="${real_cargo_home}" \
  RUSTUP_HOME="${real_rustup_home}" \
  TIZENCLAW_INSTALL_ROOT="${install_root}" \
  TIZENCLAW_BASHRC_PATH="${fake_home}/.bashrc" \
  TIZENCLAW_SKIP_SERVICES="1" \
  TIZENCLAW_NO_NETWORK_FALLBACK="1" \
    bash "${script_dir}/install.sh" \
      --skip-deps \
      --skip-setup \
      "$@" \
      -- --no-restart --build-root "${build_root}"
}

# Convenience wrapper that always runs install.sh from the main PROJECT_DIR.
run_install() {
  local install_root="$1"
  local fake_home="$2"
  local build_root="$3"
  shift 3
  run_install_from_dir "${PROJECT_DIR}" "${install_root}" "${fake_home}" "${build_root}" "$@"
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
  INSTALL_ROOT_EXPLICIT="${fake_home_explicit}/.tizenclaw"
  local fake_home_implicit="${TMP_DIR}/home-implicit"
  INSTALL_ROOT_IMPLICIT="${fake_home_implicit}/.tizenclaw"
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
    "${INSTALL_ROOT_EXPLICIT}" \
    "${fake_home_explicit}" \
    "${build_root}" \
    --local-checkout \
    || fail "install.sh --local-checkout exited non-zero"

  verify_installed_tree "${INSTALL_ROOT_EXPLICIT}" "explicit"
  log "Test 1 PASSED"

  # ── Test 2: implicit auto-detection ───────────────────────────────────────
  # install.sh is invoked via its absolute path (bash <path>), so SCRIPT_DIR
  # inside install.sh resolves to PROJECT_DIR. auto_select_local_checkout then
  # finds deploy_host.sh and .git there and activates --local-checkout without
  # the caller passing the flag. The second run reuses cached Cargo artifacts
  # from build_root so it completes quickly.
  log "=== Test 2: implicit auto-detection (no --local-checkout flag) ==="
  run_install \
    "${INSTALL_ROOT_IMPLICIT}" \
    "${fake_home_implicit}" \
    "${build_root}" \
    || fail "install.sh (implicit auto-detection) exited non-zero"

  verify_installed_tree "${INSTALL_ROOT_IMPLICIT}" "implicit"
  log "Test 2 PASSED"

  # ── Test 3: git worktree checkout ─────────────────────────────────────────
  # Creates a temporary worktree from the current repo and exercises both
  # explicit --local-checkout and implicit auto-detection from that worktree.
  # In a worktree, .git is a file (not a directory), so this test validates
  # that all checkout-detection paths use Git plumbing rather than -d .git.
  log "=== Test 3: git worktree checkout ==="
  local worktree_dir="${TMP_DIR}/worktree"
  local fake_home_worktree="${TMP_DIR}/home-worktree"
  local fake_home_worktree_implicit="${TMP_DIR}/home-worktree-implicit"
  INSTALL_ROOT_WORKTREE="${fake_home_worktree}/.tizenclaw"
  local install_root_worktree_implicit="${fake_home_worktree_implicit}/.tizenclaw"
  mkdir -p "${fake_home_worktree}" "${fake_home_worktree_implicit}"

  git -C "${PROJECT_DIR}" worktree add --detach "${worktree_dir}" \
    || fail "git worktree add failed — cannot create worktree for Test 3"

  log "[worktree] .git entry type in worktree: $(stat -c '%F' "${worktree_dir}/.git" 2>/dev/null || echo '(absent)')"

  log "[worktree] Test 3a: explicit --local-checkout from worktree"
  run_install_from_dir \
    "${worktree_dir}" \
    "${INSTALL_ROOT_WORKTREE}" \
    "${fake_home_worktree}" \
    "${build_root}" \
    --local-checkout \
    || fail "install.sh --local-checkout from worktree exited non-zero"
  verify_installed_tree "${INSTALL_ROOT_WORKTREE}" "worktree-explicit"
  log "[worktree] Test 3a PASSED"

  log "[worktree] Test 3b: implicit auto-detection from worktree"
  run_install_from_dir \
    "${worktree_dir}" \
    "${install_root_worktree_implicit}" \
    "${fake_home_worktree_implicit}" \
    "${build_root}" \
    || fail "install.sh (implicit) from worktree exited non-zero"
  verify_installed_tree "${install_root_worktree_implicit}" "worktree-implicit"
  log "[worktree] Test 3b PASSED"

  git -C "${PROJECT_DIR}" worktree remove --force "${worktree_dir}" \
    || true  # cleanup handles this too; tolerate double-remove
  log "Test 3 PASSED"

  # ── Test 4: hook setup scripts from a worktree ────────────────────────────
  # Verifies that both scripts/setup-hooks.sh and scripts/setup_hooks.sh
  # succeed when invoked from a git worktree checkout (.git is a file, not a
  # directory). Both scripts must use --git-common-dir so the hook lands in
  # the correct shared hooks directory.
  log "=== Test 4: hook setup scripts from worktree ==="
  local hooks_wt_dir="${TMP_DIR}/worktree-hooks"
  git -C "${PROJECT_DIR}" worktree add --detach "${hooks_wt_dir}" \
    || fail "git worktree add failed — cannot create worktree for Test 4"

  log "[hooks] .git entry type in hooks worktree: $(stat -c '%F' "${hooks_wt_dir}/.git" 2>/dev/null || echo '(absent)')"

  # Locate the shared hooks directory and stash any pre-existing hook so we
  # can verify a fresh install and restore the original state on cleanup.
  local common_dir_hooks hook_file_path
  common_dir_hooks="$(git -C "${PROJECT_DIR}" rev-parse --git-common-dir)"
  [[ "${common_dir_hooks}" == /* ]] || common_dir_hooks="${PROJECT_DIR}/${common_dir_hooks}"
  hook_file_path="${common_dir_hooks}/hooks/pre-commit"
  if [[ -e "${hook_file_path}" ]]; then
    HOOK_BACKUP_PATH="${TMP_DIR}/pre-commit.bak"
    cp -p "${hook_file_path}" "${HOOK_BACKUP_PATH}"
    rm -f "${hook_file_path}"
  fi

  log "[hooks] Test 4a: scripts/setup-hooks.sh from worktree"
  bash "${hooks_wt_dir}/scripts/setup-hooks.sh" \
    || fail "setup-hooks.sh from worktree exited non-zero"
  [[ -f "${hook_file_path}" ]] \
    || fail "[hooks] pre-commit hook not installed by setup-hooks.sh (expected: ${hook_file_path})"
  log "[hooks] Test 4a PASSED"

  rm -f "${hook_file_path}"

  log "[hooks] Test 4b: scripts/setup_hooks.sh from worktree"
  bash "${hooks_wt_dir}/scripts/setup_hooks.sh" \
    || fail "setup_hooks.sh from worktree exited non-zero"
  [[ -f "${hook_file_path}" ]] \
    || fail "[hooks] pre-commit hook not installed by setup_hooks.sh (expected: ${hook_file_path})"
  log "[hooks] Test 4b PASSED"

  git -C "${PROJECT_DIR}" worktree remove --force "${hooks_wt_dir}" \
    || true
  log "Test 4 PASSED"

  # ── Stray process check ───────────────────────────────────────────────────
  log "Verifying no stray daemon processes remain..."
  if any_stray_for_root "${INSTALL_ROOT_EXPLICIT}"; then
    fail "Stray processes remain for explicit install root (${INSTALL_ROOT_EXPLICIT})"
  fi
  if any_stray_for_root "${INSTALL_ROOT_IMPLICIT}"; then
    fail "Stray processes remain for implicit install root (${INSTALL_ROOT_IMPLICIT})"
  fi
  if any_stray_for_root "${INSTALL_ROOT_WORKTREE}"; then
    fail "Stray processes remain for worktree install root (${INSTALL_ROOT_WORKTREE})"
  fi
  if any_stray_for_root "${install_root_worktree_implicit}"; then
    fail "Stray processes remain for worktree-implicit install root (${install_root_worktree_implicit})"
  fi

  log "Source-checkout installer smoke test PASSED"
}

main "$@"
