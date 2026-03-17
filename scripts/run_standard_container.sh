#!/bin/bash
set -euo pipefail

APP_DATA_DIR="/opt/usr/share/tizenclaw"
BUNDLE_DIR="${APP_DATA_DIR}/bundles/standard_agent"
ROOTFS_TAR="${APP_DATA_DIR}/img/rootfs.tar.gz"
CONTAINER_ID="tizenclaw_standard"
LOG_FILE="/opt/var/log/tizenclaw-standard-container.log"
SAFE_MODE="${SAFE_MODE:-0}"

detect_runtime() {
  if [ "${SAFE_MODE}" = "1" ] && command -v runc >/dev/null 2>&1; then
    echo "runc"
    return
  fi
  if [ -x /usr/libexec/tizenclaw/crun ]; then
    echo "/usr/libexec/tizenclaw/crun"
    return
  fi
  if command -v crun >/dev/null 2>&1; then
    echo "crun"
    return
  fi
  if command -v runc >/dev/null 2>&1; then
    echo "runc"
    return
  fi
  echo ""
}

RUNTIME_BIN="$(detect_runtime)"
if [ -z "${RUNTIME_BIN}" ]; then
  echo "No OCI runtime found (crun/runc)." >&2
  exit 1
fi

mkdir -p "$(dirname "${LOG_FILE}")"
log() {
  echo "[$(date '+%Y-%m-%d %H:%M:%S%z')] $*" >>"${LOG_FILE}"
}

run_without_container() {
  log "Watchdog cgroup unavailable. Running without OCI container (fallback to chroot with unshare)."
  
  if [ "${SAFE_MODE}" = "1" ]; then
    CMD="/usr/bin/sleep 2147483647"
  else
    CMD="/usr/bin/tizenclaw"
  fi

  exec unshare -m --propagation unchanged /usr/bin/sh -c "
    mkdir -p \"${BUNDLE_DIR}/rootfs/proc\" \"${BUNDLE_DIR}/rootfs/dev\" \"${BUNDLE_DIR}/rootfs/sys\" \\
             \"${BUNDLE_DIR}/rootfs/usr\" \"${BUNDLE_DIR}/rootfs/lib\" \\
             \"${BUNDLE_DIR}/rootfs/etc/dbus-1\" \\
             \"${BUNDLE_DIR}/rootfs/opt/etc\" \\
             \"${BUNDLE_DIR}/rootfs/opt/usr/share/tizenclaw\" \"${BUNDLE_DIR}/rootfs/run\" \"${BUNDLE_DIR}/rootfs/tmp\"

    touch \"${BUNDLE_DIR}/rootfs/etc/resolv.conf\" 2>/dev/null || true
    mount --bind /etc/resolv.conf \"${BUNDLE_DIR}/rootfs/etc/resolv.conf\" || true
    touch \"${BUNDLE_DIR}/rootfs/etc/nsswitch.conf\" 2>/dev/null || true
    mount --bind /etc/nsswitch.conf \"${BUNDLE_DIR}/rootfs/etc/nsswitch.conf\" || true
    
    mount --make-rprivate / || true
    
    mount -t proc proc \"${BUNDLE_DIR}/rootfs/proc\" || true
    mount --rbind /sys \"${BUNDLE_DIR}/rootfs/sys\" || true
    mount --rbind /dev \"${BUNDLE_DIR}/rootfs/dev\" || true
    mount --rbind /usr \"${BUNDLE_DIR}/rootfs/usr\" || true
    mount --rbind /lib \"${BUNDLE_DIR}/rootfs/lib\" || true
    if [ -d /lib64 ]; then
      mkdir -p \"${BUNDLE_DIR}/rootfs/lib64\"
      mount --rbind /lib64 \"${BUNDLE_DIR}/rootfs/lib64\" || true
    fi
    touch \"${BUNDLE_DIR}/rootfs/etc/tizen-platform.conf\" 2>/dev/null || true
    mount --bind /etc/tizen-platform.conf \"${BUNDLE_DIR}/rootfs/etc/tizen-platform.conf\" || true
    mount --rbind /etc/dbus-1 \"${BUNDLE_DIR}/rootfs/etc/dbus-1\" || true
    if [ -f /etc/dlog.conf ]; then
      touch \"${BUNDLE_DIR}/rootfs/etc/dlog.conf\" 2>/dev/null || true
      mount --bind /etc/dlog.conf \"${BUNDLE_DIR}/rootfs/etc/dlog.conf\" || true
    fi
    if [ -d /etc/dlog.conf.d ]; then
      mkdir -p \"${BUNDLE_DIR}/rootfs/etc/dlog.conf.d\"
      mount --rbind /etc/dlog.conf.d \"${BUNDLE_DIR}/rootfs/etc/dlog.conf.d\" || true
    fi
    touch \"${BUNDLE_DIR}/rootfs/etc/passwd\" 2>/dev/null || true
    mount --bind /etc/passwd \"${BUNDLE_DIR}/rootfs/etc/passwd\" || true
    touch \"${BUNDLE_DIR}/rootfs/etc/group\" 2>/dev/null || true
    mount --bind /etc/group \"${BUNDLE_DIR}/rootfs/etc/group\" || true
    mount --rbind /opt/etc \"${BUNDLE_DIR}/rootfs/opt/etc\" || true
    mount -o remount,bind,ro \"${BUNDLE_DIR}/rootfs/opt/etc\" || true
    mount --rbind /opt/usr/share/tizenclaw \"${BUNDLE_DIR}/rootfs/opt/usr/share/tizenclaw\" || true
    mount --rbind /run \"${BUNDLE_DIR}/rootfs/run\" || true
    mount --rbind /tmp \"${BUNDLE_DIR}/rootfs/tmp\" || true

    exec chroot \"${BUNDLE_DIR}/rootfs\" ${CMD} 2>>/tmp/tizenclaw_daemon.log
  "
}

