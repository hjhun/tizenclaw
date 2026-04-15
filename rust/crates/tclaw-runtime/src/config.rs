use serde::{Deserialize, Serialize};

use crate::{
    mcp_lifecycle_hardened::McpLifecyclePolicy, mcp_stdio::McpStdioServerSpec,
    permissions::PermissionMode, policy_engine::PolicyEngineState, sandbox::SandboxPolicy,
};

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
pub struct McpRuntimeConfig {
    pub enabled: bool,
    #[serde(default)]
    pub servers: Vec<McpStdioServerSpec>,
    #[serde(default)]
    pub lifecycle_policy: McpLifecyclePolicy,
    pub tool_namespace: String,
}

impl Default for McpRuntimeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            servers: Vec::new(),
            lifecycle_policy: McpLifecyclePolicy::default(),
            tool_namespace: "mcp".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub profile: RuntimeProfile,
    pub paths: RuntimePaths,
    pub permission_mode: PermissionMode,
    #[serde(default)]
    pub permission_policy: PolicyEngineState,
    pub hooks_enabled: bool,
    pub sandbox_enabled: bool,
    #[serde(default)]
    pub sandbox_policy: SandboxPolicy,
    pub plugin_roots: Vec<String>,
    #[serde(default)]
    pub mcp: McpRuntimeConfig,
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
        if let Some(permission_policy) = patch.permission_policy {
            self.permission_policy = permission_policy;
        }
        if let Some(hooks_enabled) = patch.hooks_enabled {
            self.hooks_enabled = hooks_enabled;
        }
        if let Some(sandbox_enabled) = patch.sandbox_enabled {
            self.sandbox_enabled = sandbox_enabled;
        }
        if let Some(sandbox_policy) = patch.sandbox_policy {
            self.sandbox_policy = sandbox_policy;
        }
        if let Some(plugin_roots) = patch.plugin_roots {
            self.plugin_roots = plugin_roots;
        }
        if let Some(mcp) = patch.mcp {
            self.mcp = mcp;
        }
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            profile: RuntimeProfile::Host,
            paths: RuntimePaths::default(),
            permission_mode: PermissionMode::Ask,
            permission_policy: PolicyEngineState::default(),
            hooks_enabled: true,
            sandbox_enabled: true,
            sandbox_policy: SandboxPolicy::default(),
            plugin_roots: vec!["plugins".to_string()],
            mcp: McpRuntimeConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RuntimeConfigPatch {
    pub profile: Option<RuntimeProfile>,
    pub paths: Option<RuntimePaths>,
    pub permission_mode: Option<PermissionMode>,
    pub permission_policy: Option<PolicyEngineState>,
    pub hooks_enabled: Option<bool>,
    pub sandbox_enabled: Option<bool>,
    pub sandbox_policy: Option<SandboxPolicy>,
    pub plugin_roots: Option<Vec<String>>,
    pub mcp: Option<McpRuntimeConfig>,
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
            sandbox_policy: Some(SandboxPolicy {
                enabled: false,
                profile_name: "test".to_string(),
                writable_roots: vec!["tests".to_string()],
                network_access: true,
            }),
            ..RuntimeConfigPatch::default()
        });

        assert_eq!(config.profile, RuntimeProfile::Test);
        assert!(!config.sandbox_enabled);
        assert_eq!(config.sandbox_policy.profile_name, "test");
        assert_eq!(config.permission_mode, PermissionMode::Ask);
    }
}
