#!/usr/bin/env bash
set -euo pipefail

RELEASE_REPO="hjhun/tizenclaw"
RELEASE_VERSION="latest"
ASSET_URL=""
SOURCE_INSTALL=false
LOCAL_CHECKOUT=false

REPO_URL="https://github.com/hjhun/tizenclaw.git"
REPO_REF="develRust"
SOURCE_DIR="${HOME}/.local/src/tizenclaw"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

HOST_BASE_DIR="${TIZENCLAW_INSTALL_ROOT:-${HOME}/.tizenclaw}"
HOST_BIN_DIR="${HOST_BASE_DIR}/bin"
HOST_MANAGE_SCRIPT="${HOST_BASE_DIR}/manage/deploy_host.sh"
BASHRC_PATH="${TIZENCLAW_BASHRC_PATH:-${HOME}/.bashrc}"
PATH_EXPORT='export PATH="$HOME/.tizenclaw/bin:$PATH"'

SKIP_DEPS=false
SKIP_SETUP=false
HOST_ARGS=()

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
TizenClaw host installer

Usage:
  ./install.sh [options] [-- deploy_host_args...]

Default mode:
  Download a prebuilt host bundle from GitHub Releases and install it
  under ~/.tizenclaw.

Options:
  --version <tag>      Release tag to install (default: latest)
  --asset-url <url>    Override the bundle asset URL
  --source-install     Clone the repository and build on the local host
  --local-checkout     Install from the current repository checkout
  --repo <url>         Override the Git repository URL for source install
  --ref <git-ref>      Git ref to checkout for source install
  --dir <path>         Repository clone directory for source install
  --skip-deps          Skip apt and rustup bootstrap steps
  --skip-setup         Skip the interactive post-install setup wizard
  --debug              Forward --debug to deploy_host.sh in source mode
  --build-only         Forward --build-only to deploy_host.sh in source mode
  --test               Forward --test to deploy_host.sh in source mode
  -h, --help           Show this help

Examples:
  ./install.sh
  ./install.sh --local-checkout
  ./install.sh --version v1.0.0
  ./install.sh --asset-url file:///tmp/tizenclaw-host-bundle.tar.gz
  ./install.sh --source-install --ref develRust