write_config() {
  local process_args_json
  if [ "${SAFE_MODE}" = "1" ]; then
    # Reboot triage: use inert PID1 instead of launching tizenclaw directly.
    process_args_json='["/usr/bin/sleep", "2147483647"]'
  else
    process_args_json='["/usr/bin/tizenclaw"]'
  fi

  # Build optional mount entries based on host filesystem
  local OPTIONAL_MOUNTS=""
  if [ -d /lib64 ]; then
    OPTIONAL_MOUNTS="${OPTIONAL_MOUNTS},
    {
      \"destination\": \"/lib64\",
      \"type\": \"bind\",
      \"source\": \"/lib64\",
      \"options\": [\"rbind\", \"ro\"]
    }"
  fi
  if [ -f /etc/dlog.conf ]; then
    OPTIONAL_MOUNTS="${OPTIONAL_MOUNTS},
    {
      \"destination\": \"/etc/dlog.conf\",
      \"type\": \"bind\",
      \"source\": \"/etc/dlog.conf\",
      \"options\": [\"bind\", \"ro\"]
    }"
  fi
  if [ -d /etc/dlog.conf.d ]; then
    OPTIONAL_MOUNTS="${OPTIONAL_MOUNTS},
    {
      \"destination\": \"/etc/dlog.conf.d\",
      \"type\": \"bind\",
      \"source\": \"/etc/dlog.conf.d\",
      \"options\": [\"rbind\", \"ro\"]
    }"
  fi

  cat >"${BUNDLE_DIR}/config.json" <<EOF
{
  "ociVersion": "1.0.2",
  "process": {
    "terminal": false,
    "user": {"uid": 0, "gid": 0},
    "args": ${process_args_json},
    "env": [
      "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
    ],
    "cwd": "/",
    "noNewPrivileges": true,
    "capabilities": {
      "bounding": [],
      "effective": [],
      "inheritable": [],
      "permitted": [],
      "ambient": []
    }
  },
  "root": {
    "path": "rootfs",
    "readonly": true
  },
  "mounts": [
    {
      "destination": "/proc",
      "type": "proc",
      "source": "proc"
    },
    {
      "destination": "/dev",
      "type": "bind",
      "source": "/dev",
      "options": ["rbind", "ro"]
    },
    {
      "destination": "/sys",
      "type": "bind",
      "source": "/sys",
      "options": ["rbind", "ro"]
    },
    {
      "destination": "/usr",
      "type": "bind",
      "source": "/usr",
      "options": ["rbind", "ro"]
    },
    {
      "destination": "/lib",
      "type": "bind",
      "source": "/lib",
      "options": ["rbind", "ro"]
    },
    {
      "destination": "/opt/etc",
      "type": "bind",
      "source": "/opt/etc",
      "options": ["rbind", "ro"]
    },
    {
      "destination": "/opt/usr/share/tizenclaw",
      "type": "bind",
      "source": "/opt/usr/share/tizenclaw",
      "options": ["rbind", "rw"]
    },
    {
      "destination": "/run",
      "type": "bind",
      "source": "/run",
      "options": ["rbind", "rslave", "rw"]
    },
    {
      "destination": "/etc/tizen-platform.conf",
      "type": "bind",
      "source": "/etc/tizen-platform.conf",
      "options": ["bind", "ro"]
    },
    {
      "destination": "/etc/dbus-1",
      "type": "bind",
      "source": "/etc/dbus-1",
      "options": ["rbind", "ro"]
    },
    {
      "destination": "/tmp",
      "type": "bind",
      "source": "/tmp",
      "options": ["rbind", "rw"]
    },
    {
      "destination": "/etc/resolv.conf",
      "type": "bind",
      "source": "/etc/resolv.conf",
      "options": ["bind", "ro"]
    },
    {
      "destination": "/etc/nsswitch.conf",
      "type": "bind",
      "source": "/etc/nsswitch.conf",
      "options": ["bind", "ro"]
    },
    {
      "destination": "/etc/passwd",
      "type": "bind",
      "source": "/etc/passwd",
      "options": ["bind", "ro"]
    },
    {
      "destination": "/etc/group",
      "type": "bind",
      "source": "/etc/group",
      "options": ["bind", "ro"]
    }${OPTIONAL_MOUNTS}
  ],
  "linux": {
    "namespaces": [
      {"type": "mount"}
    ],
    "maskedPaths": [
      "/proc/acpi",
      "/proc/kcore",
      "/proc/keys",
      "/proc/latency_stats",
      "/proc/timer_list",
      "/proc/timer_stats",
      "/proc/sched_debug",
      "/sys/firmware"
    ],
    "readonlyPaths": [
      "/proc/asound",
      "/proc/bus",
      "/proc/fs",
      "/proc/irq",
      "/proc/sys",
      "/proc/sysrq-trigger"
    ]
  }
}
EOF
}

