---
description: Tizen Crash Dump Debugging Workflow (sdb shell + gdb)
---

# Crash Dump Debugging Workflow

This is the procedure to debug with GDB using the crash dump file saved on the device when a crash occurs in the tizenclaw process.

## Prerequisites
- The device must be connected with `sdb devices`.
- Obtain root privileges with `sdb root on`.

## Debugging Procedure

### 1. Enter Device Shell
```bash
sdb shell
```

### 2. Move to crash dump directory
```bash
cd /opt/usr/share/crash/dump/
ls
```
- Look for a file in the format `tizenclaw_<PID>_<TIMESTAMP>.zip`.

### 3. Unzip the dump file
```bash
unzip tizenclaw_<PID>_<TIMESTAMP>.zip
cd tizenclaw_<PID>_<TIMESTAMP>
```

### 4. Extract the coredump tarball
```bash
tar -xvf tizenclaw_<PID>_<TIMESTAMP>.coredump.tar
ls
```
- The `tizenclaw_<PID>_<TIMESTAMP>.coredump` file will be created.

### 5. Start debugging with GDB
```bash
gdb /usr/bin/tizenclaw tizenclaw_<PID>_<TIMESTAMP>.coredump
```

### 6. Main GDB Commands
| Command | Description |
|---|---|
| `bt` | Print full backtrace |
| `bt full` | Backtrace including local variables |
| `info threads` | List of all threads |
| `thread apply all bt` | Backtrace for all threads |
| `frame <N>` | Move to a specific frame |
| `info registers` | Check register values |
| `quit` | Exit GDB |

## Example (Full Flow)
```bash
sdb shell
cd /opt/usr/share/crash/dump/
unzip tizenclaw_74434_20260305224907.zip
cd tizenclaw_74434_20260305224907
tar -xvf tizenclaw_74434_20260305224907.coredump.tar
gdb /usr/bin/tizenclaw tizenclaw_74434_20260305224907.coredump
```

After entering GDB:
```
(gdb) bt
(gdb) bt full
(gdb) quit
```
