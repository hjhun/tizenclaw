//! Web dashboard channel — launches the standalone dashboard binary.

use super::{Channel, ChannelConfig};
use serde_json::{json, Value};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};

pub struct WebDashboardChannel {
    name: String,
    process: Option<std::process::Child>,
    port: u16,
    web_root: PathBuf,
    socket_path: String,
}

pub type WebDashboard = WebDashboardChannel;

impl WebDashboardChannel {
    pub fn new(config: &ChannelConfig) -> Self {
        let port = config
            .settings
            .get("port")
            .and_then(Value::as_u64)
            .and_then(|value| u16::try_from(value).ok())
            .unwrap_or(crate::core::runtime_paths::default_dashboard_port());
        let web_root = config
            .settings
            .get("web_root")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .unwrap_or_else(|| libtizenclaw_core::framework::paths::PlatformPaths::detect().web_root);
        let socket_path = std::env::var("TIZENCLAW_SOCKET_PATH").unwrap_or_default();

        Self {
            name: config.name.clone(),
            process: None,
            port,
            web_root,
            socket_path,
        }
    }

    fn find_binary() -> PathBuf {
        if let Ok(exe) = std::env::current_exe() {
            let candidate = exe.with_file_name("tizenclaw-web-dashboard");
            if candidate.exists() {
                return candidate;
            }
        }
        PathBuf::from("tizenclaw-web-dashboard")
    }

    pub fn start(&mut self, port: u16, web_root: &Path, socket_path: &str) -> Result<(), String> {
        self.stop();

        self.port = port;
        self.web_root = web_root.to_path_buf();
        self.socket_path = socket_path.to_string();

        let binary = Self::find_binary();
        let mut command = std::process::Command::new(&binary);
        command
            .arg("--port")
            .arg(self.port.to_string())
            .arg("--web-root")
            .arg(&self.web_root);
        if !self.socket_path.trim().is_empty() {
            command.arg("--socket-path").arg(&self.socket_path);
        }

        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }

        command
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit());

        let child = command
            .spawn()
            .map_err(|err| format!("Failed to spawn {}: {}", binary.display(), err))?;
        self.process = Some(child);
        Ok(())
    }

    pub fn stop(&mut self) {
        let Some(mut child) = self.process.take() else {
            return;
        };

        let pid = child.id() as libc::pid_t;
        unsafe {
            libc::kill(-pid, libc::SIGTERM);
        }

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Ok(None) | Err(_) => {
                    unsafe {
                        libc::kill(-pid, libc::SIGKILL);
                    }
                    let _ = child.wait();
                    break;
                }
            }
        }
    }

    pub fn status(&self) -> Value {
        json!({
            "name": self.name(),
            "running": self.is_running(),
            "port": self.port,
            "url": format!("http://127.0.0.1:{}", self.port),
        })
    }
}

impl Channel for WebDashboardChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn start(&mut self) -> bool {
        let port = self.port;
        let web_root = self.web_root.clone();
        let socket_path = self.socket_path.clone();
        self.start(port, &web_root, &socket_path).is_ok()
    }

    fn stop(&mut self) {
        WebDashboardChannel::stop(self);
    }

    fn is_running(&self) -> bool {
        let Some(child) = self.process.as_ref() else {
            return false;
        };
        unsafe { libc::kill(child.id() as libc::pid_t, 0) == 0 }
    }

    fn send_message(&self, _text: &str) -> Result<(), String> {
        Ok(())
    }

    fn status(&self) -> Value {
        WebDashboardChannel::status(self)
    }

    fn configure(&mut self, settings: &Value) -> Result<(), String> {
        if let Some(port) = settings.get("port") {
            let port = port
                .as_u64()
                .ok_or_else(|| "Dashboard port must be a number".to_string())?;
            if !(1..=65535).contains(&port) {
                return Err("Dashboard port must be between 1 and 65535".to_string());
            }
            self.port = port as u16;
        }

        if let Some(web_root) = settings.get("web_root").and_then(Value::as_str) {
            self.web_root = PathBuf::from(web_root);
        }

        if let Some(socket_path) = settings.get("socket_path").and_then(Value::as_str) {
            self.socket_path = socket_path.to_string();
        }

        Ok(())
    }
}
