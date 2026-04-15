use tclaw_runtime::{
    runtime_command_registry, ResumeBehavior, RuntimeBootstrap, RuntimeConfig, RuntimeConfigPatch,
    RuntimeProfile,
};

#[test]
fn runtime_bootstrap_exposes_documented_surfaces_and_modules() {
    let bootstrap = RuntimeBootstrap::new();

    assert_eq!(bootstrap.canonical_runtime, "rust");
    assert!(bootstrap
        .surfaces
        .iter()
        .any(|surface| surface.name == "runtime"));
    assert!(bootstrap
        .surfaces
        .iter()
        .any(|surface| surface.name == "api"));
    assert!(bootstrap
        .modules
        .modules
        .contains(&"conversation".to_string()));
    assert!(bootstrap
        .modules
        .modules
        .contains(&"worker_boot".to_string()));
}

#[test]
fn runtime_registry_keeps_built_in_and_plugin_resume_contracts() {
    let registry = runtime_command_registry().expect("registry");
    let resume = registry.resolve("resume").expect("resume");
    let plugin = registry.resolve("metadata.resume").expect("plugin resume");

    assert_eq!(resume.metadata.resume_behavior, ResumeBehavior::ResumeOnly);
    assert_eq!(plugin.metadata.resume_behavior, ResumeBehavior::Supported);
}

#[test]
fn runtime_config_patch_updates_only_selected_fields() {
    let mut config = RuntimeConfig::default();
    config.apply_patch(RuntimeConfigPatch {
        profile: Some(RuntimeProfile::Test),
        plugin_roots: Some(vec!["plugins".to_string(), "custom".to_string()]),
        ..RuntimeConfigPatch::default()
    });

    assert_eq!(config.profile, RuntimeProfile::Test);
    assert_eq!(
        config.plugin_roots,
        vec!["plugins".to_string(), "custom".to_string()]
    );
    assert!(config.hooks_enabled);
}
