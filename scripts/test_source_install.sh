#!/usr/bin/env bash
set -euo pipefail

# Hermetic smoke test for the --source-install installer path.
#
# Uses a local bare-repo file:// remote so no network access is needed.
# All output is directed to isolated temporary directories so the real
# ~/.tizenclaw, ~/.bashrc, and Cargo build cache are never touched.
#
# Tests:
#   1. Fresh clone + build/install from a local bare repo.
#   2. Advance the local remote with an empty commit; re-run against the
#      same clean checkout to confirm safe fast-forward update works.
#   3. Make the checkout dirty; assert the installer fails without altering
#      the dirty file or resetting the Git working tree.
#   4. Create a local commit not present on origin; assert the installer
#      fails and does not discard the local commit.
#
# curl is shadowed with a failing stub before the first install run.
# Any accidental bundle-download attempt causes immediate test failure,
# proving the source-install path never falls back to release downloads.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
TMP_DIR=""
INSTALL_ROOT=""

log()  { printf '[source-install-smoke] %s\n' "$*"; }
fail() { printf '[source-install-smoke][fail] %s\n' "$*" >&2; exit 1; }

cleanup() {
  if [[ -n "${TMP_DIR}" && -d "${TMP_DIR}" ]]; then
    if [[ -n "${INSTALL_ROOT}" ]]; then
      local hostctl="${INSTALL_ROOT}/bin/tizenclaw-hostctl"
      if [[ -x "${hostctl}" ]]; then
        TIZENCLAW_INSTALL_ROOT="${INSTALL_ROOT}" \
          "${hostctl}" --stop >/dev/null 2>&1 || true
      fi
      pkill -u "$(id -u)" -f "${INSTALL_ROOT}/bin/" >/dev/null 2>&1 || true
    fi
    sleep 0.2
    rm -rf "${TMP_DIR}"
  fi
}

# Verifies a binary executes without a kernel-level failure.
# Exit codes 0-125 are accepted; 126/127 mean the binary could not start;
# codes >= 128 indicate a signal (crash).
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

# Place a failing curl stub at the front of PATH. With --skip-deps no
# legitimate curl call should occur; any bundle-download attempt aborts.
shadow_curl_to_fail() {
  local fake_bin_dir="$1"
  mkdir -p "${fake_bin_dir}"
  cat > "${fake_bin_dir}/curl" <<'STUB'
#!/usr/bin/env bash
printf '[source-install-smoke] ERROR: curl invoked — network/bundle download is forbidden in source-install mode\n' >&2
exit 1
STUB
  chmod +x "${fake_bin_dir}/curl"
  export PATH="${fake_bin_dir}:${PATH}"
  log "curl shadowed with a failing stub"
}

# Create a local bare repository seeded with the current project's HEAD
# as a branch named <ref>.  Using git push avoids shallow-clone depth
# restrictions that would otherwise block clone-from-clone operations.
make_bare_repo() {
  local bare_dir="$1"
  local ref="$2"

  log "Creating local bare repo at ${bare_dir}..."
  git init --bare "${bare_dir}"
  # CI fetches the repository by commit with --depth 1.  The smoke remote is
  # intentionally local and disposable, so allow it to accept that shallow
  # source ref instead of requiring network access to unshallow first.
  git -C "${bare_dir}" config receive.shallowUpdate true
  git -C "${PROJECT_DIR}" push "${bare_dir}" "HEAD:refs/heads/${ref}"
}

# Clone the bare repo to a temporary working copy, add an empty commit,
# and push it back.  This simulates an upstream advance without requiring
# any source-file modification (so rebuilds remain incremental).
advance_bare_repo() {
  local bare_dir="$1"
  local ref="$2"
  local work_dir="$3"

  log "Advancing bare repo with an empty commit..."
  git clone "${bare_dir}" "${work_dir}"
  git -C "${work_dir}" checkout "${ref}"
  git -C "${work_dir}" \
    -c user.email="smoke@test.local" \
    -c user.name="Smoke Test" \
    commit --allow-empty \
    -m "smoke: advance remote for re-install test"
  git -C "${work_dir}" push origin "${ref}"
}

