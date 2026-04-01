//! tizenclaw-tool-executor — Asynchronous tool execution daemon.
//!
//! Listens on an abstract namespace Unix domain socket and executes
//! tool scripts on the host Linux.
//! Supports oneshot, streaming, and interactive modes via multiplexed JSON.
//!
//! Protocol: 4-byte big-endian length prefix + UTF-8 JSON body

mod peer_validator;

use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::process::Command;
use tokio::sync::Mutex;
use std::process::Stdio;

const SOCKET_NAME: &str = "tizenclaw-tool-executor.sock";
const MAX_PAYLOAD: usize = 10 * 1024 * 1024;

// ═══════════════════════════════════════════
//  Socket I/O helpers
// ═══════════════════════════════════════════

async fn send_response(stream: &mut UnixStream, resp: &Value) -> bool {
    let payload = resp.to_string();
    let len = (payload.len() as u32).to_be_bytes();
    if stream.write_all(&len).await.is_err() {
        return false;
    }
    stream.write_all(payload.as_bytes()).await.is_ok()
}

async fn recv_payload(stream: &mut UnixStream) -> Option<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    if stream.read_exact(&mut len_buf).await.is_err() {
        return None;
    }
    let payload_len = u32::from_be_bytes(len_buf) as usize;
    if payload_len > MAX_PAYLOAD {
        log::error!("Payload too large: {}", payload_len);
        return None;
    }
    let mut buf = vec![0u8; payload_len];
    if stream.read_exact(&mut buf).await.is_err() {
        return None;
    }
    Some(buf)
}

// ═══════════════════════════════════════════
//  Handler
// ═══════════════════════════════════════════

