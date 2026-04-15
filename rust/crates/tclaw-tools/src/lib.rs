mod builtins;
mod executor;
mod manifest;
mod registry;

use std::{cell::RefCell, rc::Rc};

use serde_json::Value;
use tclaw_api::SurfaceDescriptor;
use tclaw_plugins::{
    PluginPermission, PluginPermissionLevel, PluginPermissionScope, PluginToolManifest,
};
use tclaw_runtime::{
    McpToolBridge, PermissionLevel, PermissionScope, ToolCallRequest, ToolRuntimeError,
};

pub use builtins::{
    built_in_tool_registry, FetchToolBackend, FileToolBackend, GlobalToolContext,
    InMemoryFileBackend, NullFetchToolBackend, NullFileToolBackend, NullShellToolBackend,
    ShellToolBackend, StaticFetchToolBackend, StaticShellToolBackend, TextSearchMatch,
};
pub use executor::{PermissionAwareToolExecutor, RegistryToolExecutor};
pub use manifest::{ToolManifestEntry, ToolPermissionSpec, ToolSource};
pub use registry::{ToolCatalog, ToolHandler, ToolRegistration, ToolRegistry, ToolRegistryError};

pub fn tool_surface() -> SurfaceDescriptor {
    SurfaceDescriptor {
        name: "tools".into(),
        role: "tool registry and execution boundary".into(),
    }
}

pub fn runtime_tool_registration(
    provider: impl Into<String>,
    name: impl Into<String>,
    description: impl Into<String>,
    input_schema: Value,
    permissions: ToolPermissionSpec,
    handler: impl ToolHandler<GlobalToolContext> + 'static,
) -> ToolRegistration<GlobalToolContext> {
    ToolRegistration::new(
        ToolManifestEntry::new(
            name,
            ToolSource::Runtime {
                provider: provider.into(),
            },
            description,
            input_schema,
        )
        .with_permissions(permissions),
        handler,
    )
}

pub fn plugin_tool_registration(
    plugin_name: impl Into<String>,
    manifest: PluginToolManifest,
    permissions: ToolPermissionSpec,
    handler: impl ToolHandler<GlobalToolContext> + 'static,
) -> ToolRegistration<GlobalToolContext> {
    let plugin_name = plugin_name.into();
    let mut entry = ToolManifestEntry::new(
        manifest.name,
        ToolSource::Plugin {
            plugin_name: plugin_name.clone(),
        },
        manifest.description,
        manifest.input_schema,
    )
    .with_aliases(manifest.aliases)
    .with_permissions(permissions)
    .with_tags(
        manifest
            .tags
            .into_iter()
            .chain(std::iter::once(plugin_name.clone())),
    );

    for (key, value) in manifest.metadata {
        entry = entry.with_metadata(key, value);
    }

    ToolRegistration::new(entry, handler)
}

fn tool_permission_spec_from_plugin_permission(
    permission: &PluginPermission,
    fallback_target: &str,
    fallback_reason: &str,
) -> ToolPermissionSpec {
    ToolPermissionSpec::new(
        match permission.scope {
            PluginPermissionScope::Read => PermissionScope::Read,
            PluginPermissionScope::Write => PermissionScope::Write,
            PluginPermissionScope::Execute => PermissionScope::Execute,
            PluginPermissionScope::Network => PermissionScope::Network,
        },
        match permission.level {
            PluginPermissionLevel::Low => PermissionLevel::Low,
            PluginPermissionLevel::Standard => PermissionLevel::Standard,
            PluginPermissionLevel::Sensitive => PermissionLevel::Sensitive,
        },
    )
    .with_target(
        permission
            .target
            .clone()
            .unwrap_or_else(|| fallback_target.to_string()),
    )
    .with_reason(
        permission
            .reason
            .clone()
            .unwrap_or_else(|| fallback_reason.to_string()),
    )
}