# Run install.sh in source-install mode with full environment isolation.
run_source_install() {
  local install_root="$1"
  local fake_home="$2"
  local build_root="$3"
  local repo_url="$4"
  local ref="$5"
  local source_dir="$6"

  # Preserve the real Rust toolchain locations so rustup shims remain
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
    bash "${PROJECT_DIR}/install.sh" \
      --source-install \
      --repo "${repo_url}" \
      --ref "${ref}" \
      --dir "${source_dir}" \
      --skip-deps \
      --skip-setup \
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
        || fail "[${label}] ${installed_path} is empty but source has files in ${src_path}"
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

  local fake_home="${TMP_DIR}/home"
  local build_root="${TMP_DIR}/build"
  local bare_repo="${TMP_DIR}/bare.git"
  local source_dir="${TMP_DIR}/source"
  local fake_curl_dir="${TMP_DIR}/fake-bin"
  # Use a fixed internal ref name so the test is independent of the
  # project's current branch state (handles detached HEAD in CI too).
  local test_ref="smoke-test"

  INSTALL_ROOT="${fake_home}/.tizenclaw"
  mkdir -p "${fake_home}" "${build_root}"

  make_bare_repo "${bare_repo}" "${test_ref}"
  local bare_url="file://${bare_repo}"

  # Shadow curl before any install run. With --skip-deps the only legitimate
  # curl callers (apt-get, rustup) are skipped. Any bundle-download attempt
  # after this point aborts the test immediately.
  shadow_curl_to_fail "${fake_curl_dir}"

  # ── Test 1: Fresh clone + build + install ──────────────────────────────────
  log "=== Test 1: Fresh clone from local bare repo ==="
  run_source_install \
    "${INSTALL_ROOT}" \
    "${fake_home}" \
    "${build_root}" \
    "${bare_url}" \
    "${test_ref}" \
    "${source_dir}" \
    || fail "Test 1: install.sh --source-install exited non-zero"

  verify_installed_tree "${INSTALL_ROOT}" "fresh-install"
  log "Test 1 PASSED"

  # ── Test 2: Advance remote, re-run against clean checkout ─────────────────
  log "=== Test 2: Re-install after remote advances (clean checkout) ==="
  advance_bare_repo "${bare_repo}" "${test_ref}" "${TMP_DIR}/advance-work"

  run_source_install \
    "${INSTALL_ROOT}" \
    "${fake_home}" \
    "${build_root}" \
    "${bare_url}" \
    "${test_ref}" \
    "${source_dir}" \
    || fail "Test 2: install.sh --source-install (re-install) exited non-zero"

  verify_installed_tree "${INSTALL_ROOT}" "re-install"
  log "Test 2 PASSED"

  # ── Test 3: Dirty checkout must cause install failure ─────────────────────
  log "=== Test 3: Dirty checkout must cause install failure ==="

  local dirty_file="${source_dir}/SMOKE_DIRTY_FILE"
  printf 'dirty content added by source-install smoke test\n' > "${dirty_file}"
  local dirty_content
  dirty_content="$(cat "${dirty_file}")"

  # Capture stderr to verify the error message is actionable.
  local err_output
  local install_rc=0
  err_output="$(
    HOME="${fake_home}" \
    CARGO_HOME="${CARGO_HOME:-${HOME}/.cargo}" \
    RUSTUP_HOME="${RUSTUP_HOME:-${HOME}/.rustup}" \
    TIZENCLAW_INSTALL_ROOT="${INSTALL_ROOT}" \
    TIZENCLAW_BASHRC_PATH="${fake_home}/.bashrc" \
    TIZENCLAW_SKIP_SERVICES="1" \
    TIZENCLAW_NO_NETWORK_FALLBACK="1" \
      bash "${PROJECT_DIR}/install.sh" \
        --source-install \
        --repo "${bare_url}" \
        --ref "${test_ref}" \
        --dir "${source_dir}" \
        --skip-deps \
        --skip-setup \
        -- --no-restart --build-root "${build_root}" \
      2>&1 >/dev/null
  )" || install_rc=$?

  if [[ "${install_rc}" -eq 0 ]]; then
    fail "Test 3: Expected non-zero exit on dirty checkout, got 0"
  fi
  log "Test 3: installer exited ${install_rc} (expected non-zero) — OK"

  # Error message must mention the checkout path so the user knows where
  # to look, and must include a recovery hint.
  if ! grep -Fq "${source_dir}" <<< "${err_output}"; then
    fail "Test 3: Error message does not mention the checkout path (${source_dir}): ${err_output}"
  fi

  # Dirty file must be untouched.
  if [[ ! -f "${dirty_file}" ]]; then
    fail "Test 3: Installer deleted the dirty file at ${dirty_file}"
  fi
  local after_content
  after_content="$(cat "${dirty_file}")"
  if [[ "${after_content}" != "${dirty_content}" ]]; then
    fail "Test 3: Installer modified the dirty file at ${dirty_file}"
  fi

  # Git status must still show the dirty state.
  local git_status
  git_status="$(git -C "${source_dir}" status --porcelain 2>/dev/null)"
  if [[ -z "${git_status}" ]]; then
    fail "Test 3: git status is clean after dirty-checkout test; installer may have reset the working tree"
  fi
  log "Test 3 PASSED"

  # ── Test 4: Local-only commits must cause install failure ─────────────────
  log "=== Test 4: Local-only commits must cause install failure ==="

  # Clean the dirty file left by Test 3 so the working tree is pristine,
  # then create and commit a local-only change that is not on origin.
  rm -f "${dirty_file}"
  local local_commit_file="${source_dir}/SMOKE_LOCAL_COMMIT"
  printf 'local commit added by source-install smoke test\n' > "${local_commit_file}"
  git -C "${source_dir}" add "${local_commit_file}"
  git -C "${source_dir}" \
    -c user.email="smoke@test.local" \
    -c user.name="Smoke Test" \
    commit -m "smoke: local-only commit not on origin"

  local err4_output
  local install4_rc=0
  err4_output="$(
    HOME="${fake_home}" \
    CARGO_HOME="${CARGO_HOME:-${HOME}/.cargo}" \
    RUSTUP_HOME="${RUSTUP_HOME:-${HOME}/.rustup}" \
    TIZENCLAW_INSTALL_ROOT="${INSTALL_ROOT}" \
    TIZENCLAW_BASHRC_PATH="${fake_home}/.bashrc" \
    TIZENCLAW_SKIP_SERVICES="1" \
    TIZENCLAW_NO_NETWORK_FALLBACK="1" \
      bash "${PROJECT_DIR}/install.sh" \
        --source-install \
        --repo "${bare_url}" \
        --ref "${test_ref}" \
        --dir "${source_dir}" \
        --skip-deps \
        --skip-setup \
        -- --no-restart --build-root "${build_root}" \
      2>&1 >/dev/null
  )" || install4_rc=$?

  if [[ "${install4_rc}" -eq 0 ]]; then
    fail "Test 4: Expected non-zero exit when checkout has local-only commits, got 0"
  fi
  log "Test 4: installer exited ${install4_rc} (expected non-zero) — OK"

  if ! grep -Fq "${source_dir}" <<< "${err4_output}"; then
    fail "Test 4: Error message does not mention the checkout path (${source_dir}): ${err4_output}"
  fi

  # The local commit must still exist — installer must not have reset HEAD.
  local head_after
  head_after="$(git -C "${source_dir}" log --oneline -1 2>/dev/null)"
  if ! grep -Fq "smoke: local-only commit not on origin" <<< "${head_after}"; then
    fail "Test 4: Local commit was removed by the installer: ${head_after}"
  fi
  log "Test 4 PASSED"

  # ── Test 5: locked worktree must cause installer to fail (not build from wrong branch) ──
  # Reproduces the scenario where SOURCE_DIR is on a different branch and
  # REPO_REF is locked in a linked worktree.  The installer must reject this
  # with a non-zero exit rather than silently building from the wrong branch.
  log "=== Test 5: locked worktree causes installer to fail cleanly ==="

  local source_dir_wt="${TMP_DIR}/source-wt"
  local fake_home_wt="${TMP_DIR}/home-wt"
  local install_root_wt="${fake_home_wt}/.tizenclaw"
  local locked_wt="${TMP_DIR}/locked-wt"
  mkdir -p "${fake_home_wt}"

  # Fresh clone from bare repo; start on test_ref.
  git clone "${bare_url}" "${source_dir_wt}"
  git -C "${source_dir_wt}" checkout "${test_ref}"

  # Create a diverged branch and leave the main clone on it.  other-branch
  # starts at the same commit as test_ref so no local-only-commit guard fires.
  git -C "${source_dir_wt}" \
    -c user.email="smoke@test.local" \
    -c user.name="Smoke Test" \
    checkout -b other-branch

  # Record the current HEAD on other-branch; it must not move after the install
  # attempt, proving the installer did not touch the wrong branch.
  local head_before_wt
  head_before_wt="$(git -C "${source_dir_wt}" rev-parse HEAD)"

  # Lock test_ref in a linked worktree.  The installer is pointed at
  # source_dir_wt (on other-branch) with --ref test_ref; it must fail rather
  # than proceeding to build from other-branch.
  git -C "${source_dir_wt}" worktree add --detach "${locked_wt}"
  git -C "${locked_wt}" checkout "${test_ref}"

  local err5_output
  local install5_rc=0
  err5_output="$(
    HOME="${fake_home_wt}" \
    CARGO_HOME="${CARGO_HOME:-${HOME}/.cargo}" \
    RUSTUP_HOME="${RUSTUP_HOME:-${HOME}/.rustup}" \
    TIZENCLAW_INSTALL_ROOT="${install_root_wt}" \
    TIZENCLAW_BASHRC_PATH="${fake_home_wt}/.bashrc" \
    TIZENCLAW_SKIP_SERVICES="1" \
    TIZENCLAW_NO_NETWORK_FALLBACK="1" \
      bash "${PROJECT_DIR}/install.sh" \
        --source-install \
        --repo "${bare_url}" \
        --ref "${test_ref}" \
        --dir "${source_dir_wt}" \
        --skip-deps \
        --skip-setup \
        -- --no-restart --build-root "${build_root}" \
      2>&1 >/dev/null
  )" || install5_rc=$?

  if [[ "${install5_rc}" -eq 0 ]]; then
    fail "Test 5: Expected non-zero exit when REPO_REF is locked in another worktree, got 0"
  fi
  log "Test 5: installer exited ${install5_rc} (expected non-zero) — OK"

  # Error message must mention the checkout path so the user can act on it.
  if ! grep -Fq "${source_dir_wt}" <<< "${err5_output}"; then
    fail "Test 5: Error message does not mention the checkout path (${source_dir_wt}): ${err5_output}"
  fi

  # The main clone must still be on other-branch and HEAD must not have moved.
  local branch_after_wt
  branch_after_wt="$(git -C "${source_dir_wt}" symbolic-ref --short HEAD \
    2>/dev/null || echo 'DETACHED')"
  if [[ "${branch_after_wt}" != "other-branch" ]]; then
    fail "Test 5: Branch changed from other-branch to '${branch_after_wt}'; installer switched branches"
  fi
  local head_after_wt
  head_after_wt="$(git -C "${source_dir_wt}" rev-parse HEAD)"
  if [[ "${head_after_wt}" != "${head_before_wt}" ]]; then
    fail "Test 5: HEAD moved from ${head_before_wt} to ${head_after_wt}; installer modified the wrong branch"
  fi

  git -C "${source_dir_wt}" worktree remove --force "${locked_wt}" || true
  log "Test 5 PASSED"

  log "Source-install smoke test PASSED"
}

main "$@"
