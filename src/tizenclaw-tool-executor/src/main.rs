//! tizenclaw-tool-executor — Asynchronous tool execution daemon.
//!
//! Supports the same length-prefixed JSON protocol over:
//! - abstract namespace Unix domain sockets
//! - systemd socket activation
//! - stdio pipes (`--stdio`) for subprocess fallback mode

mod peer_validator;

use serde_json::{json, Value};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::process::Command;
use tokio::sync::Mutex;

const SOCKET_NAME: &str = "tizenclaw-tool-executor.sock";
const MAX_PAYLOAD: usize = 10 * 1024 * 1024;

fn default_tools_dir() -> std::path::PathBuf {
    if let Ok(path) = std::env::var("TIZENCLAW_TOOLS_DIR") {
        return std::path::PathBuf::from(path);
    }
    if std::path::Path::new("/etc/tizen-release").exists()
        || std::path::Path::new("/opt/usr/share/tizenclaw").exists()
    {
        return std::path::PathBuf::from("/opt/usr/share/tizenclaw/tools");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    std::path::PathBuf::from(home).join(".tizenclaw/tools")
}

async fn send_payload<W: AsyncWrite + Unpin>(writer: &mut W, resp: &Value) -> bool {
    let payload = resp.to_string();
    let len = (payload.len() as u32).to_be_bytes();
    if writer.write_all(&len).await.is_err() {
        return false;
    }
    if writer.write_all(payload.as_bytes()).await.is_err() {
        return false;
    }
    writer.flush().await.is_ok()
}

async fn recv_payload<R: AsyncRead + Unpin>(reader: &mut R) -> Option<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    if reader.read_exact(&mut len_buf).await.is_err() {
        return None;
    }
    let payload_len = u32::from_be_bytes(len_buf) as usize;
    if payload_len > MAX_PAYLOAD {
        log::error!("Payload too large: {}", payload_len);
        return None;
    }
    let mut buf = vec![0u8; payload_len];
    if reader.read_exact(&mut buf).await.is_err() {
        return None;
    }
    Some(buf)
}

fn resolve_binary_path(tool_name: &str) -> Option<String> {
    let bin_path = if tool_name.starts_with('/') {
        tool_name.to_string()
    } else {
        let tools_dir = default_tools_dir();
        let nested_cli_path = tools_dir.join("cli").join(tool_name).join(tool_name);
        let flat_cli_path = tools_dir.join("cli").join(tool_name);
        if nested_cli_path.exists() {
            nested_cli_path.to_string_lossy().to_string()
        } else if flat_cli_path.exists() {
            flat_cli_path.to_string_lossy().to_string()
        } else {
            format!("/usr/bin/{}", tool_name)
        }
    };

    std::path::Path::new(&bin_path).exists().then_some(bin_path)
}

