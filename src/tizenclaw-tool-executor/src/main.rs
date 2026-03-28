//! tizenclaw-tool-executor — Sandboxed tool execution daemon.
//!
//! Listens on an abstract namespace Unix domain socket and executes
//! tool scripts on the host Linux. Shell code is run via subprocess.
//!
//! Protocol: 4-byte big-endian length prefix + UTF-8 JSON body
//! Security: SO_PEERCRED validates peer is tizenclaw or tizenclaw-cli.

mod peer_validator;

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};

const SOCKET_NAME: &str = "tizenclaw-tool-executor.sock";
const MAX_PAYLOAD: usize = 10 * 1024 * 1024;
const CODE_EXEC_TIMEOUT: u64 = 15;

// ═══════════════════════════════════════════
//  Socket I/O helpers
// ═══════════════════════════════════════════

fn recv_exact(stream: &mut UnixStream, buf: &mut [u8]) -> bool {
    let mut total = 0;
    while total < buf.len() {
        match stream.read(&mut buf[total..]) {
            Ok(0) => return false,
            Ok(n) => total += n,
            Err(_) => return false,
        }
    }
    true
}

fn send_response(stream: &mut UnixStream, resp: &Value) -> bool {
    let payload = resp.to_string();
    let len = (payload.len() as u32).to_be_bytes();
    if stream.write_all(&len).is_err() {
        return false;
    }
    stream.write_all(payload.as_bytes()).is_ok()
}

// ═══════════════════════════════════════════
//  Command Handlers
// ═══════════════════════════════════════════

fn handle_diag() -> Value {
    json!({
        "status": "ok",
        "output": format!("tool-executor alive, pid={}", std::process::id())
    })
}

fn handle_execute_code(code: &str, timeout: u64) -> Value {
    if code.is_empty() {
        return json!({"status": "error", "output": "No code provided"});
    }

    let result = Command::new("sh")
        .args(["-c", code])
        .output();

    match result {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let exit_code = output.status.code().unwrap_or(-1);
            let combined = if stderr.is_empty() {
                stdout.trim().to_string()
            } else {
                format!("{}\n{}", stdout.trim(), stderr.trim())
            };
            json!({
                "status": if exit_code == 0 { "ok" } else { "error" },
                "output": combined,
                "exit_code": exit_code,
                "timeout": timeout,
            })
        }
        Err(e) => json!({"status": "error", "output": format!("Failed to execute: {}", e)}),
    }
}

fn handle_execute_cli(tool_name: &str, arguments: &str, timeout: u64) -> Value {
    if tool_name.is_empty() {
        return json!({"status": "error", "output": "No tool_name"});
    }

    let bin_path = if tool_name.starts_with('/') {
        tool_name.to_string()
    } else {
        format!("/usr/bin/{}", tool_name)
    };

    if !std::path::Path::new(&bin_path).exists() {
        return json!({"status": "error", "output": format!("CLI binary not found: {}", bin_path)});
    }

    let cmd_str = format!("{} {} 2>&1", bin_path, arguments);
    let result = Command::new("sh")
        .args(["-c", &cmd_str])
        .output();

    match result {
        Ok(output) => {
            let out_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let exit_code = output.status.code().unwrap_or(-1);

            // Try to parse output as JSON
            let result_json = match serde_json::from_str::<Value>(&out_str) {
                Ok(mut v) => {
                    v["exit_code"] = json!(exit_code);
                    v
                }
                Err(_) => json!({
                    "tool": tool_name,
                    "output": out_str,
                    "exit_code": exit_code,
                    "timeout": timeout,
                }),
            };

            json!({"status": "ok", "output": result_json.to_string()})
        }
        Err(e) => json!({"status": "error", "output": format!("popen failed: {}", e)}),
    }
}

fn handle_install_package(pkg_type: &str, _name: &str) -> Value {
    json!({"status": "error", "output": format!("Package installation not supported in pure-shell mode: {}", pkg_type)})
}

fn handle_tool(tool: &str, args_str: &str) -> Value {
    if tool.is_empty() {
        return json!({"status": "error", "output": "No tool specified"});
    }

    // Look for tool script in skills directory
    let skills_dir = "/opt/usr/share/tizen-tools/skills";
    let script_path = format!("{}/{}/run.sh", skills_dir, tool);

    if !std::path::Path::new(&script_path).exists() {
        return json!({
            "status": "error",
            "output": format!("Tool script not found: {}", script_path)
        });
    }

    let cmd = format!("bash {} '{}' 2>&1", script_path, args_str);
    match Command::new("sh").args(["-c", &cmd]).output() {
        Ok(output) => {
            let out_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let exit_code = output.status.code().unwrap_or(-1);
            json!({
                "status": if exit_code == 0 { "ok" } else { "error" },
                "output": out_str,
                "exit_code": exit_code,
            })
        }
        Err(e) => json!({"status": "error", "output": format!("Tool exec failed: {}", e)}),
    }
}

// ═══════════════════════════════════════════
//  Client handler
// ═══════════════════════════════════════════

