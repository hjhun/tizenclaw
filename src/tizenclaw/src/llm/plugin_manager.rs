//! LLM Plugin Manager — discovers and manages LLM backend plugins.
//!
//! Scans plugin directories for `.so` files and registers them as
//! available LLM backends alongside the built-in ones.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::backend::{self, LlmBackend};
use super::plugin_llm_backend::PluginLlmBackend;
use serde_json::Value;

/// Default plugin search directories.
const DEFAULT_PLUGIN_DIRS: &[&str] = &[
    "/usr/lib/tizenclaw/plugins/llm",
];

/// Manages LLM backend creation with plugin support.
pub struct PluginManager {
    plugin_registry: HashMap<String, PathBuf>,
    /// Map of plugin name -> default JSON configuration parsed from plugin_llm_config.json.
    plugin_configs: HashMap<String, Value>,
    /// Directories to scan for plugins.
    plugin_dirs: Vec<PathBuf>,
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginManager {
    pub fn new() -> Self {
        PluginManager {
            plugin_registry: HashMap::new(),
            plugin_configs: HashMap::new(),
            plugin_dirs: DEFAULT_PLUGIN_DIRS.iter().map(PathBuf::from).collect(),
        }
    }

    /// Add a directory to scan for plugins.
    pub fn add_plugin_dir(&mut self, dir: PathBuf) {
        if !self.plugin_dirs.contains(&dir) {
            self.plugin_dirs.push(dir);
        }
    }

    /// Scan plugin directories and register discovered plugins via local dirs and pkgmgr.
    pub fn scan_plugins(&mut self, pm: Option<&dyn libtizenclaw_core::framework::PackageManagerProvider>) {
        for dir in self.plugin_dirs.clone() {
            self.scan_directory(&dir);
        }

        if let Some(pkgmgr) = pm {
            let pkgs = pkgmgr.get_packages_by_metadata_key("http://tizen.org/metadata/tizenclaw/llm-backend");
            for pkg in pkgs {
                if let Some(so_name) = pkgmgr.get_package_metadata_value(&pkg.pkg_id, "http://tizen.org/metadata/tizenclaw/llm-backend") {
                    if let Some(root_path) = pkgmgr.get_package_root_path(&pkg.pkg_id) {
                        // Match tizenclaw-cpp string concatenation to prevent PathBuf absolute path wiping
                        let so_path_str = format!("{}/lib/{}", root_path, so_name);
                        let so_path = PathBuf::from(&so_path_str);
                        
                        let cfg_path = PathBuf::from(format!("{}/res/plugin_llm_config.json", root_path));
                        let fallback_cfg_path = PathBuf::from(format!("{}/plugin_llm_config.json", root_path));
                        
                        let mut config = serde_json::json!({});
                        if let Ok(content) = std::fs::read_to_string(&cfg_path).or_else(|_| std::fs::read_to_string(&fallback_cfg_path)) {
                            if let Ok(parsed) = serde_json::from_str(&content) {
                                config = parsed;
                            }
                        }

                        if so_path.exists() {
                            let name = pkg.pkg_id.clone();
                            
                            log::info!("PluginManager: pkgmgr discovered plugin '{}' at {:?}", name, so_path);
                            self.plugin_registry.insert(name.clone(), so_path);
                            self.plugin_configs.insert(name, config);
                        } else {
                            log::warn!("PluginManager: plugin path does not exist: {:?}", so_path);
                        }
                    }
                }
            }
        }

        if !self.plugin_registry.is_empty() {
            log::info!(
                "PluginManager: discovered {} LLM plugin(s): {:?}",
                self.plugin_registry.len(),
                self.plugin_registry.keys().collect::<Vec<_>>()
            );
        }
    }

    /// Scan a specific directory for `.so` plugin files.
    pub fn scan_directory(&mut self, dir: &Path) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("so") {
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .strip_prefix("lib")
                    .unwrap_or_else(|| {
                        path.file_stem().and_then(|s| s.to_str()).unwrap_or("")
                    })
                    .to_string();

                if !name.is_empty() {
                    log::debug!("PluginManager: found plugin '{}' at {:?}", name, path);
                    self.plugin_registry.insert(name, path);
                }
            }
        }
    }

    /// Create an LLM backend by name (built-in or plugin).
    pub fn create_backend(&self, name: &str) -> Option<Box<dyn LlmBackend>> {
        // Try built-in backends first
        if let Some(be) = backend::create_backend(name) {
            return Some(be);
        }

        // Try plugin backends
        if let Some(plugin_path) = self.plugin_registry.get(name) {
            let path_str = plugin_path.to_string_lossy();
            let base_config = self.plugin_configs.get(name).cloned();
            log::info!("Creating plugin LLM backend '{}' from {}", name, path_str);
            return Some(Box::new(PluginLlmBackend::new(&path_str, base_config)));
        }

        log::warn!("No LLM backend found for name '{}'", name);
        None
    }

    /// List all available backend names (built-in + plugins).
    pub fn available_backends(&self) -> Vec<String> {
        let mut names: Vec<String> = vec![
            "gemini".into(),
            "openai".into(),
            "xai".into(),
            "anthropic".into(),
            "ollama".into(),
        ];
        names.extend(self.plugin_registry.keys().cloned());
        names
    }
}