pub fn plugin_manifest_entries(plugin_name: impl Into<String>) -> Vec<ToolManifestEntry> {
    let plugin_name = plugin_name.into();
    tclaw_plugins::plugin_tool_manifests()
        .into_iter()
        .map(|manifest| {
            let fallback_target = manifest.name.clone();
            let fallback_reason = format!("execute plugin tool {}", manifest.name);
            let mut entry = ToolManifestEntry::new(
                manifest.name,
                ToolSource::Plugin {
                    plugin_name: plugin_name.clone(),
                },
                manifest.description,
                manifest.input_schema,
            )
            .with_aliases(manifest.aliases)
            .with_permissions(tool_permission_spec_from_plugin_permission(
                &manifest.permissions,
                &fallback_target,
                &fallback_reason,
            ))
            .with_tags(
                manifest
                    .tags
                    .into_iter()
                    .chain(std::iter::once(plugin_name.clone())),
            );

            for (key, value) in manifest.metadata {
                entry = entry.with_metadata(key, value);
            }

            entry
        })
        .collect()
}

pub fn register_mcp_tools(
    registry: &mut ToolRegistry<GlobalToolContext>,
    bridge: Rc<RefCell<McpToolBridge>>,
) -> Result<(), ToolRegistryError> {
    let manifests = bridge.borrow().bridged_tool_manifests();
    for manifest in manifests {
        let full_name = manifest.full_name.clone();
        let original_name = manifest.original_name.clone();
        let server_name = manifest.server_name.clone();
        let handler_bridge = Rc::clone(&bridge);
        registry.register(ToolRegistration::new(
            ToolManifestEntry::new(
                full_name.clone(),
                ToolSource::Mcp {
                    server_name,
                    original_name,
                },
                manifest.description,
                manifest.input_schema,
            )
            .with_permissions(ToolPermissionSpec::new(
                manifest.permission_scope,
                manifest.minimum_permission_level,
            )),
            move |call: &ToolCallRequest, _ctx: &mut GlobalToolContext| {
                handler_bridge
                    .borrow_mut()
                    .execute(call)
                    .unwrap_or_else(|| {
                        Err(ToolRuntimeError::Execution {
                            tool_name: full_name.clone(),
                            message: "MCP bridge did not recognize the tool".to_string(),
                        })
                    })
            },
        ))?;
    }

    Ok(())
}

