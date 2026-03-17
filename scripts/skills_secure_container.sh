#!/bin/bash
set -euo pipefail

APP_DATA_DIR="/opt/usr/share/tizenclaw"
BUNDLE_DIR="${APP_DATA_DIR}/bundles/skills_secure"
ROOTFS_TAR="${APP_DATA_DIR}/img/rootfs.tar.gz"
CONTAINER_ID="tizenclaw_skills_secure"
MERGED_USR="${BUNDLE_DIR}/merged_usr"

detect_runtime() {
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

write_config() {
  cat >"${BUNDLE_DIR}/config.json" <<EOF
{
  "ociVersion": "1.0.2",
  "process": {
    "terminal": false,
    "user": {"uid": 0, "gid": 0},
    "args": ["python3", "/skills/skill_executor.py"],
    "env": [
      "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
      "LD_LIBRARY_PATH=/usr/lib:/usr/lib64:/host_lib:/lib64"
    ],
    "cwd": "/",
    "noNewPrivileges": true,
    "capabilities": {
      "bounding": [],
      "effective": [],
      "inheritable": [],
      "permitted": [],
      "ambient": []
    },
    "rlimits": [
      {"type": "RLIMIT_NOFILE", "hard": 256, "soft": 256},
      {"type": "RLIMIT_NPROC", "hard": 64, "soft": 64},
      {"type": "RLIMIT_AS", "hard": 268435456, "soft": 268435456}
    ]
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
      "destination": "/skills",
      "type": "bind",
      "source": "${APP_DATA_DIR}/tools/skills",
      "options": ["rbind", "rw"]
    },
    {
      "destination": "/data",
      "type": "bind",
      "source": "${APP_DATA_DIR}/data",
      "options": ["rbind", "rw"]
    },
    {
      "destination": "/usr",
      "type": "bind",
      "source": "${MERGED_USR}",
      "options": ["rbind", "ro"]
    },
    {
      "destination": "/etc",
      "type": "bind",
      "source": "/etc",
      "options": ["rbind", "ro"]
    },
    {
      "destination": "/opt/etc",
      "type": "bind",
      "source": "/opt/etc",
      "options": ["rbind", "ro"]
    },
    {
      "destination": "/host_lib",
      "type": "bind",
      "source": "/lib",
      "options": ["rbind", "ro"]
    },
    {
      "destination": "/lib64",
      "type": "bind",
      "source": "/lib64",
      "options": ["rbind", "ro"]
    },
    {
      "destination": "/run",
      "type": "bind",
      "source": "/run",
      "options": ["rbind", "rw"]
    },
    {
      "destination": "/tmp",
      "type": "bind",
      "source": "/tmp",
      "options": ["rbind", "rw"]
    },
    {
      "destination": "/opt/usr/share/tizenclaw/tools/cli",
      "type": "bind",
      "source": "/opt/usr/share/tizenclaw/tools/cli",
      "options": ["rbind", "ro"]
    }
  ],
  "linux": {
    "cgroupsPath": "",
    "namespaces": [
      {"type": "mount"},
      {"type": "pid"},
      {"type": "ipc"}
    ],
    "seccomp": {
      "defaultAction": "SCMP_ACT_ERRNO",
      "architectures": ["SCMP_ARCH_X86_64", "SCMP_ARCH_X86", "SCMP_ARCH_AARCH64"],
      "syscalls": [{
        "names": [
          "read","write","open","close","stat","fstat","lstat",
          "poll","lseek","mmap","mprotect","munmap","brk",
          "ioctl","access","pipe","select","sched_yield",
          "dup","dup2","nanosleep","getpid","socket","connect",
          "sendto","recvfrom","sendmsg","recvmsg","bind","listen",
          "getsockname","getpeername","getsockopt","setsockopt",
          "clone","fork","vfork","execve","exit","wait4",
          "kill","uname","fcntl","flock","fsync","fdatasync",
          "truncate","ftruncate","getdents","getcwd","chdir",
          "mkdir","rmdir","creat","link","unlink","symlink",
          "readlink","chmod","chown","lchown","umask",
          "gettimeofday","getrlimit","getrusage","sysinfo",
          "times","getuid","getgid","setuid","setgid",
          "geteuid","getegid","getppid","getpgrp","setsid",
          "getgroups","setgroups","sigaltstack","madvise",
          "shmget","shmat","shmctl","shmdt",
          "clock_gettime","clock_getres","clock_nanosleep",
          "exit_group","epoll_wait","epoll_ctl","tgkill",
          "openat","mkdirat","fchownat","fstatat",
          "unlinkat","renameat","linkat","symlinkat",
          "readlinkat","fchmodat","faccessat","futex",
          "set_robust_list","get_robust_list",
          "epoll_create1","pipe2","dup3","accept4",
          "prlimit64","getrandom","memfd_create",
          "statx","clone3","close_range","rseq",
          "newfstatat","accept","shutdown","fchmod",
          "rt_sigaction","rt_sigprocmask","rt_sigreturn"
        ],
        "action": "SCMP_ACT_ALLOW"
      }]
    },
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
  if [ ! -f "${BUNDLE_DIR}/.extracted" ]; then
    tar --overwrite -xzf "${ROOTFS_TAR}" -C "${BUNDLE_DIR}/rootfs"
    touch "${BUNDLE_DIR}/.extracted"
  fi
  write_config
}

prepare_overlay_usr() {
  mkdir -p "${MERGED_USR}"
  if mountpoint -q "${MERGED_USR}" 2>/dev/null; then
    OVERLAY_OK=true
    return 0
  fi
  # Read-only overlay: rootfs /usr (priority) + host /usr (fallback)
  # Rootfs Python 3.11 (Debian glibc) must take precedence over any
  # host Python.  Host-only libraries (Tizen CAPI .so) are accessible
  # from the host layer.
  if mount -t overlay overlay \
       -o "lowerdir=${BUNDLE_DIR}/rootfs/usr:/usr" \
       "${MERGED_USR}" 2>/dev/null; then
    echo "OverlayFS mounted: rootfs/usr + /usr -> merged_usr"
    OVERLAY_OK=true
  else
    echo "OverlayFS unavailable, rootfs /usr used as-is"
    OVERLAY_OK=false
  fi
}

cleanup_overlay_usr() {
  if mountpoint -q "${MERGED_USR}" 2>/dev/null; then
    umount "${MERGED_USR}" 2>/dev/null || true
  fi
}

run_without_container() {
  echo "Watchdog cgroup unavailable. Falling back to chroot with unshare."

  local R="${BUNDLE_DIR}/rootfs"

  mkdir -p "$R/skills" "$R/proc" "$R/dev" "$R/tmp" \
           "$R/usr" "$R/etc" "$R/opt/etc" \
           "$R/host_lib" "$R/host_usr_lib" "$R/host_usr_lib64" \
           "$R/run" "$R/data" "${APP_DATA_DIR}/data"

  # Build the mount + chroot command as a single string for unshare
  local CMD="mount --make-rprivate / || true"
  CMD="$CMD; mount -t proc proc \"$R/proc\" || true"
  CMD="$CMD; mount --rbind /dev \"$R/dev\" || true"
  CMD="$CMD; mount --rbind \"${APP_DATA_DIR}/tools/skills\" \"$R/skills\" || true"
  CMD="$CMD; mount --rbind \"${APP_DATA_DIR}/data\" \"$R/data\" || true"
  CMD="$CMD; mount --rbind /tmp \"$R/tmp\" || true"

  # Mount /usr (overlay or rootfs-direct)
  if [ "${OVERLAY_OK}" = "true" ]; then
    CMD="$CMD; mount --rbind \"${MERGED_USR}\" \"$R/usr\" || true"
    CMD="$CMD; mount -o remount,bind,ro \"$R/usr\" || true"
  else
    # No overlay: mount host CAPI libs at separate paths
    CMD="$CMD; mount --rbind /usr/lib \"$R/host_usr_lib\" 2>/dev/null || true"
    CMD="$CMD; mount --rbind /usr/lib64 \"$R/host_usr_lib64\" 2>/dev/null || true"
  fi

  CMD="$CMD; mount --rbind /etc \"$R/etc\" || true"
  CMD="$CMD; mount -o remount,bind,ro \"$R/etc\" || true"
  CMD="$CMD; mount --rbind /opt/etc \"$R/opt/etc\" || true"
  CMD="$CMD; mount -o remount,bind,ro \"$R/opt/etc\" || true"
  CMD="$CMD; mount --rbind /lib \"$R/host_lib\" || true"
  CMD="$CMD; mount -o remount,bind,ro \"$R/host_lib\" || true"

  if [ -d /lib64 ]; then
    mkdir -p "$R/lib64"
    CMD="$CMD; mount --rbind /lib64 \"$R/lib64\" || true"
    CMD="$CMD; mount -o remount,bind,ro \"$R/lib64\" || true"
  fi

  CMD="$CMD; mount --rbind /run \"$R/run\" || true"

  # CLI tools
  mkdir -p "$R/opt/usr/share/tizenclaw/tools/cli"
  CMD="$CMD; mount --rbind \"${APP_DATA_DIR}/tools/cli\" \"$R/opt/usr/share/tizenclaw/tools/cli\" || true"
  CMD="$CMD; mount -o remount,bind,ro \"$R/opt/usr/share/tizenclaw/tools/cli\" || true"

  CMD="$CMD; exec chroot \"$R\" /bin/sh -c 'LD_LIBRARY_PATH=/usr/lib:/usr/lib64:/host_lib:/host_usr_lib:/host_usr_lib64:/lib64 exec python3 /skills/skill_executor.py'"

  exec unshare -m /bin/sh -c "$CMD"
}

start_container() {
  if [ -z "${RUNTIME_BIN}" ]; then
    echo "No OCI runtime found (crun/runc)" >&2
    return 1
  fi
  prepare_bundle
  prepare_overlay_usr
  "${RUNTIME_BIN}" delete -f "${CONTAINER_ID}" >/dev/null 2>&1 || true

  cd "${BUNDLE_DIR}"
  if [[ "$(basename "${RUNTIME_BIN}")" == "crun" ]]; then
    # Check if watchdog cgroup is accessible; if not and no runc, use chroot fallback
    if { [ ! -d "/sys/fs/cgroup/watchdog" ] || [ ! -w "/sys/fs/cgroup/watchdog" ]; } \
      && ! command -v runc >/dev/null 2>&1; then
      run_without_container
    fi
    # Try --cgroup-manager=disabled if supported
    if "${RUNTIME_BIN}" run --help 2>&1 | grep -q -- "--cgroup-manager"; then
      exec "${RUNTIME_BIN}" run --cgroup-manager=disabled "${CONTAINER_ID}"
    else
      # crun doesn't support --cgroup-manager; try runc if available
      if command -v runc >/dev/null 2>&1; then
        echo "crun does not support --cgroup-manager, switching to runc"
        exec runc run "${CONTAINER_ID}"
      else
        echo "crun does not support --cgroup-manager, fallback to unshare+chroot"
        run_without_container
      fi
    fi
  else
    exec "${RUNTIME_BIN}" run "${CONTAINER_ID}"
  fi
}

stop_container() {
  if [ -z "${RUNTIME_BIN}" ]; then
    cleanup_overlay_usr
    return 0
  fi
  "${RUNTIME_BIN}" delete -f "${CONTAINER_ID}" >/dev/null 2>&1 || true
  cleanup_overlay_usr
}

status_container() {
  if [ -z "${RUNTIME_BIN}" ]; then
    echo "runtime-missing"
    return 1
  fi
  "${RUNTIME_BIN}" state "${CONTAINER_ID}" >/dev/null 2>&1
}

ACTION="${1:-start}"
case "${ACTION}" in
  start)
    start_container
    ;;
  stop)
    stop_container
    ;;
  restart)
    stop_container
    start_container
    ;;
  status)
    status_container
    ;;
  *)
    echo "Usage: $0 {start|stop|restart|status}" >&2
    exit 2
    ;;
esac
