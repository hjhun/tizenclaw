//! CLI plugin manager — manages TPK CLI tool plugin packages.

use std::collections::HashMap;

pub struct CliPluginManager {
    plugins: HashMap<String, CliPluginInfo>,
    on_change: Option<Box<dyn Fn() + Send + Sync>>,
}

#[derive(Clone, Debug)]
pub struct CliPluginInfo {
    pub package_id: String,
    pub tool_name: String,
    pub binary_path: String,
    pub enabled: bool,
}

impl Default for CliPluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CliPluginManager {
    pub fn new() -> Self {
        CliPluginManager {
            plugins: HashMap::new(),
            on_change: None,
        }
    }

    pub fn set_change_callback(&mut self, cb: impl Fn() + Send + Sync + 'static) {
        self.on_change = Some(Box::new(cb));
    }

    pub fn initialize(&mut self) {
        self.scan_installed_plugins();
        log::info!("CliPluginManager: {} plugins loaded", self.plugins.len());
    }

    fn scan_installed_plugins(&mut self) {
        let metadata_dir = std::env::var("TIZENCLAW_DATA_DIR")
            .map(|d| format!("{}/plugins/cli", d))
            .unwrap_or_else(|_| "/opt/usr/share/tizenclaw/plugins/cli".to_string());
        let entries = match std::fs::read_dir(metadata_dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() { continue; }
            let pkg_id = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if pkg_id.is_empty() { continue; }

            // Look for metadata JSON
            let meta_path = path.join("cli_metadata.json");
            let (tool_name, binary) = if let Ok(content) = std::fs::read_to_string(&meta_path) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                    (
                        v["tool_name"].as_str().unwrap_or(&pkg_id).to_string(),
                        v["binary_path"].as_str().unwrap_or("").to_string(),
                    )
                } else {
                    (pkg_id.clone(), String::new())
                }
            } else {
                (pkg_id.clone(), String::new())
            };

            self.plugins.insert(pkg_id.clone(), CliPluginInfo {
                package_id: pkg_id,
                tool_name,
                binary_path: binary,
                enabled: true,
            });
        }
    }

    pub fn on_package_installed(&mut self, package_id: &str) {
        log::info!("CliPluginManager: package installed: {}", package_id);
        self.scan_installed_plugins();
        if let Some(cb) = &self.on_change { cb(); }
    }

    pub fn on_package_uninstalled(&mut self, package_id: &str) {
        log::info!("CliPluginManager: package uninstalled: {}", package_id);
        self.plugins.remove(package_id);
        if let Some(cb) = &self.on_change { cb(); }
    }

    pub fn get_all_plugins(&self) -> Vec<&CliPluginInfo> {
        self.plugins.values().collect()
    }
}
