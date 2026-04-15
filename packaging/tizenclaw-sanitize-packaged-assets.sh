#!/bin/sh

set -eu

packaged_root="${TIZENCLAW_PACKAGED_ROOT:-/opt/usr/share/tizenclaw}"
packaged_owner="${TIZENCLAW_PACKAGED_OWNER:-root}"
packaged_group="${TIZENCLAW_PACKAGED_GROUP:-root}"
manifest_path="${TIZENCLAW_PACKAGED_MANIFEST:-${packaged_root}/.packaged-assets.manifest}"

[ -d "${packaged_root}" ] || exit 0

if [ ! -f "${manifest_path}" ]; then
  echo "Missing packaged asset manifest: ${manifest_path}" >&2
  exit 1
fi

allowlist="$(mktemp)"
cleanup() {
  rm -f "${allowlist}"
}
trap cleanup EXIT HUP INT TERM

while IFS= read -r entry || [ -n "${entry}" ]; do
  case "${entry}" in
    "" | \#*)
      continue
      ;;
  esac

  rel="${entry#./}"
  rel="${rel#/}"
  [ -n "${rel}" ] || continue
  printf '%s\n' "${rel}" >> "${allowlist}"
done < "${manifest_path}"

printf '%s\n' ".packaged-assets.manifest" >> "${allowlist}"
sort -u "${allowlist}" -o "${allowlist}"

find "${packaged_root}" -depth -mindepth 1 | while IFS= read -r path; do
  rel="${path#${packaged_root}/}"
  if ! grep -Fqx "${rel}" "${allowlist}"; then
    rm -rf "${path}"
  fi
done

chown "${packaged_owner}:${packaged_group}" "${packaged_root}"
find "${packaged_root}" -mindepth 1 -exec \
  chown "${packaged_owner}:${packaged_group}" {} \;

find "${packaged_root}" -type d -exec chmod 755 {} \;
find "${packaged_root}" -type f -exec chmod 644 {} \;

if [ -f "${packaged_root}/plugins/libtizenclaw_plugin.so" ]; then
  chmod 755 "${packaged_root}/plugins/libtizenclaw_plugin.so"
fi

if command -v chsmack >/dev/null 2>&1; then
  find "${packaged_root}" -type d -exec chsmack -a _ {} \;
fi
