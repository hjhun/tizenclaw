use tclaw_commands::{
    built_in_command_manifests, CommandManifestEntry, CommandRegistry, CommandRegistryError,
};
use tclaw_plugins::plugin_command_manifests;

pub fn runtime_command_manifests() -> Vec<CommandManifestEntry> {
    built_in_command_manifests()
        .into_iter()
        .chain(plugin_command_manifests())
        .collect()
}

pub fn runtime_command_registry() -> Result<CommandRegistry, CommandRegistryError> {
    CommandRegistry::from_entries(runtime_command_manifests())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_registry_contains_built_in_and_plugin_commands() {
        let registry = runtime_command_registry().expect("registry");

        assert!(registry.get("help").is_some());
        assert!(registry.get("metadata.sync").is_some());
        assert!(!registry.plugin_commands().is_empty());
    }
}
