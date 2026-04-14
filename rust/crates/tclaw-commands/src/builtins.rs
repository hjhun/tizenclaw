use crate::{CommandManifestEntry, CommandSource, ResumeBehavior, SlashCommandArgHint};

pub fn built_in_command_manifests() -> Vec<CommandManifestEntry> {
    vec![
        CommandManifestEntry::new("help", CommandSource::BuiltIn, "List available slash commands")
            .with_aliases(["h"])
            .with_argument_hints([SlashCommandArgHint::optional(
                "command",
                "Optional command name to inspect",
            )]),
        CommandManifestEntry::new(
            "plugins",
            CommandSource::BuiltIn,
            "Inspect plugin-provided command manifests",
        )
        .with_aliases(["plugin"])
        .with_argument_hints([SlashCommandArgHint::optional(
            "plugin",
            "Optional plugin name to filter",
        )]),
        CommandManifestEntry::new(
            "resume",
            CommandSource::BuiltIn,
            "Resume a recorded session or continuation point",
        )
        .with_aliases(["continue"])
        .with_argument_hints([
            SlashCommandArgHint::required("session", "Session identifier to resume"),
            SlashCommandArgHint::optional("message", "Optional continuation note"),
        ])
        .with_resume_behavior(ResumeBehavior::ResumeOnly),
    ]
}