EOF
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --version)
        [[ $# -lt 2 ]] && fail "--version requires a value"
        RELEASE_VERSION="$2"
        shift 2
        ;;
      --asset-url)
        [[ $# -lt 2 ]] && fail "--asset-url requires a value"
        ASSET_URL="$2"
        shift 2
        ;;
      --source-install)
        SOURCE_INSTALL=true
        shift
        ;;
      --local-checkout)
        LOCAL_CHECKOUT=true
        shift
        ;;
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
        SOURCE_DIR="$2"
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

auto_select_local_checkout() {
  if [[ "${SOURCE_INSTALL}" == true || "${LOCAL_CHECKOUT}" == true ]]; then
    return
  fi

  if [[ -n "${ASSET_URL}" || "${RELEASE_VERSION}" != "latest" ]]; then
    return
  fi

  if [[ -x "${SCRIPT_DIR}/deploy_host.sh" && -d "${SCRIPT_DIR}/.git" ]]; then
    LOCAL_CHECKOUT=true
    log "Detected repository checkout; defaulting to --local-checkout"
  fi
}

install_runtime_deps() {
  if ! need_cmd apt-get; then
    fail "apt-get not found. This installer currently targets Ubuntu/WSL."
  fi

  log "Installing Ubuntu runtime dependencies"
  sudo env DEBIAN_FRONTEND=noninteractive apt-get update
  sudo env DEBIAN_FRONTEND=noninteractive apt-get install -y \
    ca-certificates \
    curl \
    iproute2 \
    python3 \
    tar
}

install_build_deps() {
  install_runtime_deps
  log "Installing Ubuntu build dependencies"
  sudo env DEBIAN_FRONTEND=noninteractive apt-get install -y \
    build-essential \
    clang \
    cmake \
    git \
    libssl-dev \
    make \
    pkg-config \
    perl
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

ensure_path_export() {
  if [[ ! -f "${BASHRC_PATH}" ]]; then
    touch "${BASHRC_PATH}"
  fi

  if grep -Fqx "${PATH_EXPORT}" "${BASHRC_PATH}" 2>/dev/null; then
    return
  fi

  printf '\n%s\n' "${PATH_EXPORT}" >> "${BASHRC_PATH}"
}

normalize_host_dashboard_config() {
  local config_path="${HOST_BASE_DIR}/config/channel_config.json"

  python3 - <<'PY' "${config_path}"
import json, pathlib, sys

path = pathlib.Path(sys.argv[1])
port = 9091

data = {"channels": []}
if path.exists():
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        data = {"channels": []}

channels = data.get("channels")
if not isinstance(channels, list):
    channels = []
    data["channels"] = channels

dashboard = None
for channel in channels:
    if isinstance(channel, dict) and channel.get("name") == "web_dashboard":
        dashboard = channel
        break

if dashboard is None:
    dashboard = {
        "name": "web_dashboard",
        "type": "web_dashboard",
        "enabled": True,
        "settings": {},
    }
    channels.append(dashboard)

settings = dashboard.get("settings")
if not isinstance(settings, dict):
    settings = {}
    dashboard["settings"] = settings

dashboard.setdefault("type", "web_dashboard")
dashboard.setdefault("enabled", True)
settings["port"] = port
settings.setdefault("localhost_only", False)

path.parent.mkdir(parents=True, exist_ok=True)
path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
PY
}

prepare_host_runtime_dirs() {
  mkdir -p \
    "${HOST_BASE_DIR}/bin" \
    "${HOST_BASE_DIR}/lib/pkgconfig" \
    "${HOST_BASE_DIR}/include" \
    "${HOST_BASE_DIR}/config" \
    "${HOST_BASE_DIR}/sample" \
    "${HOST_BASE_DIR}/manage" \
    "${HOST_BASE_DIR}/run" \
    "${HOST_BASE_DIR}/tools/cli" \
    "${HOST_BASE_DIR}/workspace/skills" \
    "${HOST_BASE_DIR}/logs" \
    "${HOST_BASE_DIR}/embedded" \
    "${HOST_BASE_DIR}/web" \
    "${HOST_BASE_DIR}/docs" \
    "${HOST_BASE_DIR}/workflows" \
    "${HOST_BASE_DIR}/pipelines" \
    "${HOST_BASE_DIR}/codes" \
    "${HOST_BASE_DIR}/memory" \
    "${HOST_BASE_DIR}/plugins"

  if [[ -L "${HOST_BASE_DIR}/tools/skills" \
    || -d "${HOST_BASE_DIR}/tools/skills" \
    || -f "${HOST_BASE_DIR}/tools/skills" ]]; then
    rm -rf "${HOST_BASE_DIR}/tools/skills"
  fi
  ln -s "${HOST_BASE_DIR}/workspace/skills" "${HOST_BASE_DIR}/tools/skills"
}

copy_tree_contents() {
  local src="$1"
  local dest="$2"
  if [[ -d "${src}" ]]; then
    mkdir -p "${dest}"
    cp -a "${src}/." "${dest}/"
  fi
}

seed_config_from_bundle() {
  local bundle_root="$1"
  local file_name
  local target_path

  while IFS= read -r config_path; do
    file_name="$(basename "${config_path}")"
    target_path="${HOST_BASE_DIR}/config/${file_name}"
    if [[ ! -f "${target_path}" ]]; then
      install -m 644 "${config_path}" "${target_path}"
    fi
  done < <(find "${bundle_root}/config" -maxdepth 1 -type f | sort)

  if [[ -d "${bundle_root}/sample" ]]; then
    copy_tree_contents "${bundle_root}/sample" "${HOST_BASE_DIR}/sample"
  fi

  normalize_host_dashboard_config
}

restart_host_services() {
  if [[ "${TIZENCLAW_SKIP_SERVICES:-}" == "1" ]]; then
    log "Skipping service restart (TIZENCLAW_SKIP_SERVICES=1)"
    return
  fi

  if [[ -x "${HOST_MANAGE_SCRIPT}" ]]; then
    TIZENCLAW_INSTALL_ROOT="${HOST_BASE_DIR}" \
    TIZENCLAW_BASHRC_PATH="${BASHRC_PATH}" \
      "${HOST_MANAGE_SCRIPT}" --restart-only
    return
  fi

  if [[ -x "${SOURCE_DIR}/deploy_host.sh" ]]; then
    (
      cd "${SOURCE_DIR}"
      TIZENCLAW_INSTALL_ROOT="${HOST_BASE_DIR}" \
      TIZENCLAW_BASHRC_PATH="${BASHRC_PATH}" \
        ./deploy_host.sh --restart-only
    )
    return
  fi

  warn "No host management script found; skipping restart"
}

stop_host_services_if_present() {
  if [[ "${TIZENCLAW_SKIP_SERVICES:-}" == "1" ]]; then
    log "Skipping service stop (TIZENCLAW_SKIP_SERVICES=1)"
    return
  fi

  wait_for_exit() {
    local binary_name="$1"
    local attempts=0
    local match_pat="${HOST_BIN_DIR}/${binary_name}([[:space:]]|\$)"

    while pgrep -u "$(id -u)" -f "${match_pat}" >/dev/null 2>&1; do
      if [[ "${attempts}" -ge 5 ]]; then
        pkill -9 -u "$(id -u)" -f "${match_pat}" >/dev/null 2>&1 || true
        break
      fi
      sleep 1
      attempts=$((attempts + 1))
    done
  }

  if [[ -x "${HOST_MANAGE_SCRIPT}" ]]; then
    TIZENCLAW_INSTALL_ROOT="${HOST_BASE_DIR}" \
    TIZENCLAW_BASHRC_PATH="${BASHRC_PATH}" \
      "${HOST_MANAGE_SCRIPT}" --stop || true
  else
    pkill -f "${HOST_BIN_DIR}/tizenclaw-tool-executor" >/dev/null 2>&1 || true
    pkill -f "${HOST_BIN_DIR}/tizenclaw-web-dashboard" >/dev/null 2>&1 || true
    pkill -f "${HOST_BIN_DIR}/tizenclaw" >/dev/null 2>&1 || true
  fi

  wait_for_exit "tizenclaw-tool-executor"
  wait_for_exit "tizenclaw-web-dashboard"
  wait_for_exit "tizenclaw"
}

install_release_bundle() {
  local bundle_root="$1"

  stop_host_services_if_present
  prepare_host_runtime_dirs

  copy_tree_contents "${bundle_root}/bin" "${HOST_BASE_DIR}/bin"
  copy_tree_contents "${bundle_root}/lib" "${HOST_BASE_DIR}/lib"
  copy_tree_contents "${bundle_root}/include" "${HOST_BASE_DIR}/include"
  copy_tree_contents "${bundle_root}/web" "${HOST_BASE_DIR}/web"
  copy_tree_contents "${bundle_root}/docs" "${HOST_BASE_DIR}/docs"
  copy_tree_contents "${bundle_root}/embedded" "${HOST_BASE_DIR}/embedded"
  copy_tree_contents "${bundle_root}/manage" "${HOST_BASE_DIR}/manage"

  if [[ -f "${bundle_root}/bundle-manifest.json" ]]; then
    install -m 644 "${bundle_root}/bundle-manifest.json" "${HOST_BASE_DIR}/bundle-manifest.json"
  fi

  if [[ -x "${HOST_MANAGE_SCRIPT}" ]]; then
    ln -sf ../manage/deploy_host.sh "${HOST_BIN_DIR}/tizenclaw-hostctl"
  fi

  seed_config_from_bundle "${bundle_root}"
  ensure_path_export
  restart_host_services
}

resolve_latest_asset_url() {
  local metadata_file
  local resolved
  metadata_file="$(mktemp)"

  curl -fsSL \
    -H "Accept: application/vnd.github+json" \
    "https://api.github.com/repos/${RELEASE_REPO}/releases/latest" \
    -o "${metadata_file}"

  mapfile -t resolved < <(
    python3 - <<'PY' "${metadata_file}"
import json, pathlib, re, sys

data = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
pattern = re.compile(r"^tizenclaw-host-bundle-.*-linux-x86_64\.tar\.gz$")

for asset in data.get("assets") or []:
    name = asset.get("name", "")
    if pattern.match(name):
        print(data.get("tag_name", "latest"))
        print(asset.get("browser_download_url", ""))
        break
else:
    raise SystemExit("No host bundle asset found in the latest release")
PY
  )

  rm -f "${metadata_file}"

  RELEASE_VERSION="${resolved[0]}"
  ASSET_URL="${resolved[1]}"
}

resolve_release_asset_url() {
  if [[ -n "${ASSET_URL}" ]]; then
    return
  fi

  if [[ "${RELEASE_VERSION}" == "latest" ]]; then
    resolve_latest_asset_url
    return
  fi

  ASSET_URL="https://github.com/${RELEASE_REPO}/releases/download/${RELEASE_VERSION}/tizenclaw-host-bundle-${RELEASE_VERSION}-linux-x86_64.tar.gz"
}

download_release_bundle() {
  local asset_path="$1"
  resolve_release_asset_url

  log "Downloading host bundle from ${ASSET_URL}"
  curl -fsSL "${ASSET_URL}" -o "${asset_path}"
}

fetch_checksum_for_url() {
  local asset_url="$1"

  if [[ "${asset_url}" == file://* ]]; then
    local local_cs="${asset_url#file://}.sha256"
    if [[ -f "${local_cs}" ]]; then
      cat "${local_cs}"
      return 0
    fi
    return 1
  fi

  local cs_content
  cs_content="$(curl -fsSL "${asset_url}.sha256" 2>/dev/null)" || return 1
  printf '%s\n' "${cs_content}"
}

verify_bundle_checksum() {
  local asset_url="$1"
  local archive_path="$2"
  local cs_content

  if ! cs_content="$(fetch_checksum_for_url "${asset_url}")"; then
    if [[ "${asset_url}" == https://* ]]; then
      fail "Checksum file not retrievable for ${asset_url}; refusing to install an unverified bundle. Ensure the .sha256 asset is published alongside the bundle."
    fi
    warn "No checksum file found for ${asset_url}; skipping integrity check"
    return 0
  fi

  local expected_hash
  expected_hash="$(awk '{print $1}' <<< "${cs_content}")"
  local actual_hash
  actual_hash="$(sha256sum "${archive_path}" | awk '{print $1}')"

  if [[ "${expected_hash}" != "${actual_hash}" ]]; then
    fail "Bundle checksum mismatch: expected ${expected_hash}, got ${actual_hash}"
  fi

  log "Bundle checksum verified: ${actual_hash}"
}

locate_bundle_root() {
  local extracted_root="$1"
  find "${extracted_root}" -name bundle-manifest.json -print -quit | xargs -r dirname
}

install_from_release_asset() {
  local temp_dir
  local archive_path
  local bundle_root

  temp_dir="$(mktemp -d)"
  archive_path="${temp_dir}/tizenclaw-host-bundle.tar.gz"

  download_release_bundle "${archive_path}"
  verify_bundle_checksum "${ASSET_URL}" "${archive_path}"
  tar -xzf "${archive_path}" -C "${temp_dir}"

  bundle_root="$(locate_bundle_root "${temp_dir}")"
  [[ -n "${bundle_root}" ]] || fail "bundle-manifest.json not found in the archive"

  install_release_bundle "${bundle_root}"
  rm -rf "${temp_dir}"
}

prepare_repo() {
  local parent_dir
  parent_dir="$(dirname "${SOURCE_DIR}")"
  mkdir -p "${parent_dir}"

  if [[ -e "${SOURCE_DIR}" && ! -d "${SOURCE_DIR}/.git" ]]; then
    fail "${SOURCE_DIR} exists but is not a Git checkout"
  fi

  if [[ -d "${SOURCE_DIR}/.git" ]]; then
    log "Updating existing repository at ${SOURCE_DIR}"
    git -C "${SOURCE_DIR}" fetch --tags origin
  else
    log "Cloning ${REPO_URL} into ${SOURCE_DIR}"
    git clone "${REPO_URL}" "${SOURCE_DIR}"
  fi

  log "Checking out ${REPO_REF}"
  git -C "${SOURCE_DIR}" checkout "${REPO_REF}"

  if git -C "${SOURCE_DIR}" rev-parse --verify "origin/${REPO_REF}" >/dev/null 2>&1; then
    git -C "${SOURCE_DIR}" reset --hard "origin/${REPO_REF}"
  else
    warn "origin/${REPO_REF} not found; using the checked out ref as-is"
  fi
}

run_source_install() {
  [[ -x "${SOURCE_DIR}/deploy_host.sh" ]] || fail "deploy_host.sh not found"

  log "Running deploy_host.sh ${HOST_ARGS[*]:-}"
  (
    cd "${SOURCE_DIR}"
    ./deploy_host.sh "${HOST_ARGS[@]}"
  )
}

run_local_checkout_install() {
  [[ -x "${SCRIPT_DIR}/deploy_host.sh" ]] || fail "deploy_host.sh not found in ${SCRIPT_DIR}"
  [[ -d "${SCRIPT_DIR}/.git" ]] || fail "${SCRIPT_DIR} is not a Git checkout"

  log "Running deploy_host.sh from local checkout ${SCRIPT_DIR} ${HOST_ARGS[*]:-}"
  (
    cd "${SCRIPT_DIR}"
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
  local config_dir="${HOST_BASE_DIR}/config"
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
    restart_host_services
  else
    log "No config changes detected; keeping the current services running"
  fi
}

main() {
  parse_args "$@"
  auto_select_local_checkout

  if [[ "${LOCAL_CHECKOUT}" == true ]]; then
    if [[ "${SKIP_DEPS}" != true ]]; then
      install_build_deps
      install_rustup
    fi
    ensure_rust_in_shell
    run_local_checkout_install
  elif [[ "${SOURCE_INSTALL}" == true ]]; then
    if [[ "${SKIP_DEPS}" != true ]]; then
      install_build_deps
      install_rustup
    fi
    ensure_rust_in_shell
    prepare_repo
    run_source_install
  else
    if [[ "${SKIP_DEPS}" != true ]]; then
      install_runtime_deps
    fi
    install_from_release_asset
  fi

  if should_run_setup; then
    run_setup_wizard
  else
    log "Skipping interactive setup wizard"
  fi

  log "TizenClaw host install complete"
}

main "$@"
