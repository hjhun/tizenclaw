---
description: SDB Device and Emulator Development Workflow
---

# SDB (Smart Development Bridge) Development Skill

This is a workflow (Skill) used for development and debugging when connecting to a Tizen Emulator or an actual device.
Follow this document's procedures when you need to install an app on the Tizen target device or control the file system with root privileges in the terminal environment.

## 1. Check Device Connection
Check the current list of connected Tizen Emulators or devices.
```bash
sdb devices
```
* **Check Result**: Ensure that `emulator-26101` or a specific USB device ID is displayed in the `device` state. If it is `offline`, restart the emulator from the Tizen Emulator Manager.

## 2. Obtain Root Privilege (Root On)
By default, Tizen grants restricted privileges (`owner` user) for security reasons when entering the target shell.
To mount the root file system, modify system files, or set up container engines (`crun`/`lxc`), you must obtain root privileges.
```bash
sdb root on
```
*Upon success, a message like `Switched to 'root' account mode` will be output.*

## 3. Read-Write Mount the Root File System
Tizen's root file system (`/`) or core system partitions are mounted Read-Only by default. You must remount them in Read-Write (`rw`) mode to modify daemon installations or runtime dependencies.
```bash
sdb shell mount -o remount,rw /
```
Afterwards, you can enter `sdb shell` to perform necessary file modifications, gdb debugging, and container module installations with `root` privileges.

## Summary Script Execution
When the TizenClaw Agent needs to automatically set up the environment for development convenience, execute the following sequential commands (sh).

```bash
sdb devices
sdb root on
sdb shell mount -o remount,rw /
```

// turbo-all