async fn run_protocol_session<R, W>(mut reader: R, writer: W)
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let payload = match recv_payload(&mut reader).await {
        Some(p) => p,
        None => return,
    };

    let req: Value = match serde_json::from_slice(&payload) {
        Ok(v) => v,
        Err(e) => {
            let mut writer = writer;
            let _ = send_payload(
                &mut writer,
                &json!({"status": "error", "message": format!("Bad JSON: {}", e)}),
            )
            .await;
            return;
        }
    };

    let command = req["command"].as_str().unwrap_or("");
    if command != "execute" {
        let mut writer = writer;
        let _ = send_payload(
            &mut writer,
            &json!({"status": "error", "message": "Expected 'execute' command as first payload"}),
        )
        .await;
        return;
    }

    let tool_name = req["tool_name"].as_str().unwrap_or("");
    let mode = req["mode"].as_str().unwrap_or("oneshot");
    let cwd = req["cwd"].as_str().filter(|value| !value.trim().is_empty());
    let mut args: Vec<String> = vec![];

    if let Some(arr) = req["args"].as_array() {
        for value in arr {
            if let Some(text) = value.as_str() {
                args.push(text.to_string());
            }
        }
    }

    if tool_name.is_empty() {
        let mut writer = writer;
        let _ = send_payload(
            &mut writer,
            &json!({"status": "error", "message": "No tool_name provided"}),
        )
        .await;
        return;
    }

    let bin_path = match resolve_binary_path(tool_name) {
        Some(path) => path,
        None => {
            let mut writer = writer;
            let _ = send_payload(
                &mut writer,
                &json!({"status": "error", "message": format!("CLI binary not found: {}", tool_name)}),
            )
            .await;
            return;
        }
    };

    log::debug!("Executing [{}]: {} {:?} cwd={:?}", mode, bin_path, args, cwd);

    let mut command = Command::new(&bin_path);
    command
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }

    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => {
            let mut writer = writer;
            let _ = send_payload(
                &mut writer,
                &json!({"status": "error", "message": format!("Spawn failed: {}", e)}),
            )
            .await;
            return;
        }
    };

    let mut stdout = child.stdout.take().expect("Failed to grab stdout");
    let mut stderr = child.stderr.take().expect("Failed to grab stderr");
    let stdin_opt = child.stdin.take();

    let writer = Arc::new(Mutex::new(writer));

    let stdout_writer = writer.clone();
    let stdout_task = tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            match stdout.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                    let mut lock = stdout_writer.lock().await;
                    let _ =
                        send_payload(&mut *lock, &json!({"event": "stdout", "data": chunk})).await;
                }
                Err(_) => break,
            }
        }
    });

    let stderr_writer = writer.clone();
    let stderr_task = tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            match stderr.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).to_string();
                    let mut lock = stderr_writer.lock().await;
                    let _ =
                        send_payload(&mut *lock, &json!({"event": "stderr", "data": chunk})).await;
                }
                Err(_) => break,
            }
        }
    });

    let stdin_task = if mode == "interactive" {
        if let Some(mut stdin) = stdin_opt {
            tokio::spawn(async move {
                loop {
                    let payload = match recv_payload(&mut reader).await {
                        Some(value) => value,
                        None => break,
                    };

                    let req: Value = match serde_json::from_slice(&payload) {
                        Ok(value) => value,
                        Err(_) => break,
                    };

                    if req["command"] == "stdin" {
                        if let Some(data) = req["data"].as_str() {
                            if stdin.write_all(data.as_bytes()).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            })
        } else {
            tokio::spawn(async {})
        }
    } else {
        tokio::spawn(async {})
    };

    let exit_status = match child.wait().await {
        Ok(status) => status,
        Err(e) => {
            log::error!("Failed to wait on child: {}", e);
            return;
        }
    };

    let _ = tokio::join!(stdout_task, stderr_task, stdin_task);

    let mut lock = writer.lock().await;
    let _ = send_payload(
        &mut *lock,
        &json!({"event": "exit", "code": exit_status.code().unwrap_or(-1)}),
    )
    .await;
}

async fn handle_socket_client(stream: UnixStream) {
    log::debug!("New client connection");

    if !peer_validator::validate(&stream, &["tizenclaw", "tizenclaw-cli"]) {
        log::warn!("Rejecting unauthenticated peer");
        let (_reader, mut writer) = stream.into_split();
        let _ = send_payload(
            &mut writer,
            &json!({"status": "error", "message": "Permission denied: caller not authorized"}),
        )
        .await;
        return;
    }

    let (reader, writer) = stream.into_split();
    run_protocol_session(reader, writer).await;
}

async fn run_stdio_server() {
    log::info!("Starting stdio executor mode");
    run_protocol_session(tokio::io::stdin(), tokio::io::stdout()).await;
}

#[tokio::main]
async fn main() {
    struct PlatformLogger;
    impl log::Log for PlatformLogger {
        fn enabled(&self, _: &log::Metadata) -> bool {
            true
        }

        fn log(&self, record: &log::Record) {
            let is_tizen = std::fs::read_to_string("/etc/os-release")
                .map(|s| s.to_lowercase().contains("tizen"))
                .unwrap_or(false);

            let filepath = record.file().unwrap_or("?");
            let filename = filepath
                .rsplit('/')
                .next()
                .unwrap_or(filepath)
                .rsplit('\\')
                .next()
                .unwrap_or(filepath);
            let msg = format!(
                "{}:{} [{}] {}",
                filename,
                record.line().unwrap_or(0),
                record.level(),
                record.args()
            );

            if is_tizen {
                let prio = match record.level() {
                    log::Level::Error => libtizenclaw_core::tizen_sys::dlog::DLOG_ERROR,
                    log::Level::Warn => libtizenclaw_core::tizen_sys::dlog::DLOG_WARN,
                    log::Level::Info => libtizenclaw_core::tizen_sys::dlog::DLOG_INFO,
                    log::Level::Debug | log::Level::Trace => {
                        libtizenclaw_core::tizen_sys::dlog::DLOG_DEBUG
                    }
                };
                let tag_c = std::ffi::CString::new("TIZENCLAW_EXEC").unwrap();
                let msg_c = std::ffi::CString::new(msg.replace("%", "%%"))
                    .unwrap_or_else(|_| std::ffi::CString::new("Log error").unwrap());
                unsafe {
                    libtizenclaw_core::tizen_sys::dlog::dlog_print(
                        prio,
                        tag_c.as_ptr(),
                        msg_c.as_ptr(),
                    );
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

    if std::env::args().any(|arg| arg == "--stdio") {
        run_stdio_server().await;
        return;
    }

    log::info!(
        "tizenclaw-tool-executor starting (pid={})",
        std::process::id()
    );

    let listener = match systemd_socket() {
        Some(listener) => listener,
        None => create_abstract_socket().expect("Failed to create socket"),
    };

    log::info!("Listening on abstract socket: @{}", SOCKET_NAME);

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(handle_socket_client(stream));
            }
            Err(e) => {
                log::error!("accept() failed: {}", e);
                break;
            }
        }
    }

    log::info!("tizenclaw-tool-executor stopped");
}

fn systemd_socket() -> Option<UnixListener> {
    use std::os::unix::io::FromRawFd;
    let listen_fds = std::env::var("LISTEN_FDS").ok()?;
    let listen_pid = std::env::var("LISTEN_PID").ok()?;
    let pid: u32 = listen_pid.parse().ok()?;
    let fds: i32 = listen_fds.parse().ok()?;

    if pid == std::process::id() && fds >= 1 {
        log::debug!("Using systemd socket activation (fd=3)");
        let std_listener = unsafe { std::os::unix::net::UnixListener::from_raw_fd(3) };
        std_listener.set_nonblocking(true).ok()?;
        UnixListener::from_std(std_listener).ok()
    } else {
        None
    }
}

fn create_abstract_socket() -> std::io::Result<UnixListener> {
    use std::os::unix::io::FromRawFd;

    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }

    let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
    let name_bytes = SOCKET_NAME.as_bytes();
    addr.sun_path[1..1 + name_bytes.len()].copy_from_slice(unsafe {
        std::slice::from_raw_parts(name_bytes.as_ptr() as *const libc::c_char, name_bytes.len())
    });

    let addr_len =
        (std::mem::size_of::<libc::sa_family_t>() + 1 + name_bytes.len()) as libc::socklen_t;

    let ret = unsafe {
        libc::bind(
            fd,
            &addr as *const libc::sockaddr_un as *const libc::sockaddr,
            addr_len,
        )
    };
    if ret < 0 {
        unsafe { libc::close(fd) };
        return Err(std::io::Error::last_os_error());
    }

    let ret = unsafe { libc::listen(fd, 128) };
    if ret < 0 {
        unsafe { libc::close(fd) };
        return Err(std::io::Error::last_os_error());
    }

    let opts = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    unsafe { libc::fcntl(fd, libc::F_SETFL, opts | libc::O_NONBLOCK) };

    let std_listener = unsafe { std::os::unix::net::UnixListener::from_raw_fd(fd) };
    std_listener.set_nonblocking(true)?;
    UnixListener::from_std(std_listener)
}