async fn handle_client(mut stream: UnixStream) {
    log::debug!("New client connection");

    if !peer_validator::validate(&stream, &["tizenclaw", "tizenclaw-cli"]) {
        log::warn!("Rejecting unauthenticated peer");
        let resp = json!({"status": "error", "message": "Permission denied: caller not authorized"});
        let _ = send_response(&mut stream, &resp).await;
        return;
    }

    // Read initial execution command
    let payload = match recv_payload(&mut stream).await {
        Some(p) => p,
        None => return,
    };

    let req: Value = match serde_json::from_slice(&payload) {
        Ok(v) => v,
        Err(e) => {
            let _ = send_response(&mut stream, &json!({"status": "error", "message": format!("Bad JSON: {}", e)})).await;
            return;
        }
    };

    let command = req["command"].as_str().unwrap_or("");
    if command != "execute" {
        let _ = send_response(&mut stream, &json!({"status": "error", "message": "Expected 'execute' command as first payload"})).await;
        return;
    }

    let tool_name = req["tool_name"].as_str().unwrap_or("");
    let mode = req["mode"].as_str().unwrap_or("oneshot"); // oneshot, streaming, interactive
    let mut args: Vec<String> = vec![];

    if let Some(arr) = req["args"].as_array() {
        for a in arr {
            if let Some(s) = a.as_str() {
                args.push(s.to_string());
            }
        }
    }

    if tool_name.is_empty() {
        let _ = send_response(&mut stream, &json!({"status": "error", "message": "No tool_name provided"})).await;
        return;
    }

    // Resolve binary path
    let bin_path = if tool_name.starts_with('/') {
        tool_name.to_string()
    } else {
        let cli_path = format!("/opt/usr/share/tizen-tools/cli/{}", tool_name);
        if std::path::Path::new(&cli_path).exists() {
            cli_path
        } else {
            format!("/usr/bin/{}", tool_name)
        }
    };

    if !std::path::Path::new(&bin_path).exists() {
        let _ = send_response(&mut stream, &json!({"status": "error", "message": format!("CLI binary not found: {}", bin_path)})).await;
        return;
    }

    log::info!("Executing [{}]: {} {:?}", mode, bin_path, args);

    let mut child = match Command::new(&bin_path)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = send_response(&mut stream, &json!({"status": "error", "message": format!("Spawn failed: {}", e)})).await;
            return;
        }
    };

    let mut stdout = child.stdout.take().expect("Failed to grab stdout");
    let mut stderr = child.stderr.take().expect("Failed to grab stderr");
    let mut stdin_opt = child.stdin.take();

    let (mut rx, mut tx) = stream.into_split();
    let tx_mutex = Arc::new(Mutex::new(tx));
    
    let stdout_tx = tx_mutex.clone();
    let stdout_task = tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            match stdout.read(&mut buf).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                    let resp = json!({"event": "stdout", "data": chunk});
                    let mut lock = stdout_tx.lock().await;
                    let payload = resp.to_string();
                    let len = (payload.len() as u32).to_be_bytes();
                    let _ = lock.write_all(&len).await;
                    let _ = lock.write_all(payload.as_bytes()).await;
                }
                Err(_) => break,
            }
        }
    });

    let stderr_tx = tx_mutex.clone();
    let stderr_task = tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            match stderr.read(&mut buf).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                    let resp = json!({"event": "stderr", "data": chunk});
                    let mut lock = stderr_tx.lock().await;
                    let payload = resp.to_string();
                    let len = (payload.len() as u32).to_be_bytes();
                    let _ = lock.write_all(&len).await;
                    let _ = lock.write_all(payload.as_bytes()).await;
                }
                Err(_) => break,
            }
        }
    });

    let stdin_task = {
        if mode == "interactive" {
            if let Some(mut stdin) = stdin_opt {
                tokio::spawn(async move {
                    loop {
                        let mut len_buf = [0u8; 4];
                        if rx.read_exact(&mut len_buf).await.is_err() { break; }
                        let payload_len = u32::from_be_bytes(len_buf) as usize;
                        if payload_len > MAX_PAYLOAD { break; }
                        let mut buf = vec![0u8; payload_len];
                        if rx.read_exact(&mut buf).await.is_err() { break; }
                        
                        if let Ok(req) = serde_json::from_slice::<Value>(&buf) {
                            if req["command"] == "stdin" {
                                if let Some(data) = req["data"].as_str() {
                                    if stdin.write_all(data.as_bytes()).await.is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                })
            } else { tokio::spawn(async {}) }
        } else { tokio::spawn(async {}) }
    };

    let exit_status = match child.wait().await {
        Ok(status) => status,
        Err(e) => {
            log::error!("Failed to wait on child: {}", e);
            return;
        }
    };
    
    let _ = tokio::join!(stdout_task, stderr_task, stdin_task);
    
    let resp = json!({
        "event": "exit",
        "code": exit_status.code().unwrap_or(-1)
    });
    
    let mut lock = tx_mutex.lock().await;
    let payload = resp.to_string();
    let len = (payload.len() as u32).to_be_bytes();
    let _ = lock.write_all(&len).await;
    let _ = lock.write_all(payload.as_bytes()).await;

    log::debug!("Client session completed");
}

// ═══════════════════════════════════════════
//  Main
// ═══════════════════════════════════════════

#[tokio::main]
async fn main() {
    // Simple conditionally dual logger
    struct PlatformLogger;
    impl log::Log for PlatformLogger {
        fn enabled(&self, _: &log::Metadata) -> bool { true }
        fn log(&self, record: &log::Record) {
            let is_tizen = std::fs::read_to_string("/etc/os-release")
                .map(|s| s.to_lowercase().contains("tizen"))
                .unwrap_or(false);

            let msg = format!("[{}] {}", record.level(), record.args());

            if is_tizen {
                let prio = match record.level() {
                    log::Level::Error => tizen_sys::dlog::DLOG_ERROR,
                    log::Level::Warn  => tizen_sys::dlog::DLOG_WARN,
                    log::Level::Info  => tizen_sys::dlog::DLOG_INFO,
                    log::Level::Debug | log::Level::Trace => tizen_sys::dlog::DLOG_DEBUG,
                };
                let tag_c = std::ffi::CString::new("TIZENCLAW_EXEC").unwrap();
                let msg_c = std::ffi::CString::new(msg.replace("%", "%%")).unwrap_or_else(|_| std::ffi::CString::new("Log error").unwrap());
                unsafe {
                    tizen_sys::dlog::dlog_print(prio, tag_c.as_ptr(), msg_c.as_ptr());
                }
            } else {
                eprintln!("{}", msg);
            }
        }
        fn flush(&self) {}
    }
    static LOGGER: PlatformLogger = PlatformLogger;
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);

    log::info!("tizenclaw-tool-executor starting (pid={})", std::process::id());

    // Check systemd socket activation
    let listener = match systemd_socket() {
        Some(l) => l,
        None => create_abstract_socket().expect("Failed to create socket"),
    };

    log::info!("Listening on abstract socket: @{}", SOCKET_NAME);

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(handle_client(stream));
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
    use std::os::unix::io::FromRawFd;
    let listen_fds = std::env::var("LISTEN_FDS").ok()?;
    let listen_pid = std::env::var("LISTEN_PID").ok()?;
    let pid: u32 = listen_pid.parse().ok()?;
    let fds: i32 = listen_fds.parse().ok()?;

    if pid == std::process::id() && fds >= 1 {
        log::info!("Using systemd socket activation (fd=3)");
        let std_listener = unsafe { std::os::unix::net::UnixListener::from_raw_fd(3) };
        std_listener.set_nonblocking(true).ok()?;
        UnixListener::from_std(std_listener).ok()
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

    let opts = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    unsafe { libc::fcntl(fd, libc::F_SETFL, opts | libc::O_NONBLOCK); }

    let std_listener = unsafe { std::os::unix::net::UnixListener::from_raw_fd(fd) };
    UnixListener::from_std(std_listener)
}
