//! Peer validation via SO_PEERCRED on Unix domain sockets.

use std::os::unix::io::AsRawFd;
use tokio::net::UnixStream;

/// Validate that the peer process is one of the allowed program names.
///
/// Uses SO_PEERCRED to get the peer PID, then reads /proc/{pid}/comm
/// to check the process name against the allowlist.
pub fn validate(stream: &UnixStream, allowed: &[&str]) -> bool {
    let fd = stream.as_raw_fd();

    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;

    let ret = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut libc::ucred as *mut libc::c_void,
            &mut len,
        )
    };

    if ret != 0 {
        log::warn!("SO_PEERCRED getsockopt failed");
        return false;
    }

    let comm_path = format!("/proc/{}/comm", cred.pid);
    match std::fs::read_to_string(&comm_path) {
        Ok(name) => {
            let name = name.trim();
            let ok = allowed.iter().any(|a| *a == name);
            if !ok {
                log::warn!(
                    "Peer pid={} comm='{}' not in allowed list",
                    cred.pid,
                    name
                );
            }
            ok
        }
        Err(e) => {
            log::warn!("Cannot read {}: {}", comm_path, e);
            false
        }
    }
}
