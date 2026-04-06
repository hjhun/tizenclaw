#!/usr/bin/env bash
set -euo pipefail

REPO_URL="https://github.com/hjhun/tizenclaw.git"
REPO_REF="develRust"
INSTALL_DIR="${HOME}/.local/src/tizenclaw"
SKIP_DEPS=false
SKIP_SETUP=false
HOST_ARGS=()
HOST_BIN_DIR="${HOME}/.tizenclaw/bin"

log() {
  printf '[install] %s\n' "$*"
}

warn() {
  printf '[install][warn] %s\n' "$*" >&2
}

fail() {
  printf '[install][fail] %s\n' "$*" >&2
  exit 1
}

usage() {
  cat <<'EOF'
TizenClaw GitHub bootstrap installer

Usage:
  ./install.sh [options] [-- deploy_host_args...]

Options:
  --repo <url>       Override the Git repository URL
  --ref <git-ref>    Git branch, tag, or commit to checkout
  --dir <path>       Destination directory for the repository clone
  --skip-deps        Skip apt and rustup bootstrap steps
  --skip-setup       Skip the interactive post-install setup wizard
  --debug            Forward --debug to deploy_host.sh
  --build-only       Forward --build-only to deploy_host.sh
  --test             Forward --test to deploy_host.sh
  -h, --help         Show this help

Examples:
  ./install.sh
  ./install.sh --build-only
  ./install.sh --ref develRust --dir "$HOME/src/tizenclaw"
  ./install.sh -- --status
EOF
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --repo)
        [[ $# -lt 2 ]] && fail "--repo requires a value"
        REPO_URL="$2"
        shift 2
        ;;
      --ref)
        [[ $# -lt 2 ]] && fail "--ref requires a value"
        REPO_REF="$2"
        shift 2
        ;;
      --dir)
        [[ $# -lt 2 ]] && fail "--dir requires a value"
        INSTALL_DIR="$2"
        shift 2
        ;;
      --skip-deps)
        SKIP_DEPS=true
        shift
        ;;
      --skip-setup)
        SKIP_SETUP=true
        shift
        ;;
      --debug)
        HOST_ARGS+=("--debug")
        shift
        ;;
      --build-only)
        HOST_ARGS+=("--build-only")
        shift
        ;;
      --test)
        HOST_ARGS+=("--test")
        shift
        ;;
      --)
        shift
        HOST_ARGS+=("$@")
        break
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        fail "Unknown option: $1"
        ;;
    esac
  done
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1
}

install_apt_deps() {
  if ! need_cmd apt-get; then
    fail "apt-get not found. This installer currently targets Ubuntu/WSL."
  fi

  log "Installing Ubuntu build dependencies"
  sudo env DEBIAN_FRONTEND=noninteractive apt-get update
  sudo env DEBIAN_FRONTEND=noninteractive apt-get install -y \
    build-essential \
    ca-certificates \
    clang \
    cmake \
    curl \
    git \
    iproute2 \
    libssl-dev \
    make \
    pkg-config \
    perl \
    python3
}

install_rustup() {
  if need_cmd cargo && need_cmd rustc; then
    log "Rust toolchain already available"
    return
  fi

  log "Installing Rust toolchain with rustup"
  curl https://sh.rustup.rs -sSf | sh -s -- -y
  # shellcheck disable=SC1090
  source "${HOME}/.cargo/env"
}

ensure_rust_in_shell() {
  if need_cmd cargo && need_cmd rustc; then
    return
  fi

  if [[ -f "${HOME}/.cargo/env" ]]; then
    # shellcheck disable=SC1090
    source "${HOME}/.cargo/env"
  fi

  need_cmd cargo || fail "cargo is still unavailable after bootstrap"
  need_cmd rustc || fail "rustc is still unavailable after bootstrap"
}

prepare_repo() {
  local parent_dir
  parent_dir="$(dirname "${INSTALL_DIR}")"
  mkdir -p "${parent_dir}"

  if [[ -e "${INSTALL_DIR}" && ! -d "${INSTALL_DIR}/.git" ]]; then
    fail "${INSTALL_DIR} exists but is not a Git checkout"
  fi

  if [[ -d "${INSTALL_DIR}/.git" ]]; then
    log "Updating existing repository at ${INSTALL_DIR}"
    git -C "${INSTALL_DIR}" fetch --tags origin
  else
    log "Cloning ${REPO_URL} into ${INSTALL_DIR}"
    git clone "${REPO_URL}" "${INSTALL_DIR}"
  fi

  log "Checking out ${REPO_REF}"
  git -C "${INSTALL_DIR}" checkout "${REPO_REF}"

  if git -C "${INSTALL_DIR}" rev-parse --verify "origin/${REPO_REF}" >/dev/null 2>&1; then
    git -C "${INSTALL_DIR}" reset --hard "origin/${REPO_REF}"
  else
    warn "origin/${REPO_REF} not found; using the checked out ref as-is"
  fi
}

run_host_install() {
  [[ -x "${INSTALL_DIR}/deploy_host.sh" ]] || fail "deploy_host.sh not found"

  log "Running deploy_host.sh ${HOST_ARGS[*]:-}"
  (
    cd "${INSTALL_DIR}"
    ./deploy_host.sh "${HOST_ARGS[@]}"
  )
}

host_args_contain() {
  local wanted="$1"
  for arg in "${HOST_ARGS[@]}"; do
    if [[ "${arg}" == "${wanted}" ]]; then
      return 0
    fi
  done
  return 1
}

should_run_setup() {
  if [[ "${SKIP_SETUP}" == true ]]; then
    return 1
  fi
  for disallowed in "--build-only" "--test" "--status" "--log" "--stop" "--remove" "--restart-only"; do
    if host_args_contain "${disallowed}"; then
      return 1
    fi
  done
  return 0
}

config_fingerprint() {
  local config_dir="${HOST_BIN_DIR%/bin}/config"
  if [[ ! -d "${config_dir}" ]]; then
    return 0
  fi
  find "${config_dir}" -maxdepth 1 -type f -name '*config.json' -printf '%f %T@ %s\n' 2>/dev/null | sort
}

run_setup_wizard() {
  local cli_bin="${HOST_BIN_DIR}/tizenclaw-cli"
  local before_fingerprint
  local after_fingerprint
  if [[ ! -x "${cli_bin}" ]]; then
    warn "Skipping setup because ${cli_bin} is not available yet"
    return 0
  fi

  before_fingerprint="$(config_fingerprint)"
  log "Launching the interactive TizenClaw setup wizard"
  "${cli_bin}" setup
  after_fingerprint="$(config_fingerprint)"

  if [[ "${before_fingerprint}" != "${after_fingerprint}" ]]; then
    log "Restarting host services to apply the latest configuration"
    (
      cd "${INSTALL_DIR}"
      ./deploy_host.sh --restart-only
    )
  else
    log "No config changes detected; keeping the current services running"
  fi
}

main() {
  parse_args "$@"

  if [[ "${SKIP_DEPS}" != true ]]; then
    install_apt_deps
    install_rustup
  fi

  ensure_rust_in_shell
  prepare_repo
  run_host_install
  if should_run_setup; then
    run_setup_wizard
  else
    log "Skipping interactive setup wizard"
  fi

  log "TizenClaw host bootstrap complete"
}

main "$@"
