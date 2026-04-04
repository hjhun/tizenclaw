//! Package manager client — queries installed packages and app metadata.
//!
//! Wraps `pkgcmd` / `pkginfo` CLI tools for querying package information.

use serde_json::Value;
use std::collections::HashMap;
use std::process::Command;

/// Basic info about an installed package.
#[derive(Debug, Clone)]
pub struct PackageInfo {
    pub pkg_id: String,
    pub app_id: String,
    pub label: String,
    pub version: String,
    pub pkg_type: String,
    pub installed: bool,
}

pub struct PkgmgrClient;

impl PkgmgrClient {
    /// List all installed packages on the device.
    pub fn list_packages() -> Vec<PackageInfo> {
        let output = Command::new("pkgcmd")
            .args(["--list", "-t", "0"])
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                Self::parse_pkg_list(&stdout)
            }
            Err(e) => {
                log::warn!("PkgmgrClient: pkgcmd failed: {}", e);
                Vec::new()
            }
        }
    }

    /// Get info about a specific package.
    pub fn get_package_info(pkg_id: &str) -> Option<PackageInfo> {
        let output = Command::new("pkgcmd")
            .args(["--info", "-n", pkg_id])
            .output();

        match output {
            Ok(out) => {
                if !out.status.success() {
                    return None;
                }
                let stdout = String::from_utf8_lossy(&out.stdout);
                Self::parse_pkg_info(&stdout, pkg_id)
            }
            Err(_) => None,
        }
    }

    /// Check if a package is installed.
    pub fn is_installed(pkg_id: &str) -> bool {
        Self::get_package_info(pkg_id).is_some_and(|p| p.installed)
    }

    fn parse_pkg_list(output: &str) -> Vec<PackageInfo> {
        let mut packages = Vec::new();
        for line in output.lines() {
            // pkgcmd --list output format: "pkg_id\tversion\ttype\tinstalled"
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                packages.push(PackageInfo {
                    pkg_id: parts[0].trim().to_string(),
                    app_id: String::new(),
                    label: String::new(),
                    version: parts.get(1).unwrap_or(&"").to_string(),
                    pkg_type: parts.get(2).unwrap_or(&"").to_string(),
                    installed: true,
                });
            }
        }
        packages
    }

    fn parse_pkg_info(output: &str, pkg_id: &str) -> Option<PackageInfo> {
        let mut info = PackageInfo {
            pkg_id: pkg_id.to_string(),
            app_id: String::new(),
            label: String::new(),
            version: String::new(),
            pkg_type: String::new(),
            installed: true,
        };

        for line in output.lines() {
            let trimmed = line.trim();
            if let Some((key, val)) = trimmed.split_once(':') {
                let key = key.trim().to_lowercase();
                let val = val.trim().to_string();
                match key.as_str() {
                    "version" => info.version = val,
                    "type" => info.pkg_type = val,
                    "label" => info.label = val,
                    "mainappid" | "main_appid" => info.app_id = val,
                    _ => {}
                }
            }
        }

        Some(info)
    }
}
