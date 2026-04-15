use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tclaw_runtime::{PermissionLevel, PermissionScope, ToolDefinition};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolSource {
    BuiltIn,
    Runtime {
        provider: String,
    },
    Plugin {
        plugin_name: String,
    },
    Mcp {
        server_name: String,
        original_name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolPermissionSpec {
    pub scope: PermissionScope,
    pub minimum_level: PermissionLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl ToolPermissionSpec {
    pub fn new(scope: PermissionScope, minimum_level: PermissionLevel) -> Self {
        Self {
            scope,
            minimum_level,
            target: None,
            reason: None,
        }
    }

    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolManifestEntry {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub description: String,
    pub input_schema: Value,
    pub source: ToolSource,
    pub permissions: ToolPermissionSpec,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

impl ToolManifestEntry {
    pub fn new(
        name: impl Into<String>,
        source: ToolSource,
        description: impl Into<String>,
        input_schema: Value,
    ) -> Self {
        Self {
            name: name.into(),
            aliases: Vec::new(),
            description: description.into(),
            input_schema,
            source,
            permissions: ToolPermissionSpec::new(PermissionScope::Read, PermissionLevel::Low),
            tags: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }

    pub fn with_aliases<I, S>(mut self, aliases: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.aliases = aliases.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_permissions(mut self, permissions: ToolPermissionSpec) -> Self {
        self.permissions = permissions;
        self
    }

    pub fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    pub fn matches_query(&self, query: &str) -> bool {
        if query.trim().is_empty() {
            return true;
        }

        let query = query.to_ascii_lowercase();
        self.name.to_ascii_lowercase().contains(&query)
            || self
                .aliases
                .iter()
                .any(|alias| alias.to_ascii_lowercase().contains(&query))
            || self.description.to_ascii_lowercase().contains(&query)
            || self
                .tags
                .iter()
                .any(|tag| tag.to_ascii_lowercase().contains(&query))
    }

    pub fn to_runtime_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name.clone(),
            description: self.description.clone(),
            permission_scope: self.permissions.scope.clone(),
            minimum_permission_level: self.permissions.minimum_level,
        }
    }
}
