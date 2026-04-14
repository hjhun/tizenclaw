use std::collections::BTreeMap;

use serde_json::json;

use super::*;

#[test]
fn parses_plugin_manifest_with_permissions_and_commands() {
    let manifest = parse_plugin_manifest(
        r#"{
            "schema_version": 1,
            "name": "metadata",
            "kind": "metadata",
            "summary": "Metadata plugin",
            "metadata": {
                "version": "1.0.0",
                "authors": ["TizenClaw"]
            },
            "permissions": [
                {
                    "scope": "read",
                    "level": "standard",
                    "target": "plugin-registry"
                }
            ],
            "lifecycle": {
                "default_phase": "discovered",
                "hooks": [
                    {
                        "name": "before-tool",
                        "phase": "pre_tool",
                        "command": "hooks/pre.sh",
                        "enabled": true
                    }
                ]
            },
            "commands": [
                {
                    "name": "metadata.sync",
                    "summary": "Refresh metadata",
                    "aliases": ["meta-sync"]
                }
            ],
            "tools": [
                {
                    "name": "metadata.sync",
                    "description": "Refresh metadata annotations",
                    "input_schema": { "type": "object" },
                    "permissions": {
                        "scope": "read",
                        "level": "standard",
                        "target": "plugin-registry"
                    }
                }
            ]
        }"#,
    )
    .expect("parse manifest");

    assert_eq!(manifest.name, "metadata");
    assert_eq!(manifest.kind, PluginKind::Metadata);
    assert_eq!(manifest.lifecycle.default_phase, Some(PluginLifecyclePhase::Discovered));
    assert_eq!(manifest.command_manifests()[0].canonical_name, "metadata.sync");
    assert_eq!(manifest.tools[0].permissions.scope, PluginPermissionScope::Read);
}

#[test]
fn rejects_invalid_execute_permission_without_reason() {
    let error = parse_plugin_manifest(
        r#"{
            "name": "bad-plugin",
            "kind": "tooling",
            "summary": "Broken permissions",
            "tools": [
                {
                    "name": "bad.exec",
                    "description": "Runs something",
                    "input_schema": { "type": "object" },
                    "permissions": {
                        "scope": "execute",
                        "level": "sensitive",
                        "target": "hooks/pre.sh"
                    }
                }
            ]
        }"#,
    )
    .expect_err("invalid execute permission should fail");

    match error {
        PluginManifestError::InvalidPermission { .. } => {}
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn lifecycle_merging_prefers_overlay_phase_and_keeps_hooks() {
    let base = PluginLifecycleDefinition {
        default_phase: Some(PluginLifecyclePhase::Discovered),
        hooks: vec![HookSpec {
            name: "base".to_string(),
            phase: HookPhase::PrePrompt,
            command: "hooks/pre.sh".to_string(),
            enabled: true,
            env: BTreeMap::new(),
        }],
    };
    let overlay = PluginLifecycleDefinition {
        default_phase: Some(PluginLifecyclePhase::Active),
        hooks: vec![HookSpec {
            name: "overlay".to_string(),
            phase: HookPhase::PostSession,
            command: "hooks/post.sh".to_string(),
            enabled: true,
            env: BTreeMap::new(),
        }],
    };

    let merged = base.merged_with(&overlay);

    assert_eq!(merged.default_phase, Some(PluginLifecyclePhase::Active));
    assert_eq!(merged.hooks.len(), 2);
    assert_eq!(merged.hooks[0].name, "base");
    assert_eq!(merged.hooks[1].name, "overlay");
}

#[test]
fn bundled_examples_are_discoverable_and_expose_tools() {
    let plugins = discover_bundled_plugins().expect("discover bundled plugins");

    assert!(plugins
        .iter()
        .any(|plugin| plugin.root.ends_with("example-bundled")));
    assert!(plugins
        .iter()
        .flat_map(|plugin| plugin.manifest.tools.iter())
        .any(|tool| tool.name == "metadata.sync"));
    assert!(plugins
        .iter()
        .flat_map(|plugin| plugin.manifest.lifecycle.hooks.iter())
        .any(|hook| hook.command.ends_with("hooks/pre.sh")));
}

#[test]
fn plugin_commands_are_tagged_with_plugin_source() {
    let commands = plugin_command_manifests();

    assert!(commands.iter().all(|command| matches!(
        &command.source,
        CommandSource::Plugin { .. }
    )));
    assert!(commands
        .iter()
        .any(|command| command.canonical_name == "metadata.resume"));
}

#[test]
fn plugin_tools_publish_input_schemas() {
    let tools = plugin_tool_manifests();

    assert!(tools.iter().any(|tool| tool.name == "metadata.sync"));
    assert_eq!(
        tools
            .iter()
            .find(|tool| tool.name == "metadata.sync")
            .expect("metadata tool")
            .input_schema,
        json!({
            "type": "object",
            "properties": {
                "scope": {"type": "string"}
            }
        })
    );
}
