#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
HOST_BASE_DIR="${HOME}/.tizenclaw"
CARGO_TARGET_DIR_HOST="${CARGO_TARGET_DIR:-${HOST_BASE_DIR}/build/cargo-target}"
BUILD_MODE="release"
OUTPUT_DIR="${PROJECT_DIR}/dist"
VERSION=""
SKIP_BUILD=false

PKG_NAME="tizenclaw"
TOOL_EXECUTOR_NAME="tizenclaw-tool-executor"
CLI_NAME="tizenclaw-cli"
WEB_DASHBOARD_NAME="tizenclaw-web-dashboard"

usage() {
  cat <<'EOF'
Create a prebuilt host release bundle for GitHub Releases.

Usage:
  scripts/create_host_release_bundle.sh [options]

Options:
  --version <value>     Version or tag string to embed in the bundle name
  --output-dir <path>   Directory for the generated tar.gz and checksum
  --skip-build          Reuse the current host cargo build output
  -h, --help            Show this help
EOF
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --version)
        [[ $# -lt 2 ]] && { echo "--version requires a value" >&2; exit 1; }
        VERSION="$2"
        shift 2
        ;;
      --output-dir)
        [[ $# -lt 2 ]] && { echo "--output-dir requires a value" >&2; exit 1; }
        OUTPUT_DIR="$2"
        shift 2
        ;;
      --skip-build)
        SKIP_BUILD=true
        shift
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

resolve_version() {
  if [[ -n "${VERSION}" ]]; then
    return
  fi
  if git -C "${PROJECT_DIR}" describe --tags --exact-match >/dev/null 2>&1; then
    VERSION="$(git -C "${PROJECT_DIR}" describe --tags --exact-match)"
    return
  fi
  VERSION="dev-$(git -C "${PROJECT_DIR}" rev-parse --short HEAD)"
}

install_executable_if_present() {
  local src="$1"
  local dest="$2"
  if [[ -f "${src}" ]]; then
    install -m 755 "${src}" "${dest}"
  fi
}

install_data_if_present() {
  local src="$1"
  local dest="$2"
  if [[ -f "${src}" ]]; then
    install -m 644 "${src}" "${dest}"
  fi
}

require_build_artifact() {
  local path="$1"
  local label="$2"
  if [[ ! -f "${path}" ]]; then
    echo "Missing build artifact for ${label}: ${path}" >&2
    exit 1
  fi
}

copy_tree_contents() {
  local src="$1"
  local dest="$2"
  if [[ -d "${src}" ]]; then
    mkdir -p "${dest}"
    cp -a "${src}/." "${dest}/"
  fi
}

generate_pkgconfig_files() {
  local prefix="$1"
  local pkgconfig_dir="$2"

  mkdir -p "${pkgconfig_dir}"

  cat > "${pkgconfig_dir}/tizenclaw.pc" <<EOF
prefix=${prefix}
libdir=\${prefix}/lib
includedir=\${prefix}/include

Name: tizenclaw
Description: TizenClaw Agent C API library
Version: 1.0.0
Libs: -L\${libdir} -Wl,-rpath,\${libdir} -ltizenclaw
Cflags: -I\${includedir} -I\${includedir}/tizenclaw
EOF

  cat > "${pkgconfig_dir}/tizenclaw-core.pc" <<EOF
prefix=${prefix}
libdir=\${prefix}/lib
includedir=\${prefix}/include

Name: tizenclaw-core
Description: TizenClaw Plugin SDK
Version: 1.0.0
Libs: -L\${libdir} -Wl,-rpath,\${libdir} -ltizenclaw_core
Cflags: -I\${includedir}/tizenclaw/core -I\${includedir}/tizenclaw
Requires: tizenclaw, libcurl
EOF
}

write_bundle_manifest() {
  local bundle_root="$1"
  local commit_sha
  local generated_at

  commit_sha="$(git -C "${PROJECT_DIR}" rev-parse HEAD)"
  generated_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

  cat > "${bundle_root}/bundle-manifest.json" <<EOF
{
  "name": "tizenclaw-host-bundle",
  "version": "${VERSION}",
  "target": "linux-x86_64",
  "format_version": 1,
  "install_prefix": "~/.tizenclaw",
  "git_commit": "${commit_sha}",
  "generated_at": "${generated_at}"
}
EOF
}

main() {
  parse_args "$@"
  resolve_version

  local asset_basename="tizenclaw-host-bundle-${VERSION}-linux-x86_64"
  local archive_path
  local checksum_path
  local stage_dir
  local bundle_root
  local build_dir="${CARGO_TARGET_DIR_HOST}/${BUILD_MODE}"

  mkdir -p "${OUTPUT_DIR}"

  if [[ "${SKIP_BUILD}" != true ]]; then
    (
      cd "${PROJECT_DIR}"
      # Bundle generation packages release artifacts, so request a
      # release build explicitly instead of relying on deploy_host.sh's
      # default debug mode.
      ./deploy_host.sh --release -b
    )
  fi

  stage_dir="$(mktemp -d)"
  bundle_root="${stage_dir}/${asset_basename}"
  archive_path="${OUTPUT_DIR}/${asset_basename}.tar.gz"
  checksum_path="${archive_path}.sha256"

  require_build_artifact "${build_dir}/${PKG_NAME}" "${PKG_NAME}"
  require_build_artifact "${build_dir}/${TOOL_EXECUTOR_NAME}" "${TOOL_EXECUTOR_NAME}"
  require_build_artifact "${build_dir}/${CLI_NAME}" "${CLI_NAME}"
  require_build_artifact "${build_dir}/${WEB_DASHBOARD_NAME}" "${WEB_DASHBOARD_NAME}"
  require_build_artifact "${build_dir}/libtizenclaw.so" "libtizenclaw.so"
  require_build_artifact "${build_dir}/libtizenclaw.rlib" "libtizenclaw.rlib"

  mkdir -p \
    "${bundle_root}/bin" \
    "${bundle_root}/lib/pkgconfig" \
    "${bundle_root}/include/tizenclaw/core" \
    "${bundle_root}/config" \
    "${bundle_root}/sample" \
    "${bundle_root}/manage"

  install_executable_if_present \
    "${build_dir}/${PKG_NAME}" \
    "${bundle_root}/bin/${PKG_NAME}"
  install_executable_if_present \
    "${build_dir}/${TOOL_EXECUTOR_NAME}" \
    "${bundle_root}/bin/${TOOL_EXECUTOR_NAME}"
  install_executable_if_present \
    "${build_dir}/${CLI_NAME}" \
    "${bundle_root}/bin/${CLI_NAME}"
  install_executable_if_present \
    "${build_dir}/${WEB_DASHBOARD_NAME}" \
    "${bundle_root}/bin/${WEB_DASHBOARD_NAME}"
  install_data_if_present \
    "${build_dir}/libtizenclaw.so" \
    "${bundle_root}/lib/libtizenclaw.so"
  install_data_if_present \
    "${build_dir}/libtizenclaw.rlib" \
    "${bundle_root}/lib/libtizenclaw.rlib"

  install -m 644 \
    "${PROJECT_DIR}/src/libtizenclaw/include/tizenclaw.h" \
    "${bundle_root}/include/tizenclaw/tizenclaw.h"
  install -m 644 \
    "${PROJECT_DIR}/src/libtizenclaw-core/include/tizenclaw_error.h" \
    "${bundle_root}/include/tizenclaw/tizenclaw_error.h"
  install -m 644 \
    "${PROJECT_DIR}/src/libtizenclaw-core/include/tizenclaw_channel.h" \
    "${bundle_root}/include/tizenclaw/core/tizenclaw_channel.h"
  install -m 644 \
    "${PROJECT_DIR}/src/libtizenclaw-core/include/tizenclaw_llm_backend.h" \
    "${bundle_root}/include/tizenclaw/core/tizenclaw_llm_backend.h"
  install -m 644 \
    "${PROJECT_DIR}/src/libtizenclaw-core/include/tizenclaw_curl.h" \
    "${bundle_root}/include/tizenclaw/core/tizenclaw_curl.h"

  generate_pkgconfig_files "\$HOME/.tizenclaw" "${bundle_root}/lib/pkgconfig"

  while IFS= read -r config_path; do
    install -m 644 "${config_path}" "${bundle_root}/config/$(basename "${config_path}")"
  done < <(
    find "${PROJECT_DIR}/data/config" -maxdepth 1 -type f ! -name '*.sample' | sort
  )

  copy_tree_contents "${PROJECT_DIR}/data/sample" "${bundle_root}/sample"
  while IFS= read -r sample_path; do
    install -m 644 "${sample_path}" "${bundle_root}/sample/$(basename "${sample_path}")"
  done < <(
    find "${PROJECT_DIR}/data/config" -maxdepth 1 -type f -name '*.sample' | sort
  )

  copy_tree_contents "${PROJECT_DIR}/data/web" "${bundle_root}/web"
  install_data_if_present \
    "${PROJECT_DIR}/data/img/tizenclaw.svg" \
    "${bundle_root}/web/img/tizenclaw.svg"
  copy_tree_contents "${PROJECT_DIR}/data/docs" "${bundle_root}/docs"
  copy_tree_contents "${PROJECT_DIR}/tools/embedded" "${bundle_root}/embedded"
  install -m 755 \
    "${PROJECT_DIR}/deploy_host.sh" \
    "${bundle_root}/manage/deploy_host.sh"

  write_bundle_manifest "${bundle_root}"

  tar -czf "${archive_path}" -C "${stage_dir}" "${asset_basename}"
  (
    cd "${OUTPUT_DIR}"
    sha256sum "$(basename "${archive_path}")" > "$(basename "${checksum_path}")"
  )

  rm -rf "${stage_dir}"

  echo "Created host bundle:"
  echo "  ${archive_path}"
  echo "Checksum:"
  echo "  ${checksum_path}"
}

main "$@"
