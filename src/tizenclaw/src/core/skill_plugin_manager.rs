//! Skill plugin manager — manages RPK skill plugin packages.

use std::collections::HashMap;

pub struct SkillPluginManager {
    plugins: HashMap<String, SkillPluginInfo>,
    on_change: Option<Box<dyn Fn() + Send + Sync>>,
}

#[derive(Clone, Debug)]
pub struct SkillPluginInfo {
    pub package_id: String,
    pub skill_name: String,
    pub install_path: String,
    pub enabled: bool,
}

impl SkillPluginManager {
    pub fn new() -> Self {
        SkillPluginManager {
            plugins: HashMap::new(),
            on_change: None,
        }
    }

    pub fn set_change_callback(&mut self, cb: impl Fn() + Send + Sync + 'static) {
        self.on_change = Some(Box::new(cb));
    }

    pub fn initialize(&mut self) {
        self.scan_installed_plugins();
        log::info!("SkillPluginManager: {} plugins loaded", self.plugins.len());
    }

    fn scan_installed_plugins(&mut self) {
        let plugin_dir = "/opt/usr/share/tizen-tools/skills/plugins";
        let entries = match std::fs::read_dir(plugin_dir) {
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

            self.plugins.insert(pkg_id.clone(), SkillPluginInfo {
                package_id: pkg_id.clone(),
                skill_name: pkg_id.clone(),
                install_path: path.to_string_lossy().to_string(),
                enabled: true,
            });
        }
    }

    pub fn on_package_installed(&mut self, package_id: &str) {
        log::info!("SkillPluginManager: package installed: {}", package_id);
        self.scan_installed_plugins();
        if let Some(cb) = &self.on_change { cb(); }
    }

    pub fn on_package_uninstalled(&mut self, package_id: &str) {
        log::info!("SkillPluginManager: package uninstalled: {}", package_id);
        self.plugins.remove(package_id);
        if let Some(cb) = &self.on_change { cb(); }
    }

    pub fn get_all_plugins(&self) -> Vec<&SkillPluginInfo> {
        self.plugins.values().collect()
    }
}