pub fn global_tool_registry(
    runtime_tools: Vec<ToolRegistration<GlobalToolContext>>,
    plugin_tools: Vec<ToolRegistration<GlobalToolContext>>,
    mcp_bridge: Option<Rc<RefCell<McpToolBridge>>>,
) -> Result<ToolRegistry<GlobalToolContext>, ToolRegistryError> {
    let mut registry = built_in_tool_registry()?;

    for registration in runtime_tools {
        registry.register(registration)?;
    }

    for registration in plugin_tools {
        registry.register(registration)?;
    }

    if let Some(bridge) = mcp_bridge {
        register_mcp_tools(&mut registry, bridge)?;
    }

    Ok(registry)
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::time::Duration;

    use serde_json::json;
    use tclaw_runtime::{
        config::RuntimeConfig, mcp::JsonRpcNotification, mcp::JsonRpcRequest, mcp::JsonRpcResponse,
        mcp::McpContentBlock, mcp::McpInitializeResult, mcp::McpListResourcesResult,
        mcp::McpListToolsResult, mcp::McpPeerCapabilities, mcp::McpServerInfo,
        mcp::McpToolCallResult, mcp::McpToolDefinition, mcp_client::McpClient,
        mcp_client::McpClientSpec, mcp_lifecycle_hardened::ManagedMcpServer,
        mcp_lifecycle_hardened::McpLifecyclePolicy, mcp_stdio::McpStdioServerSpec,
        mcp_stdio::McpTransport, mcp_stdio::McpTransportError, PermissionEnforcer, PermissionMode,
        RecordingPrompter, RuntimeProfile, ToolExecutionOutput, ToolExecutor, WorkerBootSpec,
        WorkerBootState, WorkerIdentity, WorkerKind,
    };

    use super::*;

    struct ScriptedTransport {
        responses: VecDeque<JsonRpcResponse>,
    }

    impl McpTransport for ScriptedTransport {
        fn send_request(
            &mut self,
            _request: &JsonRpcRequest,
            _timeout: Duration,
        ) -> Result<JsonRpcResponse, McpTransportError> {
            self.responses
                .pop_front()
                .ok_or(McpTransportError::ProcessExited {
                    message: "script exhausted".to_string(),
                })
        }

        fn send_notification(
            &mut self,
            _notification: &JsonRpcNotification,
        ) -> Result<(), McpTransportError> {
            Ok(())
        }

        fn is_running(&mut self) -> bool {
            true
        }

        fn close(&mut self) {}

        fn description(&self) -> String {
            "scripted".to_string()
        }
    }

    fn test_context() -> GlobalToolContext {
        let mut files = InMemoryFileBackend::default();
        FileToolBackend::write_text(
            &mut files,
            "src/lib.rs",
            "pub fn registry() {}\n// registry helper\npub fn tool() {}\n",
        )
        .expect("seed file");

        GlobalToolContext {
            file_backend: Box::new(files),
            shell_backend: Box::new(StaticShellToolBackend::with_command(
                "echo",
                json!({"stdout": "ok", "status": 0}),
            )),
            fetch_backend: Box::new(StaticFetchToolBackend::with_document(
                "https://example.test/tools.json",
                json!({"tools": ["a", "b"]}),
            )),
            task_registry: tclaw_runtime::TaskRegistrySnapshot {
                active_tasks: vec![tclaw_runtime::TaskPacket {
                    task_id: "task-1".to_string(),
                    summary: "refresh manifests".to_string(),
                    priority: tclaw_runtime::TaskPriority::High,
                    labels: vec!["tools".to_string()],
                    status: tclaw_runtime::TaskStatus::Queued,
                    assignment: Some(tclaw_runtime::TaskAssignment {
                        lane_id: "tools".to_string(),
                        worker_id: None,
                        session_id: None,
                    }),
                    trust: None,
                    metadata: std::collections::BTreeMap::new(),
                    failure: None,
                }],
                completed_tasks: vec!["task-0".to_string()],
                failed_tasks: Vec::new(),
                lane_events: Vec::new(),
            },
            cron_registry: tclaw_runtime::TeamCronRegistry {
                entries: vec![tclaw_runtime::TeamCronEntry {
                    entry_id: "cron-1".to_string(),
                    schedule: "0 * * * *".to_string(),
                    task_name: "metadata.sync".to_string(),
                    lane_id: "maintenance".to_string(),
                    enabled: true,
                    priority: tclaw_runtime::TaskPriority::Normal,
                    labels: vec!["cron".to_string()],
                    trust: None,
                }],
            },
            workers: vec![WorkerBootSpec {
                identity: WorkerIdentity {
                    worker_id: "worker-1".to_string(),
                    display_name: Some("Planner".to_string()),
                },
                kind: WorkerKind::Explorer,
                state: WorkerBootState::Ready,
                inherited_session_id: Some("session-1".to_string()),
            }],
            lsp_clients: vec![tclaw_runtime::LspClientSpec {
                language: "rust".to_string(),
                command: "rust-analyzer".to_string(),
                root_uri: Some("file:///repo".to_string()),
            }],
        }
    }

    fn allow_all_config() -> RuntimeConfig {
        RuntimeConfig {
            profile: RuntimeProfile::Test,
            permission_mode: PermissionMode::AllowAll,
            sandbox_enabled: false,
            ..RuntimeConfig::default()
        }
    }

    #[test]
    fn built_in_tools_support_file_shell_and_registry_flows() {
        let registry = built_in_tool_registry().expect("registry");
        let mut executor = registry.into_executor(test_context());

        let write = executor
            .execute(&ToolCallRequest {
                id: "call-1".to_string(),
                name: "fs.write_text".to_string(),
                input: json!({
                    "path": "notes/tools.txt",
                    "content": "tool registry ready"
                }),
            })
            .expect("write text");
        assert_eq!(write.output["path"], "notes/tools.txt");

        let read = executor
            .execute(&ToolCallRequest {
                id: "call-2".to_string(),
                name: "read_file".to_string(),
                input: json!({"path": "notes/tools.txt"}),
            })
            .expect("read text");
        assert_eq!(read.output["content"], "tool registry ready");

        let search = executor
            .execute(&ToolCallRequest {
                id: "call-3".to_string(),
                name: "fs.search_text".to_string(),
                input: json!({"path": "src/lib.rs", "query": "tool"}),
            })
            .expect("search text");
        assert_eq!(search.output["matches"][0]["line_number"], 3);

        let shell = executor
            .execute(&ToolCallRequest {
                id: "call-4".to_string(),
                name: "shell.exec".to_string(),
                input: json!({"program": "echo", "args": ["hello"]}),
            })
            .expect("shell exec");
        assert_eq!(shell.output["stdout"], "ok");

        let tasks = executor
            .execute(&ToolCallRequest {
                id: "call-5".to_string(),
                name: "registry.list_tasks".to_string(),
                input: json!({}),
            })
            .expect("list tasks");
        assert_eq!(tasks.output["active_tasks"][0]["task_id"], "task-1");

        let lsp = executor
            .execute(&ToolCallRequest {
                id: "call-6".to_string(),
                name: "automation.list_lsp_clients".to_string(),
                input: json!({}),
            })
            .expect("list lsp");
        assert_eq!(lsp.output["clients"][0]["language"], "rust");
    }

    #[test]
    fn permission_wrapper_denies_sensitive_tool_execution() {
        let registry = built_in_tool_registry().expect("registry");
        let config = RuntimeConfig {
            profile: RuntimeProfile::Test,
            permission_mode: PermissionMode::DenyAll,
            sandbox_enabled: false,
            ..RuntimeConfig::default()
        };
        let permissions = PermissionEnforcer::default();
        let mut executor = registry.into_permissioned_executor(test_context(), config, permissions);

        let error = executor
            .execute(&ToolCallRequest {
                id: "call-1".to_string(),
                name: "shell.exec".to_string(),
                input: json!({"program": "echo", "args": ["hello"]}),
            })
            .expect_err("shell tool should be denied");

        assert!(matches!(
            error,
            ToolRuntimeError::PermissionDenied { tool_name, .. } if tool_name == "shell.exec"
        ));
    }

    #[test]
    fn permission_wrapper_allows_fetch_tool_and_records_decision() {
        let registry = built_in_tool_registry().expect("registry");
        let permissions =
            PermissionEnforcer::with_prompter(RecordingPrompter::with_decisions(vec![
                tclaw_runtime::PermissionPromptDecision::AllowOnce,
            ]));
        let mut executor =
            registry.into_permissioned_executor(test_context(), allow_all_config(), permissions);

        let result = executor
            .execute(&ToolCallRequest {
                id: "call-1".to_string(),
                name: "net.fetch_json".to_string(),
                input: json!({"url": "https://example.test/tools.json"}),
            })
            .expect("fetch tool");

        assert_eq!(result.output["document"]["tools"][1], "b");
        assert!(
            executor
                .permissions()
                .state()
                .last_decision
                .as_ref()
                .expect("decision")
                .allowed
        );
    }

    #[test]
    fn global_registry_composes_builtin_runtime_plugin_and_mcp_tools() {
        let runtime_tool = runtime_tool_registration(
            "runtime-tests",
            "runtime.echo",
            "Echo a runtime-provided payload",
            json!({"type": "object", "properties": {"message": {"type": "string"}}}),
            ToolPermissionSpec::new(PermissionScope::Read, PermissionLevel::Low),
            |call: &ToolCallRequest, _ctx: &mut GlobalToolContext| {
                Ok(ToolExecutionOutput {
                    tool_call_id: call.id.clone(),
                    output: json!({"message": call.input["message"].clone()}),
                    summary: Some("runtime echo".to_string()),
                })
            },
        );

        let plugin_tools = tclaw_plugins::plugin_tool_manifests()
            .into_iter()
            .map(|manifest| {
                let tool_name = manifest.name.clone();
                plugin_tool_registration(
                    "metadata",
                    manifest,
                    ToolPermissionSpec::new(PermissionScope::Execute, PermissionLevel::Standard),
                    move |call: &ToolCallRequest, _ctx: &mut GlobalToolContext| {
                        Ok(ToolExecutionOutput {
                            tool_call_id: call.id.clone(),
                            output: json!({"plugin_tool": tool_name, "input": call.input.clone()}),
                            summary: Some("plugin tool".to_string()),
                        })
                    },
                )
            })
            .collect();

        let responses = VecDeque::from(vec![
            JsonRpcResponse::success(
                1,
                serde_json::to_value(McpInitializeResult {
                    protocol_version: tclaw_runtime::mcp::MCP_PROTOCOL_VERSION.to_string(),
                    capabilities: McpPeerCapabilities::default(),
                    server_info: McpServerInfo {
                        name: "fake".to_string(),
                        version: "1.0.0".to_string(),
                    },
                })
                .expect("init value"),
            ),
            JsonRpcResponse::success(
                2,
                serde_json::to_value(McpListToolsResult {
                    tools: vec![McpToolDefinition {
                        name: "echo".to_string(),
                        description: "Echo tool".to_string(),
                        input_schema: json!({"type": "object"}),
                    }],
                })
                .expect("tool list value"),
            ),
            JsonRpcResponse::success(
                3,
                serde_json::to_value(McpListResourcesResult { resources: vec![] })
                    .expect("resource list value"),
            ),
            JsonRpcResponse::success(
                4,
                serde_json::to_value(McpToolCallResult {
                    content: vec![McpContentBlock::Text {
                        text: "from mcp".to_string(),
                    }],
                    structured_content: Some(json!({"ok": true})),
                    is_error: false,
                })
                .expect("call value"),
            ),
        ]);

        let client = McpClient::from_transport(
            "fake",
            McpClientSpec::default(),
            ScriptedTransport { responses },
        );
        let mut server = ManagedMcpServer::with_client(
            McpStdioServerSpec {
                server_name: "fake".to_string(),
                command: "python3".to_string(),
                ..McpStdioServerSpec::default()
            },
            McpClientSpec::default(),
            McpLifecyclePolicy::default(),
            client,
        );
        server.start().expect("start managed server");

        let mut bridge = McpToolBridge::default();
        bridge.register_server(server);
        let bridge = Rc::new(RefCell::new(bridge));

        let registry = global_tool_registry(vec![runtime_tool], plugin_tools, Some(bridge))
            .expect("global registry");
        let manifests = registry.manifests();

        assert!(manifests.iter().any(|tool| tool.name == "fs.read_text"));
        assert!(manifests.iter().any(|tool| tool.name == "runtime.echo"));
        assert!(manifests.iter().any(|tool| tool.name == "metadata.sync"));
        assert!(manifests.iter().any(|tool| tool.name == "mcp__fake__echo"));

        let search = registry.search("metadata");
        assert!(search.iter().any(|tool| tool.name == "metadata.sync"));
        assert!(search.iter().any(|tool| tool.name == "metadata.resume"));
    }

    #[test]
    fn manifest_search_matches_tags_and_aliases() {
        let registry = built_in_tool_registry().expect("registry");
        let file_results = registry.search("read_file");
        assert_eq!(file_results[0].name, "fs.read_text");

        let automation_results = registry.search("automation");
        assert!(automation_results
            .iter()
            .any(|tool| tool.name == "automation.list_workers"));
    }
}
