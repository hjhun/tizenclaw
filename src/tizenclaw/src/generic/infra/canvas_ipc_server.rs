//! Canvas IPC server — handles IPC with WebView/Canvas overlay.
//!
//! Provides a Unix domain socket server for canvas webviews to
//! send/receive rendering commands and UI events.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const CANVAS_SOCKET_PATH: &str = "/tmp/tizenclaw-canvas.sock";

/// Canvas command from the webview.
#[derive(Debug, Clone)]
pub struct CanvasCommand {
    pub action: String,
    pub payload: String,
}

/// Callback for canvas commands.
pub type CanvasCallback = Box<dyn Fn(CanvasCommand) -> String + Send + 'static>;

pub struct CanvasIpcServer {
    socket_path: String,
    running: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Default for CanvasIpcServer {
    fn default() -> Self {
        Self::new()
    }
}

impl CanvasIpcServer {
    pub fn new() -> Self {
        CanvasIpcServer {
            socket_path: CANVAS_SOCKET_PATH.to_string(),
            running: Arc::new(AtomicBool::new(false)),
            thread: None,
        }
    }

    /// Start the IPC server with a command handler callback.
    pub fn start<F>(&mut self, callback: F) -> bool
    where
        F: Fn(CanvasCommand) -> String + Send + 'static,
    {
        if self.running.load(Ordering::SeqCst) {
            return true;
        }

        // Remove stale socket
        let _ = std::fs::remove_file(&self.socket_path);

        let listener = match UnixListener::bind(&self.socket_path) {
            Ok(l) => l,
            Err(e) => {
                log::error!("CanvasIPC: failed to bind {}: {}", self.socket_path, e);
                return false;
            }
        };

        self.running.store(true, Ordering::SeqCst);
        let running = self.running.clone();
        let socket_path = self.socket_path.clone();

        self.thread = Some(std::thread::spawn(move || {
            listener.set_nonblocking(true).ok();
            log::info!("CanvasIPC: listening on {}", socket_path);

            while running.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        Self::handle_client(stream, &callback);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                    Err(e) => {
                        log::warn!("CanvasIPC: accept error: {}", e);
                    }
                }
            }

            let _ = std::fs::remove_file(&socket_path);
        }));

        true
    }

    /// Stop the IPC server.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }

    fn handle_client(stream: UnixStream, callback: &dyn Fn(CanvasCommand) -> String) {
        let reader = BufReader::new(&stream);
        let mut writer = match stream.try_clone() {
            Ok(w) => w,
            Err(_) => return,
        };

        for line in reader.lines() {
            match line {
                Ok(line) => {
                    let (action, payload) = match line.split_once('|') {
                        Some((a, p)) => (a.to_string(), p.to_string()),
                        None => (line, String::new()),
                    };
                    let cmd = CanvasCommand { action, payload };
                    let response = callback(cmd);
                    let _ = writeln!(writer, "{}", response);
                }
                Err(_) => break,
            }
        }
    }
}

impl Drop for CanvasIpcServer {
    fn drop(&mut self) {
        self.stop();
    }
}
