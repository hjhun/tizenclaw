use tclaw_api::SurfaceDescriptor;
use tclaw_commands::{
    CommandManifestEntry, CommandSource, ResumeBehavior, SlashCommandArgHint,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManifest {
    pub name: String,
    pub summary: String,
    pub commands: Vec<CommandManifestEntry>,
}

pub fn plugin_surface() -> SurfaceDescriptor {
    SurfaceDescriptor {
        name: "plugins".into(),
        role: "plugin loading boundary".into(),
    }
}

pub fn plugin_manifests() -> Vec<PluginManifest> {
    vec![PluginManifest {
        name: "metadata".to_string(),
        summary: "Metadata-derived commands and resume helpers".to_string(),
        commands: vec![
            CommandManifestEntry::new(
                "metadata.sync",
                CommandSource::Plugin {
                    plugin_name: "metadata".to_string(),
                },
                "Refresh plugin metadata and command annotations",
            )
            .with_aliases(["meta-sync"])
            .with_argument_hints([SlashCommandArgHint::optional(
                "scope",
                "Optional metadata scope to refresh",
            )]),
            CommandManifestEntry::new(
                "metadata.resume",
                CommandSource::Plugin {
                    plugin_name: "metadata".to_string(),
                },
                "Inspect plugin-provided resume markers",
            )
            .with_aliases(["meta-resume"])
            .with_argument_hints([SlashCommandArgHint::optional(
                "session",
                "Optional session identifier to inspect",
            )])
            .with_resume_behavior(ResumeBehavior::Supported),
        ],
    }]
}

pub fn plugin_command_manifests() -> Vec<CommandManifestEntry> {
    plugin_manifests()
        .into_iter()
        .flat_map(|manifest| manifest.commands)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
