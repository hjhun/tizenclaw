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
    /// Map of plugin name → plugin .so path.
    plugin_registry: HashMap<String, PathBuf>,
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
            plugin_dirs: DEFAULT_PLUGIN_DIRS.iter().map(PathBuf::from).collect(),
        }
    }

    /// Add a directory to scan for plugins.
    pub fn add_plugin_dir(&mut self, dir: PathBuf) {
        if !self.plugin_dirs.contains(&dir) {
            self.plugin_dirs.push(dir);
        }
    }

    /// Scan plugin directories and register discovered plugins.
    pub fn scan_plugins(&mut self) {
        for dir in self.plugin_dirs.clone() {
            self.scan_directory(&dir);
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
            log::info!("Creating plugin LLM backend '{}' from {}", name, path_str);
            return Some(Box::new(PluginLlmBackend::new(&path_str)));
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
