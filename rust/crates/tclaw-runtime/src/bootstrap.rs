use serde::{Deserialize, Serialize};

use tclaw_api::{canonical_surfaces, SurfaceDescriptor};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeModuleMap {
    pub modules: Vec<String>,
}

impl RuntimeModuleMap {
    pub fn new() -> Self {
        Self {
            modules: vec![
                "bash",
                "bash_validation",
                "bootstrap",
                "branch_lock",
                "compact",
                "config",
                "config_validate",
                "conversation",
                "file_ops",
                "git_context",
                "green_contract",
                "hooks",
                "json",
                "lane_events",
                "lsp_client",
                "mcp",
                "mcp_client",
                "mcp_lifecycle_hardened",
                "mcp_server",
                "mcp_stdio",
                "mcp_tool_bridge",
                "oauth",
                "permission_enforcer",
                "permissions",
                "plugin_lifecycle",
                "policy_engine",
                "prompt",
                "recovery_recipes",
                "remote",
                "sandbox",
                "session",
                "session_control",
                "stale_base",
                "stale_branch",
                "summary_compression",
                "task_packet",
                "task_registry",
                "team_cron_registry",
                "trust_resolver",
                "usage",
                "worker_boot",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        }
    }
}

impl Default for RuntimeModuleMap {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeBootstrap {
    pub canonical_runtime: String,
    pub surfaces: Vec<SurfaceDescriptor>,
    pub modules: RuntimeModuleMap,
}

impl RuntimeBootstrap {
    pub fn new() -> Self {
        Self {
            canonical_runtime: "rust".to_string(),
            surfaces: canonical_surfaces(),
            modules: RuntimeModuleMap::new(),
        }
    }
}

impl Default for RuntimeBootstrap {
    fn default() -> Self {
        Self::new()
    }
}
