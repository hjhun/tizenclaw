use std::collections::BTreeMap;

use tclaw_runtime::{
    BashCommand, BashExecutionPlan, PermissionRequest, PermissionResolver, RuntimeConfig,
    ToolCallRequest, ToolDefinition, ToolExecutionOutput, ToolExecutor, ToolRuntimeError,
};

use crate::{ToolManifestEntry, ToolRegistry};

pub struct RegistryToolExecutor<C> {
    registry: ToolRegistry<C>,
    context: C,
}

impl<C> RegistryToolExecutor<C> {
    pub fn new(registry: ToolRegistry<C>, context: C) -> Self {
        Self { registry, context }
    }

    pub fn registry(&self) -> &ToolRegistry<C> {
        &self.registry
    }

    pub fn context(&self) -> &C {
        &self.context
    }
}

impl<C> ToolExecutor for RegistryToolExecutor<C> {
    fn definitions(&self) -> Vec<ToolDefinition> {
        self.registry
            .manifests()
            .into_iter()
            .map(|manifest| manifest.to_runtime_definition())
            .collect()
    }

    fn execute(&mut self, call: &ToolCallRequest) -> Result<ToolExecutionOutput, ToolRuntimeError> {
        self.registry.execute(call, &mut self.context)
    }
}

pub struct PermissionAwareToolExecutor<C, P> {
    registry: ToolRegistry<C>,
    context: C,
    config: RuntimeConfig,
    permissions: P,
}

impl<C, P> PermissionAwareToolExecutor<C, P>
where
    P: PermissionResolver,
{
    pub fn new(
        registry: ToolRegistry<C>,
        context: C,
        config: RuntimeConfig,
        permissions: P,
    ) -> Self {
        Self {
            registry,
            context,
            config,
            permissions,
        }
    }

    pub fn permissions(&self) -> &P {
        &self.permissions
    }

    fn permission_request_for(
        manifest: &ToolManifestEntry,
        call: &ToolCallRequest,
    ) -> PermissionRequest {
        let mut metadata = BTreeMap::new();
        metadata.insert("tool_source".to_string(), format!("{:?}", manifest.source));
        metadata.insert("tool_name".to_string(), manifest.name.clone());

        PermissionRequest {
            scope: manifest.permissions.scope.clone(),
            target: permission_target(manifest, call),
            reason: manifest
                .permissions
                .reason
                .clone()
                .unwrap_or_else(|| format!("execute tool {}", manifest.name)),
            tool_name: Some(manifest.name.clone()),
            minimum_level: manifest.permissions.minimum_level,
            bash_plan: derive_bash_plan(call),
            metadata,
        }
    }
}

impl<C, P> ToolExecutor for PermissionAwareToolExecutor<C, P>
where
    P: PermissionResolver,
{
    fn definitions(&self) -> Vec<ToolDefinition> {
        self.registry
            .manifests()
            .into_iter()
            .map(|manifest| manifest.to_runtime_definition())
            .collect()
    }

    fn execute(&mut self, call: &ToolCallRequest) -> Result<ToolExecutionOutput, ToolRuntimeError> {
        let manifest =
            self.registry
                .get(&call.name)
                .cloned()
                .ok_or_else(|| ToolRuntimeError::Execution {
                    tool_name: call.name.clone(),
                    message: "tool is not registered".to_string(),
                })?;

        let decision = self
            .permissions
            .decide(&self.config, Self::permission_request_for(&manifest, call))
            .map_err(|error| ToolRuntimeError::Execution {
                tool_name: manifest.name.clone(),
                message: error.to_string(),
            })?;

        if !decision.allowed {
            return Err(ToolRuntimeError::PermissionDenied {
                tool_name: manifest.name,
                message: decision.rationale,
            });
        }

        self.registry.execute(call, &mut self.context)
    }
}

fn permission_target(manifest: &ToolManifestEntry, call: &ToolCallRequest) -> String {
    manifest
        .permissions
        .target
        .clone()
        .or_else(|| {
            call.input
                .get("path")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| {
            call.input
                .get("url")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| {
            call.input
                .get("program")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| manifest.name.clone())
}

fn derive_bash_plan(call: &ToolCallRequest) -> Option<BashExecutionPlan> {
    let program = call.input.get("program")?.as_str()?.to_string();
    let args = call
        .input
        .get("args")
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let working_dir = call
        .input
        .get("cwd")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);

    Some(BashExecutionPlan {
        commands: vec![BashCommand {
            program,
            args,
            working_dir,
        }],
        require_clean_environment: false,
    })
}
