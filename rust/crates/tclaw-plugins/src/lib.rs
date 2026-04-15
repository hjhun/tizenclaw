mod hooks;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tclaw_api::SurfaceDescriptor;
use tclaw_commands::{
    validate_manifest_entry, CommandManifestEntry, CommandSource, InputValidationError,
    ResumeBehavior, SlashCommandArgHint,
};
use thiserror::Error;

pub use hooks::{
    execute_plugin_hooks, HookExecutionReport, HookExecutionResult, HookExecutionStatus, HookPhase,
    HookSpec,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    Metadata,
    #[default]
    Tooling,
    Runtime,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PluginMetadata {
    #[serde(default)]
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PluginPermissionScope {
    #[default]
    Read,
    Write,
    Execute,
    Network,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "snake_case")]
pub enum PluginPermissionLevel {
    Low,
    #[default]
    Standard,
    Sensitive,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PluginPermission {
    #[serde(default)]
    pub scope: PluginPermissionScope,
    #[serde(default)]
    pub level: PluginPermissionLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl PluginPermission {
    pub fn validate(&self, context: &str) -> Result<(), PluginManifestError> {
        if matches!(self.target.as_deref(), Some("")) {
            return Err(PluginManifestError::InvalidPermission {
                plugin_name: context.to_string(),
                message: "permission target must not be empty".to_string(),
            });
        }

        if matches!(self.reason.as_deref(), Some("")) {
            return Err(PluginManifestError::InvalidPermission {
                plugin_name: context.to_string(),
                message: "permission reason must not be empty".to_string(),
            });
        }

        if matches!(
            self.scope,
            PluginPermissionScope::Execute | PluginPermissionScope::Network
        ) && self.reason.is_none()
        {
            return Err(PluginManifestError::InvalidPermission {
                plugin_name: context.to_string(),
                message: format!("{:?} permissions require an explicit reason", self.scope)
                    .to_ascii_lowercase(),
            });
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginCommandManifest {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub argument_hints: Vec<SlashCommandArgHint>,
    #[serde(default)]
    pub resume_behavior: ResumeBehavior,
}

impl PluginCommandManifest {
    pub fn to_command_manifest(&self, plugin_name: &str) -> CommandManifestEntry {
        CommandManifestEntry::new(
            self.name.clone(),
            CommandSource::Plugin {
                plugin_name: plugin_name.to_string(),
            },
            self.summary.clone(),
        )
        .with_aliases(self.aliases.clone())
        .with_argument_hints(self.argument_hints.clone())
        .with_resume_behavior(self.resume_behavior.clone())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginToolManifest {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    pub description: String,
    pub input_schema: Value,
    #[serde(default)]
    pub permissions: PluginPermission,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PluginLifecycleDefinition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_phase: Option<PluginLifecyclePhase>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<HookSpec>,
}

impl PluginLifecycleDefinition {
    pub fn merged_with(&self, overlay: &Self) -> Self {
        let mut hooks = self.hooks.clone();
        hooks.extend(overlay.hooks.clone());
        Self {
            default_phase: overlay.default_phase.clone().or(self.default_phase.clone()),
            hooks,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginLifecyclePhase {
    Discovered,
    Loaded,
    Active,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PluginLifecycleState {
    pub plugin_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<PluginLifecyclePhase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

impl PluginLifecycleState {
    pub fn merged_with(&self, overlay: &Self) -> Self {
        Self {
            plugin_name: if overlay.plugin_name.is_empty() {
                self.plugin_name.clone()
            } else {
                overlay.plugin_name.clone()
            },
            phase: overlay.phase.clone().or(self.phase.clone()),
            last_error: overlay.last_error.clone().or(self.last_error.clone()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginManifest {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub kind: PluginKind,
    pub summary: String,
    #[serde(default)]
    pub metadata: PluginMetadata,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<PluginPermission>,
    #[serde(default)]
    pub lifecycle: PluginLifecycleDefinition,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<PluginCommandManifest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<PluginToolManifest>,
}

fn default_schema_version() -> u32 {
    1
}

impl PluginManifest {
    pub fn validate(&self) -> Result<(), PluginManifestError> {
        if self.name.trim().is_empty() {
            return Err(PluginManifestError::InvalidManifest {
                path: None,
                message: "plugin name must not be empty".to_string(),
            });
        }

        if self.summary.trim().is_empty() {
            return Err(PluginManifestError::InvalidManifest {
                path: None,
                message: format!("plugin `{}` summary must not be empty", self.name),
            });
        }

        for permission in &self.permissions {
            permission.validate(&self.name)?;
        }

        for command in &self.commands {
            let entry = command.to_command_manifest(&self.name);
            validate_manifest_entry(&entry).map_err(|err| PluginManifestError::InvalidCommand {
                plugin_name: self.name.clone(),
                source: err,
            })?;
        }

        for tool in &self.tools {
            if tool.name.trim().is_empty() {
                return Err(PluginManifestError::InvalidTool {
                    plugin_name: self.name.clone(),
                    tool_name: tool.name.clone(),
                    message: "tool name must not be empty".to_string(),
                });
            }

            if tool.description.trim().is_empty() {
                return Err(PluginManifestError::InvalidTool {
                    plugin_name: self.name.clone(),
                    tool_name: tool.name.clone(),
                    message: "tool description must not be empty".to_string(),
                });
            }

            tool.permissions
                .validate(&format!("{}::{}", self.name, tool.name))?;
        }

        Ok(())
    }

    pub fn command_manifests(&self) -> Vec<CommandManifestEntry> {
        self.commands
            .iter()
            .map(|command| command.to_command_manifest(&self.name))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginDiscoverySource {
    Bundled,
    Directory,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiscoveredPlugin {
    pub source: PluginDiscoverySource,
    pub root: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest: PluginManifest,
}

#[derive(Debug, Error)]
pub enum PluginManifestError {
    #[error("failed to read plugin manifest {path}: {source}")]
    ReadManifest {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse plugin manifest {path}: {source}")]
    ParseManifest {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("invalid plugin manifest: {message}")]
    InvalidManifest {
        path: Option<PathBuf>,
        message: String,
    },
    #[error("invalid permission for plugin `{plugin_name}`: {message}")]
    InvalidPermission {
        plugin_name: String,
        message: String,
    },
    #[error("invalid command for plugin `{plugin_name}`: {source}")]
    InvalidCommand {
        plugin_name: String,
        source: InputValidationError,
    },
    #[error("invalid tool `{tool_name}` for plugin `{plugin_name}`: {message}")]
    InvalidTool {
        plugin_name: String,
        tool_name: String,
        message: String,
    },
    #[error("failed to read plugin directory {path}: {source}")]
    ReadDirectory {
        path: PathBuf,
        source: std::io::Error,
    },
}

pub fn plugin_surface() -> SurfaceDescriptor {
    SurfaceDescriptor {
        name: "plugins".into(),
        role: "plugin loading boundary".into(),
    }
}

pub fn bundled_plugin_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("bundled")
}

pub fn parse_plugin_manifest(contents: &str) -> Result<PluginManifest, PluginManifestError> {
    let manifest: PluginManifest =
        serde_json::from_str(contents).map_err(|source| PluginManifestError::ParseManifest {
            path: PathBuf::from("<inline>"),
            source,
        })?;
    manifest.validate()?;
    Ok(manifest)
}

pub fn load_plugin_manifest(path: &Path) -> Result<PluginManifest, PluginManifestError> {
    let contents =
        fs::read_to_string(path).map_err(|source| PluginManifestError::ReadManifest {
            path: path.to_path_buf(),
            source,
        })?;
    let manifest: PluginManifest =
        serde_json::from_str(&contents).map_err(|source| PluginManifestError::ParseManifest {
            path: path.to_path_buf(),
            source,
        })?;
    manifest.validate().map_err(|err| match err {
        PluginManifestError::InvalidManifest { message, .. } => {
            PluginManifestError::InvalidManifest {
                path: Some(path.to_path_buf()),
                message,
            }
        }
        other => other,
    })?;
    Ok(manifest)
}

pub fn discover_plugins_in(root: &Path) -> Result<Vec<DiscoveredPlugin>, PluginManifestError> {
    let entries = fs::read_dir(root).map_err(|source| PluginManifestError::ReadDirectory {
        path: root.to_path_buf(),
        source,
    })?;

    let mut plugins = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|source| PluginManifestError::ReadDirectory {
            path: root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let manifest_path = path.join("plugin.json");
        if !manifest_path.is_file() {
            continue;
        }

        let manifest = load_plugin_manifest(&manifest_path)?;
        plugins.push(DiscoveredPlugin {
            source: PluginDiscoverySource::Directory,
            root: path,
            manifest_path,
            manifest,
        });
    }

    plugins.sort_by(|left, right| left.manifest.name.cmp(&right.manifest.name));
    Ok(plugins)
}

pub fn discover_bundled_plugins() -> Result<Vec<DiscoveredPlugin>, PluginManifestError> {
    let root = bundled_plugin_root();
    let mut plugins = discover_plugins_in(&root)?;
    for plugin in &mut plugins {
        plugin.source = PluginDiscoverySource::Bundled;
    }
    Ok(plugins)
}

pub fn plugin_manifests() -> Vec<PluginManifest> {
    discover_bundled_plugins()
        .unwrap_or_default()
        .into_iter()
        .map(|plugin| plugin.manifest)
        .collect()
}

pub fn plugin_command_manifests() -> Vec<CommandManifestEntry> {
    plugin_manifests()
        .into_iter()
        .flat_map(|manifest| manifest.command_manifests())
        .collect()
}

pub fn plugin_tool_manifests() -> Vec<PluginToolManifest> {
    plugin_manifests()
        .into_iter()
        .flat_map(|manifest| manifest.tools)
        .collect()
}

#[cfg(test)]
mod tests;
