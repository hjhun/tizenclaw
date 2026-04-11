use serde::Serialize;
use tclaw_plugins::{plugin_command_manifests, plugin_manifests, plugin_tool_manifests, PluginManifest};
use tclaw_runtime::{
    runtime_command_registry, CommandRegistry, RegistryParseOutcome, ResolvedSlashCommand,
    RuntimeBootstrap, RuntimeConfig, RuntimeProfile, SessionControlResult,
};
use tclaw_tools::built_in_tool_registry;

use crate::input::{merge_prompt_and_stdin, CliMode, OutputFormat, ParsedCli};

#[derive(Debug)]
pub enum CliDispatchError {
    CommandRegistry(String),
    ToolRegistry(String),
    SlashCommand(String),
}

impl std::fmt::Display for CliDispatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CommandRegistry(message) => write!(f, "{message}"),
            Self::ToolRegistry(message) => write!(f, "{message}"),
            Self::SlashCommand(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for CliDispatchError {}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CliOutcome {
    pub mode: String,
    pub output_format: OutputFormat,
    pub config: RuntimeConfig,
    pub runtime: RuntimeSummary,
    pub input: InputSummary,
    pub commands: CommandSummary,
    pub plugins: PluginSummary,
    pub tools: ToolSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<HelpSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume: Option<SessionControlResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slash_command: Option<ResolvedSlashCommand>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RuntimeSummary {
    pub canonical_runtime: String,
    pub surfaces: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InputSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merged_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CommandSummary {
    pub built_in_count: usize,
    pub plugin_count: usize,
    pub names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PluginSummary {
    pub count: usize,
    pub names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ToolSummary {
    pub built_in_count: usize,
    pub plugin_count: usize,
    pub names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HelpSummary {
    pub topic: String,
    pub title: String,
    pub lines: Vec<String>,
}

pub fn dispatch_cli(
    parsed: ParsedCli,
    stdin_text: Option<String>,
) -> Result<CliOutcome, CliDispatchError> {
    let registry = runtime_command_registry()
        .map_err(|error| CliDispatchError::CommandRegistry(error.to_string()))?;
    let built_in_tools = built_in_tool_registry()
        .map_err(|error| CliDispatchError::ToolRegistry(error.to_string()))?;
    let plugins = plugin_manifests();
    let plugin_commands = plugin_command_manifests();
    let plugin_tools = plugin_tool_manifests();

    let mut config = RuntimeConfig::default();
    if let Some(profile) = parsed.profile_override.clone() {
        config.profile = profile;
    }
    if let Some(permission_mode) = parsed.permission_override.clone() {
        config.permission_mode = permission_mode;
    }

    let prompt = parsed.prompt.clone();
    let merged_prompt = merge_prompt_and_stdin(prompt.as_deref(), stdin_text.as_deref());
    let slash_command = resolve_slash_command(&registry, merged_prompt.as_deref())?;

    let runtime = RuntimeBootstrap::new();
    let command_names = registry
        .commands()
        .map(|entry| entry.canonical_name.clone())
        .collect::<Vec<_>>();
    let tool_names = built_in_tools
        .manifests()
        .into_iter()
        .map(|manifest| manifest.name)
        .chain(plugin_tools.iter().map(|manifest| manifest.name.clone()))
        .collect::<Vec<_>>();
    let plugin_names = plugins
        .iter()
        .map(|plugin| plugin.name.clone())
        .collect::<Vec<_>>();

    let help = match &parsed.mode {
        CliMode::Help { topic } => Some(build_help(topic.as_deref(), &registry, &plugins)),
        _ => match slash_command.as_ref() {
            Some(command) if command.canonical_name == "help" => {
                let topic = command.arguments.first().map(|argument| argument.value.as_str());
                Some(build_help(topic, &registry, &plugins))
            }
            _ => None,
        },
    };

    let resume = match &parsed.mode {
        CliMode::Resume { session_id, note } => Some(resume_result(session_id, note.as_deref())),
        _ => match slash_command.as_ref() {
            Some(command) if command.canonical_name == "resume" => {
                let session_id = command
                    .arguments
                    .first()
                    .map(|argument| argument.value.as_str())
                    .unwrap_or_default();
                let note = command.arguments.get(1).map(|argument| argument.value.as_str());
                Some(resume_result(session_id, note))
            }
            _ => None,
        },
    };

    let mode = match &parsed.mode {
        CliMode::Help { .. } => "help",
        CliMode::ListCommands => "commands",
        CliMode::ListPlugins => "plugins",
        CliMode::ListTools => "tools",
        CliMode::PrintConfig => "config",
        CliMode::Resume { .. } => "resume",
        CliMode::Auto => match slash_command.as_ref() {
            Some(command) if command.canonical_name == "help" => "help",
            Some(command) if command.canonical_name == "plugins" => "plugins",
            Some(command) if command.canonical_name == "resume" => "resume",
            Some(_) => "slash_command",
            None if merged_prompt.is_some() => "prompt",
            None => "help",
        },
    }
    .to_string();

    let help = if matches!(parsed.mode, CliMode::Auto) && merged_prompt.is_none() && help.is_none() {
        Some(build_help(None, &registry, &plugins))
    } else {
        help
    };

    Ok(CliOutcome {
        mode,
        output_format: parsed.output_format,
        config,
        runtime: RuntimeSummary {
            canonical_runtime: runtime.canonical_runtime,
            surfaces: runtime
                .surfaces
                .into_iter()
                .map(|surface| format!("{}: {}", surface.name, surface.role))
                .collect(),
        },
        input: InputSummary {
            prompt,
            stdin: stdin_text,
            merged_prompt,
        },
        commands: CommandSummary {
            built_in_count: registry.built_in_commands().len(),
            plugin_count: registry.plugin_commands().len(),
            names: sorted(command_names),
        },
        plugins: PluginSummary {
            count: plugins.len(),
            names: sorted(plugin_names),
        },
        tools: ToolSummary {
            built_in_count: built_in_tools.manifests().len(),
            plugin_count: plugin_tools.len(),
            names: sorted(tool_names),
        },
        help,
        resume,
        slash_command,
    })
}

fn resolve_slash_command(
    registry: &CommandRegistry,
    merged_prompt: Option<&str>,
) -> Result<Option<ResolvedSlashCommand>, CliDispatchError> {
    let Some(prompt) = merged_prompt else {
        return Ok(None);
    };

    match registry.parse(prompt) {
        Ok(RegistryParseOutcome::Matched(command)) => Ok(Some(command)),
        Ok(_) => Ok(None),
        Err(error) => Err(CliDispatchError::SlashCommand(error.to_string())),
    }
}

fn resume_result(session_id: &str, note: Option<&str>) -> SessionControlResult {
    let message = match note {
        Some(note) if !note.trim().is_empty() => format!("resume queued: {note}"),
        _ => "resume queued".to_string(),
    };

    SessionControlResult {
        session_id: session_id.to_string(),
        accepted: true,
        message,
    }
}

fn build_help(
    topic: Option<&str>,
    registry: &CommandRegistry,
    plugins: &[PluginManifest],
) -> HelpSummary {
    match topic.map(normalize_help_topic) {
        None => HelpSummary {
            topic: "general".to_string(),
            title: "Rusty Claude CLI".to_string(),
            lines: vec![
                "Usage: rusty-claude-cli [flags] [prompt]".to_string(),
                "Local topics: help, commands, plugins, tools, resume, formats".to_string(),
                "Slash commands are resolved through the runtime command registry.".to_string(),
            ],
        },
        Some(topic) if topic == "commands" => HelpSummary {
            topic: topic.to_string(),
            title: "Slash Commands".to_string(),
            lines: registry
                .commands()
                .map(|entry| format!("/{} - {}", entry.canonical_name, entry.metadata.summary))
                .collect(),
        },
        Some(topic) if topic == "plugins" => HelpSummary {
            topic: topic.to_string(),
            title: "Bundled Plugins".to_string(),
            lines: plugins
                .iter()
                .map(|plugin| format!("{} - {}", plugin.name, plugin.summary))
                .collect(),
        },
        Some(topic) if topic == "tools" => HelpSummary {
            topic: topic.to_string(),
            title: "Output Formats".to_string(),
            lines: vec![
                "--human renders a readable multi-line summary".to_string(),
                "--json renders the structured execution envelope".to_string(),
                "--compact renders a single-line operator summary".to_string(),
            ],
        },
        Some(topic) if topic == "resume" => HelpSummary {
            topic: topic.to_string(),
            title: "Resume Flows".to_string(),
            lines: vec![
                "Use `resume <session>` or `/resume <session>`.".to_string(),
                "An optional note can be passed after the session identifier.".to_string(),
            ],
        },
        Some(topic) if topic == "formats" => HelpSummary {
            topic: topic.to_string(),
            title: "Output Formats".to_string(),
            lines: vec![
                "human: readable sections for local terminal use".to_string(),
                "json: stable machine-readable envelope".to_string(),
                "compact: terse one-line status summary".to_string(),
            ],
        },
        Some(topic) => {
            if let Some(command) = registry.resolve(&topic) {
                let mut lines = vec![command.metadata.summary.clone()];
                if !command.aliases.is_empty() {
                    lines.push(format!("aliases: {}", command.aliases.join(", ")));
                }
                if !command.metadata.argument_hints.is_empty() {
                    lines.push("arguments:".to_string());
                    lines.extend(command.metadata.argument_hints.iter().map(|hint| {
                        let qualifier = if hint.required { "required" } else { "optional" };
                        format!("{} ({qualifier}) - {}", hint.name, hint.summary)
                    }));
                }
                HelpSummary {
                    topic,
                    title: format!("Command /{}", command.canonical_name),
                    lines,
                }
            } else {
                HelpSummary {
                    topic: topic.to_string(),
                    title: "Unknown Help Topic".to_string(),
                    lines: vec!["No local help topic matched the request.".to_string()],
                }
            }
        }
    }
}

fn normalize_help_topic(topic: &str) -> String {
    topic.trim_start_matches('/').to_string()
}

fn sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{CliMode, ParsedCli};

    #[test]
    fn dispatches_print_config_with_runtime_defaults() {
        let mut parsed = ParsedCli::default();
        parsed.mode = CliMode::PrintConfig;
        let outcome = dispatch_cli(parsed, None).expect("dispatch");
        assert_eq!(outcome.mode, "config");
        assert_eq!(outcome.config.profile, RuntimeProfile::Host);
    }

    #[test]
    fn dispatches_slash_help_locally() {
        let outcome = dispatch_cli(ParsedCli::default(), Some("/help resume".to_string()))
            .expect("dispatch");
        assert_eq!(outcome.mode, "help");
        assert_eq!(
            outcome.help.as_ref().expect("help").topic,
            "resume".to_string()
        );
    }
}
