use std::collections::BTreeMap;

use thiserror::Error;
use tclaw_runtime::{ToolCallRequest, ToolExecutionOutput, ToolRuntimeError};

use crate::manifest::ToolManifestEntry;

pub trait ToolHandler<C> {
    fn execute(
        &mut self,
        call: &ToolCallRequest,
        context: &mut C,
    ) -> Result<ToolExecutionOutput, ToolRuntimeError>;
}

impl<C, F> ToolHandler<C> for F
where
    F: FnMut(&ToolCallRequest, &mut C) -> Result<ToolExecutionOutput, ToolRuntimeError>,
{
    fn execute(
        &mut self,
        call: &ToolCallRequest,
        context: &mut C,
    ) -> Result<ToolExecutionOutput, ToolRuntimeError> {
        self(call, context)
    }
}

pub struct ToolRegistration<C> {
    pub manifest: ToolManifestEntry,
    handler: Box<dyn ToolHandler<C>>,
}

impl<C> ToolRegistration<C> {
    pub fn new(
        manifest: ToolManifestEntry,
        handler: impl ToolHandler<C> + 'static,
    ) -> Self {
        Self {
            manifest,
            handler: Box::new(handler),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ToolRegistryError {
    #[error("tool `{name}` is already registered")]
    DuplicateTool { name: String },
    #[error("tool alias `{alias}` conflicts with `{existing}`")]
    AliasConflict { alias: String, existing: String },
    #[error("tool `{name}` is not registered")]
    UnknownTool { name: String },
}

#[derive(Debug, Clone, Default)]
pub struct ToolCatalog {
    manifests: BTreeMap<String, ToolManifestEntry>,
    aliases: BTreeMap<String, String>,
}

impl ToolCatalog {
    pub fn manifests(&self) -> Vec<ToolManifestEntry> {
        self.manifests.values().cloned().collect()
    }

    pub fn get(&self, name: &str) -> Option<&ToolManifestEntry> {
        let resolved = self.resolve_name(name)?;
        self.manifests.get(resolved)
    }

    pub fn resolve_name(&self, name: &str) -> Option<&str> {
        if self.manifests.contains_key(name) {
            return Some(name);
        }

        self.aliases.get(name).map(String::as_str)
    }

    pub fn search(&self, query: &str) -> Vec<ToolManifestEntry> {
        self.manifests
            .values()
            .filter(|manifest| manifest.matches_query(query))
            .cloned()
            .collect()
    }

    fn insert(&mut self, manifest: ToolManifestEntry) -> Result<(), ToolRegistryError> {
        if self.manifests.contains_key(&manifest.name) {
            return Err(ToolRegistryError::DuplicateTool {
                name: manifest.name,
            });
        }

        for alias in &manifest.aliases {
            if self.manifests.contains_key(alias) {
                return Err(ToolRegistryError::AliasConflict {
                    alias: alias.clone(),
                    existing: alias.clone(),
                });
            }

            if let Some(existing) = self.aliases.get(alias) {
                return Err(ToolRegistryError::AliasConflict {
                    alias: alias.clone(),
                    existing: existing.clone(),
                });
            }
        }

        for alias in &manifest.aliases {
            self.aliases.insert(alias.clone(), manifest.name.clone());
        }
        self.manifests.insert(manifest.name.clone(), manifest);
        Ok(())
    }
}

pub struct ToolRegistry<C> {
    catalog: ToolCatalog,
    handlers: BTreeMap<String, Box<dyn ToolHandler<C>>>,
}

impl<C> Default for ToolRegistry<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C> ToolRegistry<C> {
    pub fn new() -> Self {
        Self {
            catalog: ToolCatalog::default(),
            handlers: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, registration: ToolRegistration<C>) -> Result<(), ToolRegistryError> {
        let name = registration.manifest.name.clone();
        self.catalog.insert(registration.manifest)?;
        self.handlers.insert(name, registration.handler);
        Ok(())
    }

    pub fn manifests(&self) -> Vec<ToolManifestEntry> {
        self.catalog.manifests()
    }

    pub fn search(&self, query: &str) -> Vec<ToolManifestEntry> {
        self.catalog.search(query)
    }

    pub fn get(&self, name: &str) -> Option<&ToolManifestEntry> {
        self.catalog.get(name)
    }

    pub fn execute(
        &mut self,
        call: &ToolCallRequest,
        context: &mut C,
    ) -> Result<ToolExecutionOutput, ToolRuntimeError> {
        let resolved_name = self
            .catalog
            .resolve_name(&call.name)
            .ok_or_else(|| ToolRuntimeError::Execution {
                tool_name: call.name.clone(),
                message: "tool is not registered".to_string(),
            })?
            .to_string();
        let handler = self
            .handlers
            .get_mut(&resolved_name)
            .ok_or_else(|| ToolRuntimeError::Execution {
                tool_name: resolved_name.clone(),
                message: "tool handler is missing".to_string(),
            })?;

        let mut canonical_call = call.clone();
        canonical_call.name = resolved_name;
        handler.execute(&canonical_call, context)
    }

    pub fn into_executor(self, context: C) -> crate::RegistryToolExecutor<C> {
        crate::RegistryToolExecutor::new(self, context)
    }

    pub fn into_permissioned_executor<P>(
        self,
        context: C,
        config: tclaw_runtime::RuntimeConfig,
        permissions: P,
    ) -> crate::PermissionAwareToolExecutor<C, P>
    where
        P: tclaw_runtime::PermissionResolver,
    {
        crate::PermissionAwareToolExecutor::new(self, context, config, permissions)
    }
}
