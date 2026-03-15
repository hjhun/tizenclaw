---
description: Deploy RPM to Tizen Emulator over sdb
---

# Deploy TizenClaw to Emulator

This workflow automates the process of installing (updating) the RPM package generated via `gbs build` to a locally connected Tizen Emulator and restarting the daemon.

Basic Prerequisites:
- A Tizen Emulator or real device must be turned on. (Check with `sdb devices`)
- The device architecture is automatically detected via the `cpu_arch` field of `sdb capability`. (e.g., `x86_64`, `aarch64`)
- `gbs build -A <arch>` (or with the `--include-all` option) must have completed successfully, and the target RPM must exist in `~/GBS-ROOT/local/repos/tizen/<arch>/RPMS/`.

Execute the following steps in order.

1. **Obtain Root Privileges**
   ```bash
   sdb root on
   ```

2. **Remount Root File System in Read/Write Mode**
   The `/` (root partition) of the emulator is read-only by default. Change it to write mode to install the RPM.
   ```bash
   sdb shell mount -o remount,rw /
   ```

3. **Push and Install the RPM File**
   Transfer the latest built TizenClaw package to the device's `/tmp/` path and force install it.
   ```bash
   # Automatically detect architecture
   ARCH=$(sdb capability 2>/dev/null | grep '^cpu_arch:' | cut -d':' -f2)
   [ -z "${ARCH}" ] && ARCH=x86_64

   # Transfer build artifacts and install
   sdb push ~/GBS-ROOT/local/repos/tizen/${ARCH}/RPMS/tizenclaw-1.0.0-1.${ARCH}.rpm /tmp/
   sdb shell rpm -Uvh --force /tmp/tizenclaw-1.0.0-1.${ARCH}.rpm
   ```

4. **Restart Daemon and Check Status**
   Load the newly installed systemd daemon and restart it.
   ```bash
   sdb shell systemctl daemon-reload
   sdb shell systemctl restart tizenclaw
   sdb shell systemctl status tizenclaw -l
   ```

// turbo-all
