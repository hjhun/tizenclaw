//! Web Dashboard channel — on-demand process launcher.
//!
//! Instead of embedding the HTTP server in the daemon, this channel
//! manages a `tizenclaw-web-dashboard` child process.  The binary is
//! located next to the running daemon executable, or found via PATH.
//!
//! Lifecycle:
//!   start() → spawn tizenclaw-web-dashboard with resolved paths as args
//!   stop()  → SIGTERM + wait()
//!   is_running() → libc::kill(pid, 0)

use super::{Channel, ChannelConfig};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct WebDashboard {
    name: String,
    port: u16,
    localhost_only: bool,
    web_root: PathBuf,
    config_dir: PathBuf,
    data_dir: PathBuf,
    child_pid: Option<u32>,
    running: Arc<AtomicBool>,
    monitor: Option<std::thread::JoinHandle<()>>,
}

impl WebDashboard {
    const PROCESS_COMM_NAME: &'static str = "tizenclaw-web-d";

    pub fn new(config: &ChannelConfig) -> Self {
        let port = config
            .settings
            .get("port")
            .and_then(|v| v.as_u64())
            .unwrap_or(crate::core::runtime_paths::default_dashboard_port() as u64)
            as u16;
        let localhost_only = config
            .settings
            .get("localhost_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let data_dir = crate::core::runtime_paths::default_data_dir();
        let default_web_root = data_dir.join("web");
        let web_root = config
            .settings
            .get("web_root")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or(default_web_root);
        let config_dir = data_dir.join("config");

        WebDashboard {
            name: config.name.clone(),
            port,
            localhost_only,
            web_root,
            config_dir,
            data_dir,
            child_pid: None,
            running: Arc::new(AtomicBool::new(false)),
            monitor: None,
        }
    }

    /// Resolve the tizenclaw-web-dashboard binary path.
    /// Tries the directory of the running daemon first, then falls back to PATH.
    fn find_binary() -> PathBuf {
        if let Ok(exe) = std::env::current_exe() {
            let candidate = exe.with_file_name("tizenclaw-web-dashboard");
            if candidate.exists() {
                return candidate;
            }
        }
        PathBuf::from("tizenclaw-web-dashboard")
    }

    fn cleanup_stale_processes() {
        // `pkill` matches the kernel comm name by default, which is
        // truncated to 15 characters on Linux.
        let _ = std::process::Command::new("pkill")
            .args(["-TERM", "-x", Self::PROCESS_COMM_NAME])
            .status();
        std::thread::sleep(std::time::Duration::from_millis(250));
        let _ = std::process::Command::new("pkill")
            .args(["-KILL", "-x", Self::PROCESS_COMM_NAME])
            .status();
    }
}

impl Channel for WebDashboard {
    fn name(&self) -> &str {
        &self.name
    }

    fn start(&mut self) -> bool {
        if self.is_running() {
            return true;
        }

        self.cleanup_monitor();
        Self::cleanup_stale_processes();

        let bin = Self::find_binary();
        let mut cmd = std::process::Command::new(&bin);
        cmd.arg("--port")
            .arg(self.port.to_string())
            .arg("--web-root")
            .arg(&self.web_root)
            .arg("--config-dir")
            .arg(&self.config_dir)
            .arg("--data-dir")
            .arg(&self.data_dir);
        if self.localhost_only {
            cmd.arg("--localhost-only");
        }
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        // Inherit stdout/stderr so logs flow to the same terminal / journal
        cmd.stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit());

        match cmd.spawn() {
            Ok(child) => {
                let pid = child.id();
                let running = Arc::clone(&self.running);
                running.store(true, Ordering::SeqCst);
                let monitor = std::thread::spawn(move || {
                    let mut child = child;
                    let status = child.wait();
                    running.store(false, Ordering::SeqCst);
                    match status {
                        Ok(status) => {
                            log::info!("WebDashboard process exited with status {}", status);
                        }
                        Err(err) => {
                            log::warn!("WebDashboard process wait failed: {}", err);
                        }
                    }
                });
                log::info!(
                    "WebDashboard process started (pid {}, port {})",
                    pid,
                    self.port
                );
                self.child_pid = Some(pid);
                self.monitor = Some(monitor);
                true
            }
            Err(e) => {
                log::error!(
                    "Failed to spawn tizenclaw-web-dashboard ({}): {}",
                    bin.display(),
                    e
                );
                false
            }
        }
    }

    fn stop(&mut self) {
        if let Some(pid) = self.child_pid.take() {
            let pgid = -(pid as libc::pid_t);
            // Send SIGTERM for graceful shutdown
            unsafe {
                libc::kill(pgid, libc::SIGTERM);
            }
            // Give the process up to 3 seconds, then force-kill
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            loop {
                if !self.running.load(Ordering::SeqCst) {
                    break;
                }
                if std::time::Instant::now() >= deadline {
                    unsafe {
                        libc::kill(pgid, libc::SIGKILL);
                    }
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            self.running.store(false, Ordering::SeqCst);
            self.cleanup_monitor();
            log::info!("WebDashboard process stopped");
        }
    }

    fn is_running(&self) -> bool {
        if !self.running.load(Ordering::SeqCst) {
            return false;
        }

        let Some(pid) = self.child_pid else {
            return false;
        };

        // kill(pid, 0) returns 0 if the process exists, -1 otherwise
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }

    fn send_message(&self, _msg: &str) -> Result<(), String> {
        Ok(()) // pull-based; no push support needed
    }

    fn configure(&mut self, settings: &serde_json::Value) -> Result<(), String> {
        if let Some(port) = settings.get("port") {
            let port = port
                .as_u64()
                .ok_or_else(|| "Dashboard port must be a number".to_string())?;
            if !(1..=65535).contains(&port) {
                return Err("Dashboard port must be between 1 and 65535".to_string());
            }
            self.port = port as u16;
        }

        if let Some(localhost_only) = settings.get("localhost_only") {
            self.localhost_only = localhost_only
                .as_bool()
                .ok_or_else(|| "localhost_only must be a boolean".to_string())?;
        }

        Ok(())
    }
}

impl WebDashboard {
    fn cleanup_monitor(&mut self) {
        if let Some(handle) = self.monitor.take() {
            let _ = handle.join();
        }
    }
}
