//! LLM Plugin Manager — discovers and manages LLM backend plugins.
//!
//! Scans plugin directories for `.so` files and registers them as
//! available LLM backends alongside the built-in ones.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::backend::{self, LlmBackend};
use super::plugin_llm_backend::PluginLlmBackend;
use serde_json::Value;

/// Manages LLM backend creation with plugin support.
pub struct PluginManager {
    plugin_registry: HashMap<String, PathBuf>,
    /// Map of plugin name -> default JSON configuration parsed from plugin_llm_config.json.
    plugin_configs: HashMap<String, Value>,
}

/// Directory-based plugin loader requested by the LLM backend prompt.
///
/// This is additive to the existing package-manager-driven `PluginManager`
/// used by `AgentCore`, so current runtime wiring remains unchanged.
pub struct LlmPluginManager {
    plugins_dir: PathBuf,
    loaded: Vec<Box<dyn LlmBackend + Send + Sync>>,
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
        }
    }

    /// Scan plugin directories and register discovered plugins via local dirs and pkgmgr.
    pub fn scan_plugins(
        &mut self,
        pm: Option<&dyn libtizenclaw_core::framework::PackageManagerProvider>,
    ) {
        if let Some(pkgmgr) = pm {
            let pkgs = pkgmgr
                .get_packages_by_metadata_key("http://tizen.org/metadata/tizenclaw/llm-backend");
            log::debug!(
                "PluginManager: scanning pkgmgr for metadata key. Found {} package(s)",
                pkgs.len()
            );
            for pkg in pkgs {
                log::debug!(
                    "PluginManager: pkgmgr metadata match found for pkg_id '{}'",
                    pkg.pkg_id
                );
                if let Some(so_name) = pkgmgr.get_package_metadata_value(
                    &pkg.pkg_id,
                    "http://tizen.org/metadata/tizenclaw/llm-backend",
                ) {
                    log::debug!(
                        "PluginManager: resolved so_name '{}' for pkg_id '{}'",
                        so_name,
                        pkg.pkg_id
                    );
                    if let Some(root_path) = pkgmgr.get_package_root_path(&pkg.pkg_id) {
                        log::debug!(
                            "PluginManager: resolved root path '{}' for pkg_id '{}'",
                            root_path,
                            pkg.pkg_id
                        );
                        // Match tizenclaw-cpp string concatenation to prevent PathBuf absolute path wiping
                        let so_path_str = format!("{}/lib/{}", root_path, so_name);
                        let so_path = PathBuf::from(&so_path_str);

                        let cfg_path =
                            PathBuf::from(format!("{}/res/plugin_llm_config.json", root_path));
                        let fallback_cfg_path =
                            PathBuf::from(format!("{}/plugin_llm_config.json", root_path));

                        let mut config = serde_json::json!({});
                        if let Ok(content) = std::fs::read_to_string(&cfg_path)
                            .or_else(|_| std::fs::read_to_string(&fallback_cfg_path))
                        {
                            if let Ok(parsed) = serde_json::from_str(&content) {
                                config = parsed;
                            }
                        }

                        if so_path.exists() {
                            let name = pkg.pkg_id.clone();

                            log::debug!(
                                "PluginManager: successfully discovered valid plugin '{}' at {:?}",
                                name,
                                so_path
                            );
                            self.plugin_registry.insert(name.clone(), so_path);
                            self.plugin_configs.insert(name, config);
                        } else {
                            log::warn!(
                                "PluginManager: plugin so_path does not exist on disk: {:?}",
                                so_path
                            );
                        }
                    } else {
                        log::warn!(
                            "PluginManager: failed to get root path for pkg_id '{}'",
                            pkg.pkg_id
                        );
                    }
                } else {
                    log::warn!(
                        "PluginManager: failed to get metadata value for pkg_id '{}'",
                        pkg.pkg_id
                    );
                }
            }
        } else {
            log::warn!("PluginManager: PackageManagerProvider is None during scan_plugins!");
        }

        if !self.plugin_registry.is_empty() {
            log::debug!(
                "PluginManager: discovered {} LLM plugin(s): {:?}",
                self.plugin_registry.len(),
                self.plugin_registry.keys().collect::<Vec<_>>()
            );
        }
    }

    /// Load a single plugin by its package ID (used dynamically during installation)
    pub fn load_plugin_from_pkg(
        &mut self,
        pm: Option<&dyn libtizenclaw_core::framework::PackageManagerProvider>,
        pkgid: &str,
    ) -> bool {
        let Some(pkgmgr) = pm else {
            return false;
        };

        let root_path = match pkgmgr.get_package_root_path(pkgid) {
            Some(p) => p,
            None => return false,
        };

        let so_value = match pkgmgr
            .get_package_metadata_value(pkgid, "http://tizen.org/metadata/tizenclaw/llm-backend")
        {
            Some(v) => v,
            None => return false, // Not an LLM backend plugin
        };

        let full_so_path = format!("{}/lib/{}", root_path, so_value);

        let mut config = serde_json::json!({});
        let cfg_path = format!("{}/res/plugin_llm_config.json", root_path);
        let fallback_cfg_path = format!("{}/plugin_llm_config.json", root_path);
        if let Ok(content) = std::fs::read_to_string(&cfg_path)
            .or_else(|_| std::fs::read_to_string(&fallback_cfg_path))
        {
            if let Ok(parsed) = serde_json::from_str(&content) {
                config = parsed;
            }
        }

        let so_path = PathBuf::from(&full_so_path);
        if so_path.exists() {
            log::debug!(
                "PluginManager: dynamically registered plugin '{}' at '{}'",
                pkgid,
                full_so_path
            );
            self.plugin_registry.insert(pkgid.to_string(), so_path);
            self.plugin_configs.insert(pkgid.to_string(), config);
            true
        } else {
            false
        }
    }

    pub fn unload_plugin_from_pkg(&mut self, pkgid: &str) -> bool {
        let removed = self.plugin_registry.remove(pkgid).is_some();
        self.plugin_configs.remove(pkgid);
        if removed {
            log::info!("PluginManager: dynamically unloaded plugin '{}'", pkgid);
        }
        removed
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
            log::debug!("Creating plugin LLM backend '{}' from {}", name, path_str);
            return Some(Box::new(PluginLlmBackend::new(&path_str, base_config)));
        }

        log::warn!("No LLM backend found for name '{}'", name);
        None
    }

    pub fn get_plugin_config(&self, name: &str) -> Option<Value> {
        self.plugin_configs.get(name).cloned()
    }

    /// List all available plugin backend names.
    pub fn available_plugins(&self) -> Vec<String> {
        self.plugin_registry.keys().cloned().collect()
    }
}

impl LlmPluginManager {
    pub fn discover_and_load(plugins_dir: &Path) -> Self {
        let mut loaded: Vec<Box<dyn LlmBackend + Send + Sync>> = Vec::new();

        if let Ok(entries) = std::fs::read_dir(plugins_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let is_shared_object = path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("so"))
                    .unwrap_or(false);
                if !is_shared_object {
                    continue;
                }

                let path_str = path.to_string_lossy().to_string();
                let mut backend = PluginLlmBackend::new(&path_str, None);
                if backend.initialize(&serde_json::json!({})) {
                    loaded.push(Box::new(backend));
                } else {
                    log::warn!("Skipping LLM plugin at '{}'", path.display());
                }
            }
        }

        Self {
            plugins_dir: plugins_dir.to_path_buf(),
            loaded,
        }
    }

    pub fn backends(&self) -> &[Box<dyn LlmBackend + Send + Sync>] {
        &self.loaded
    }

    pub fn plugins_dir(&self) -> &Path {
        &self.plugins_dir
    }
}