fn handle_client(mut stream: UnixStream) {
    log::debug!("New client connection");

    if !peer_validator::validate(&stream, &["tizenclaw", "tizenclaw-cli"]) {
        log::warn!("Rejecting unauthenticated peer");
        let resp = json!({"status": "error", "output": "Permission denied: caller not authorized"});
        let _ = send_response(&mut stream, &resp);
        return;
    }

    loop {
        let mut len_buf = [0u8; 4];
        if !recv_exact(&mut stream, &mut len_buf) {
            break;
        }

        let payload_len = u32::from_be_bytes(len_buf) as usize;
        if payload_len > MAX_PAYLOAD {
            log::error!("Payload too large: {}", payload_len);
            let _ = send_response(&mut stream, &json!({"status": "error", "output": "Payload too large"}));
            break;
        }

        let mut buf = vec![0u8; payload_len];
        if !recv_exact(&mut stream, &mut buf) {
            break;
        }

        let req: Value = match serde_json::from_slice(&buf) {
            Ok(v) => v,
            Err(e) => {
                let _ = send_response(&mut stream, &json!({"status": "error", "output": format!("Bad JSON: {}", e)}));
                continue;
            }
        };

        let command = req["command"].as_str().unwrap_or("");
        log::info!("Command: {}", command);

        let resp = match command {
            "diag" => handle_diag(),
            "execute_code" => {
                let code = req["code"].as_str().unwrap_or("");
                let timeout = req["timeout"].as_u64().unwrap_or(CODE_EXEC_TIMEOUT);
                handle_execute_code(code, timeout)
            }
            "execute_cli" => {
                let tool_name = req["tool_name"].as_str().unwrap_or("");
                let arguments = req["arguments"].as_str().unwrap_or("");
                let timeout = req["timeout"].as_u64().unwrap_or(10);
                handle_execute_cli(tool_name, arguments, timeout)
            }
            "install_package" => {
                let pkg_type = req["type"].as_str().unwrap_or("pip");
                let name = req["name"].as_str().unwrap_or("");
                handle_install_package(pkg_type, name)
            }
            _ => {
                // Default: tool execution
                let tool = req["tool"].as_str().unwrap_or("");
                let args = req["args"].as_str().unwrap_or("{}");
                handle_tool(tool, args)
            }
        };

        if !send_response(&mut stream, &resp) {
            break;
        }
    }

    log::debug!("Client disconnected");
}

// ═══════════════════════════════════════════
//  Main
// ═══════════════════════════════════════════

fn main() {
    // Simple stderr logger
    struct StderrLogger;
    impl log::Log for StderrLogger {
        fn enabled(&self, _: &log::Metadata) -> bool { true }
        fn log(&self, record: &log::Record) {
            eprintln!("[{}] {}", record.level(), record.args());
        }
        fn flush(&self) {}
    }
    static LOGGER: StderrLogger = StderrLogger;
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);

    log::info!(
        "tizenclaw-tool-executor starting (pid={})",
        std::process::id()
    );

    // Handle signals
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc_handler(r);

    // Check systemd socket activation
    let listener = match systemd_socket() {
        Some(l) => l,
        None => create_abstract_socket().expect("Failed to create socket"),
    };

    log::info!("Listening on abstract socket: @{}", SOCKET_NAME);

    // Set non-blocking for shutdown polling
    listener
        .set_nonblocking(true)
        .expect("Failed to set non-blocking");

    while running.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                std::thread::spawn(move || handle_client(stream));
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Err(e) => {
                log::error!("accept() failed: {}", e);
                break;
            }
        }
    }

    log::info!("tizenclaw-tool-executor stopped");
}

// ═══════════════════════════════════════════
//  Socket setup
// ═══════════════════════════════════════════

fn systemd_socket() -> Option<UnixListener> {
    let listen_fds = std::env::var("LISTEN_FDS").ok()?;
    let listen_pid = std::env::var("LISTEN_PID").ok()?;
    let pid: u32 = listen_pid.parse().ok()?;
    let fds: i32 = listen_fds.parse().ok()?;

    if pid == std::process::id() && fds >= 1 {
        use std::os::unix::io::FromRawFd;
        log::info!("Using systemd socket activation (fd=3)");
        Some(unsafe { UnixListener::from_raw_fd(3) })
    } else {
        None
    }
}

fn create_abstract_socket() -> std::io::Result<UnixListener> {
    use std::os::unix::io::FromRawFd;

    let fd = unsafe {
        libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0)
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }

    let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
    // Abstract namespace: sun_path[0] = 0, followed by name
    let name_bytes = SOCKET_NAME.as_bytes();
    addr.sun_path[1..1 + name_bytes.len()]
        .copy_from_slice(unsafe {
            std::slice::from_raw_parts(
                name_bytes.as_ptr() as *const libc::c_char,
                name_bytes.len(),
            )
        });

    let addr_len = (std::mem::size_of::<libc::sa_family_t>() + 1 + name_bytes.len()) as libc::socklen_t;

    let ret = unsafe {
        libc::bind(fd, &addr as *const libc::sockaddr_un as *const libc::sockaddr, addr_len)
    };
    if ret < 0 {
        unsafe { libc::close(fd); }
        return Err(std::io::Error::last_os_error());
    }

    let ret = unsafe { libc::listen(fd, 128) };
    if ret < 0 {
        unsafe { libc::close(fd); }
        return Err(std::io::Error::last_os_error());
    }

    Ok(unsafe { UnixListener::from_raw_fd(fd) })
}

fn ctrlc_handler(running: Arc<AtomicBool>) {
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }
    // Simple SIGTERM/SIGINT handler via thread
    std::thread::spawn(move || {
        let mut sigset: libc::sigset_t = unsafe { std::mem::zeroed() };
        unsafe {
            libc::sigemptyset(&mut sigset);
            libc::sigaddset(&mut sigset, libc::SIGTERM);
            libc::sigaddset(&mut sigset, libc::SIGINT);
            libc::pthread_sigmask(libc::SIG_BLOCK, &sigset, std::ptr::null_mut());

            let mut sig = 0;
            libc::sigwait(&sigset, &mut sig);
        }
        running.store(false, Ordering::SeqCst);
    });
}
