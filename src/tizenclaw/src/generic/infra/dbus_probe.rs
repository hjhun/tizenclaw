//! D-Bus probe — checks whether the D-Bus system bus is accessible.
//!
//! Ported from C++ DbusProbe: uses fork() to safely probe the socket,
//! guarding against libdbuspolicy1 SIGABRT crashes.

use std::sync::atomic::{AtomicU8, Ordering};

const STATE_UNTESTED: u8 = 0;
const STATE_AVAILABLE: u8 = 1;
const STATE_UNAVAILABLE: u8 = 2;

static PROBE_STATE: AtomicU8 = AtomicU8::new(STATE_UNTESTED);

const DBUS_SYSTEM_BUS_SOCKET: &str = "/run/dbus/system_bus_socket";

/// Run the probe in a child process to safely test D-Bus availability.
fn run_probe() -> bool {
    unsafe {
        let pid = libc::fork();
        if pid < 0 {
            // fork failed — assume available
            return true;
        }

        if pid == 0 {
            // Child: check socket accessibility
            let path = std::ffi::CString::new(DBUS_SYSTEM_BUS_SOCKET).unwrap();
            if libc::access(path.as_ptr(), libc::R_OK | libc::W_OK) == 0 {
                libc::_exit(0);
            }
            libc::_exit(1);
        }

        // Parent: wait for child
        let mut status: libc::c_int = 0;
        if libc::waitpid(pid, &mut status, 0) < 0 {
            return true; // waitpid error — assume ok
        }

        // Check if child was killed by signal (e.g., SIGABRT)
        if libc::WIFSIGNALED(status) {
            let sig = libc::WTERMSIG(status);
            log::warn!(
                "DbusProbe: child killed by signal {}{}",
                sig,
                if sig == 6 { " (SIGABRT)" } else { "" }
            );
            return false;
        }

        if libc::WIFEXITED(status) {
            let code = libc::WEXITSTATUS(status);
            if code != 0 {
                log::warn!("DbusProbe: D-Bus socket not accessible (exit={})", code);
                return false;
            }
            return true;
        }

        true
    }
}

/// Check if D-Bus system bus is available. Result is cached.
pub fn is_available() -> bool {
    let prev = PROBE_STATE.load(Ordering::SeqCst);
    if prev != STATE_UNTESTED {
        return prev == STATE_AVAILABLE;
    }

    // First caller runs the probe
    if PROBE_STATE.compare_exchange(STATE_UNTESTED, STATE_AVAILABLE, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
        let ok = run_probe();
        PROBE_STATE.store(
            if ok { STATE_AVAILABLE } else { STATE_UNAVAILABLE },
            Ordering::SeqCst,
        );
        log::debug!(
            "DbusProbe: D-Bus system bus {}",
            if ok { "available" } else { "unavailable" }
        );
        return ok;
    }

    // Another thread is probing — wait
    while PROBE_STATE.load(Ordering::SeqCst) == STATE_UNTESTED {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    PROBE_STATE.load(Ordering::SeqCst) == STATE_AVAILABLE
}
