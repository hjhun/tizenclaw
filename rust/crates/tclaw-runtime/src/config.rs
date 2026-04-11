use serde::{Deserialize, Serialize};

use crate::permissions::PermissionMode;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RuntimeProfile {
    Host,
    Tizen,
    Test,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimePaths {
    pub root_dir: String,
    pub session_dir: String,
    pub plugin_dir: String,
    pub log_dir: String,
}

impl Default for RuntimePaths {
    fn default() -> Self {
        Self {
            root_dir: ".tclaw".to_string(),
            session_dir: ".tclaw/sessions".to_string(),
            plugin_dir: ".tclaw/plugins".to_string(),
            log_dir: ".tclaw/logs".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub profile: RuntimeProfile,
    pub paths: RuntimePaths,
    pub permission_mode: PermissionMode,
    pub hooks_enabled: bool,
    pub sandbox_enabled: bool,
    pub plugin_roots: Vec<String>,
}

impl RuntimeConfig {
    pub fn apply_patch(&mut self, patch: RuntimeConfigPatch) {
        if let Some(profile) = patch.profile {
            self.profile = profile;
        }
        if let Some(paths) = patch.paths {
            self.paths = paths;
        }
        if let Some(permission_mode) = patch.permission_mode {
            self.permission_mode = permission_mode;
        }
        if let Some(hooks_enabled) = patch.hooks_enabled {
            self.hooks_enabled = hooks_enabled;
        }
        if let Some(sandbox_enabled) = patch.sandbox_enabled {
            self.sandbox_enabled = sandbox_enabled;
        }
        if let Some(plugin_roots) = patch.plugin_roots {
            self.plugin_roots = plugin_roots;
        }
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            profile: RuntimeProfile::Host,
            paths: RuntimePaths::default(),
            permission_mode: PermissionMode::Ask,
            hooks_enabled: true,
            sandbox_enabled: true,
            plugin_roots: vec!["plugins".to_string()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RuntimeConfigPatch {
    pub profile: Option<RuntimeProfile>,
    pub paths: Option<RuntimePaths>,
    pub permission_mode: Option<PermissionMode>,
    pub hooks_enabled: Option<bool>,
    pub sandbox_enabled: Option<bool>,
    pub plugin_roots: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_patch_updates_selected_fields() {
        let mut config = RuntimeConfig::default();
        config.apply_patch(RuntimeConfigPatch {
            profile: Some(RuntimeProfile::Test),
            sandbox_enabled: Some(false),
            ..RuntimeConfigPatch::default()
        });

        assert_eq!(config.profile, RuntimeProfile::Test);
        assert!(!config.sandbox_enabled);
        assert_eq!(config.permission_mode, PermissionMode::Ask);
    }
}