prepare_bundle() {
  mkdir -p "${BUNDLE_DIR}/rootfs"
  # Check if rootfs has content (bin/ dir exists)
  # instead of relying on a marker file that
  # gets removed by RPM updates.
  if [ ! -d "${BUNDLE_DIR}/rootfs/bin" ]; then
    log "Extracting rootfs tarball..."
    rm -rf "${BUNDLE_DIR}/rootfs"
    mkdir -p "${BUNDLE_DIR}/rootfs"
    tar -xzf "${ROOTFS_TAR}" -C "${BUNDLE_DIR}/rootfs"
    log "Rootfs extraction complete"
  fi
  write_config
}

prepare_bundle
log "Starting standard container with runtime=${RUNTIME_BIN}, bundle=${BUNDLE_DIR}, id=${CONTAINER_ID}"
"${RUNTIME_BIN}" delete -f "${CONTAINER_ID}" >>"${LOG_FILE}" 2>&1 || true
cd "${BUNDLE_DIR}"
set +e
if [[ "$(basename "${RUNTIME_BIN}")" == "crun" ]]; then
  if { [ ! -d "/sys/fs/cgroup/watchdog" ] || [ ! -w "/sys/fs/cgroup/watchdog" ]; } \
    && ! command -v runc >/dev/null 2>&1; then
    set -e
    run_without_container
  fi
  # Disable cgroup auto-placement to avoid watchdog cgroup side effects on device.
  if "${RUNTIME_BIN}" run --help 2>&1 | grep -q -- "--cgroup-manager"; then
    "${RUNTIME_BIN}" run --cgroup-manager=disabled "${CONTAINER_ID}" >>"${LOG_FILE}" 2>&1
  else
    if [ "${SAFE_MODE}" = "1" ] && command -v runc >/dev/null 2>&1; then
      log "crun does not support --cgroup-manager, switching runtime to runc"
      RUNTIME_BIN="runc"
      "${RUNTIME_BIN}" run "${CONTAINER_ID}" >>"${LOG_FILE}" 2>&1
    else
      log "crun does not support --cgroup-manager, fallback to unshare+chroot"
      set -e
      run_without_container
    fi
  fi
else
  "${RUNTIME_BIN}" run "${CONTAINER_ID}" >>"${LOG_FILE}" 2>&1
fi
rc=$?
set -e
if [ "${rc}" -ne 0 ]; then
  log "Container run failed with rc=${rc}"
  "${RUNTIME_BIN}" list >>"${LOG_FILE}" 2>&1 || true
  exit "${rc}"
fi
