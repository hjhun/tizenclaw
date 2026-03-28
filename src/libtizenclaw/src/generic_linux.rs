//! Generic Linux platform implementation.
//!
//! Provides fallback implementations for all platform traits
//! when no platform-specific plugin (e.g., Tizen) is loaded.
//! Works on any standard Linux distribution (Ubuntu, Debian, Fedora, etc.)

use crate::{
    AppControlProvider, LogLevel, PackageInfo, PackageManagerProvider,
    PlatformLogger, PlatformPlugin, SystemInfoProvider,
};
use serde_json::{json, Value};
use std::process::Command;

// ─────────────────────────────────────────
// GenericLinuxPlatform — PlatformPlugin
// ─────────────────────────────────────────

pub struct GenericLinuxPlatform;

impl GenericLinuxPlatform {
    pub fn new() -> Self {
        GenericLinuxPlatform
    }
}

impl Default for GenericLinuxPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl PlatformPlugin for GenericLinuxPlatform {
    fn platform_name(&self) -> &str { "Generic Linux" }
    fn plugin_id(&self) -> &str { "generic-linux" }
    fn version(&self) -> &str { env!("CARGO_PKG_VERSION") }
    fn priority(&self) -> u32 { 0 } // Lowest priority — always a fallback
    fn is_compatible(&self) -> bool { true } // Always works on Linux
}

// ─────────────────────────────────────────
// StderrLogger — PlatformLogger
// ─────────────────────────────────────────

/// Logs to stderr with colorized level prefixes.
pub struct StderrLogger;

impl PlatformLogger for StderrLogger {
    fn log(&self, level: LogLevel, tag: &str, msg: &str) {
        let (prefix, _color) = match level {
            LogLevel::Error => ("E", "\x1b[31m"),
            LogLevel::Warn  => ("W", "\x1b[33m"),
            LogLevel::Info  => ("I", "\x1b[32m"),
            LogLevel::Debug => ("D", "\x1b[36m"),
        };
        eprintln!("[{}] [{}] {}", prefix, tag, msg);
    }
}

// ─────────────────────────────────────────
// LinuxSystemInfo — SystemInfoProvider
// ─────────────────────────────────────────

pub struct LinuxSystemInfo;

impl SystemInfoProvider for LinuxSystemInfo {
    fn get_os_version(&self) -> Option<String> {
        // Try /etc/os-release first (works on most modern distros)
        if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
            for line in content.lines() {
                if let Some(val) = line.strip_prefix("PRETTY_NAME=") {
                    return Some(val.trim_matches('"').to_string());
                }
            }
        }
        // Fallback: uname -r
        Command::new("uname").arg("-r").output().ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    }

    fn get_device_profile(&self) -> Value {
        let mut profile = json!({});

        // CPU info
        if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
            let cores = cpuinfo.matches("processor").count();
            profile["cpu_cores"] = json!(cores);
            for line in cpuinfo.lines() {
                if line.starts_with("model name") {
                    if let Some(name) = line.split(':').nth(1) {
                        profile["cpu_model"] = json!(name.trim());
                        break;
                    }
                }
            }
        }

        // Memory
        if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            for line in meminfo.lines() {
                if line.starts_with("MemTotal:") {
                    let kb: u64 = line.split_whitespace().nth(1)
                        .and_then(|s| s.parse().ok()).unwrap_or(0);
                    profile["memory_mb"] = json!(kb / 1024);
                    break;
                }
            }
        }

        // OS version
        if let Some(ver) = self.get_os_version() {
            profile["os_version"] = json!(ver);
        }

        // Display resolution (X11/Wayland)
        if let Ok(out) = Command::new("xrandr").arg("--current").output() {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if line.contains('*') {
                    if let Some(res) = line.split_whitespace().next() {
                        profile["display_resolution"] = json!(res);
                        break;
                    }
                }
            }
        }

        // Hostname
        if let Ok(name) = std::fs::read_to_string("/etc/hostname") {
            profile["hostname"] = json!(name.trim());
        }

        profile
    }

    fn get_battery_level(&self) -> Option<u32> {
        std::fs::read_to_string("/sys/class/power_supply/battery/capacity")
            .or_else(|_| std::fs::read_to_string("/sys/class/power_supply/BAT0/capacity"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }
}

// ─────────────────────────────────────────
// GenericPackageManager — PackageManagerProvider
// ─────────────────────────────────────────

pub struct GenericPackageManager;

impl PackageManagerProvider for GenericPackageManager {
    fn list_packages(&self) -> Vec<PackageInfo> {
        // Try dpkg (Debian/Ubuntu)
        if let Ok(out) = Command::new("dpkg").args(["--list"]).output() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            return parse_dpkg_list(&stdout);
        }
        // Try rpm (Fedora/RHEL)
        if let Ok(out) = Command::new("rpm").args(["-qa", "--queryformat",
            "%{NAME}\\t%{VERSION}\\t%{RELEASE}\\n"]).output() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            return parse_rpm_list(&stdout);
        }
        vec![]
    }

    fn get_package_info(&self, pkg_id: &str) -> Option<PackageInfo> {
        // Try dpkg
        if let Ok(out) = Command::new("dpkg").args(["-s", pkg_id]).output() {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                return Some(parse_dpkg_info(&stdout, pkg_id));
            }
        }
        None
    }
}

fn parse_dpkg_list(output: &str) -> Vec<PackageInfo> {
    let mut packages = Vec::new();
    for line in output.lines().skip(5) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 && parts[0] == "ii" {
            packages.push(PackageInfo {
                pkg_id: parts[1].to_string(),
                version: parts[2].to_string(),
                pkg_type: "deb".into(),
                installed: true,
                ..Default::default()
            });
        }
    }
    packages
}

fn parse_rpm_list(output: &str) -> Vec<PackageInfo> {
    let mut packages = Vec::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            packages.push(PackageInfo {
                pkg_id: parts[0].to_string(),
                version: parts[1].to_string(),
                pkg_type: "rpm".into(),
                installed: true,
                ..Default::default()
            });
        }
    }
    packages
}

fn parse_dpkg_info(output: &str, pkg_id: &str) -> PackageInfo {
    let mut info = PackageInfo {
        pkg_id: pkg_id.to_string(),
        installed: true,
        pkg_type: "deb".into(),
        ..Default::default()
    };
    for line in output.lines() {
        if let Some(val) = line.strip_prefix("Version: ") {
            info.version = val.to_string();
        } else if let Some(val) = line.strip_prefix("Description: ") {
            info.label = val.to_string();
        }
    }
    info
}

// ─────────────────────────────────────────
// GenericAppControl — AppControlProvider
// ─────────────────────────────────────────

pub struct GenericAppControl;

impl AppControlProvider for GenericAppControl {
    fn launch_app(&self, app_id: &str) -> Result<(), String> {
        // Try xdg-open for URLs and files
        Command::new("xdg-open")
            .arg(app_id)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Failed to launch '{}': {}", app_id, e))
    }

    fn list_running_apps(&self) -> Vec<String> {
        // Use /proc to list running processes
        let mut apps = vec![];
        if let Ok(entries) = std::fs::read_dir("/proc") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.chars().all(|c| c.is_ascii_digit()) {
                    if let Ok(cmdline) = std::fs::read_to_string(
                        entry.path().join("cmdline")
                    ) {
                        if let Some(cmd) = cmdline.split('\0').next() {
                            if !cmd.is_empty() {
                                apps.push(cmd.to_string());
                            }
                        }
                    }
                }
            }
        }
        apps
    }
}
