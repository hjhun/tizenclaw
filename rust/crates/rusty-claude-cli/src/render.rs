use crate::init::CliOutcome;
use crate::input::OutputFormat;

pub fn render_outcome(outcome: &CliOutcome) -> String {
    match outcome.output_format {
        OutputFormat::Human => render_human(outcome),
        OutputFormat::Json => serde_json::to_string_pretty(outcome).unwrap_or_else(|error| {
            format!(
                "{{\"mode\":\"error\",\"message\":\"failed to render json: {}\"}}",
                error
            )
        }),
        OutputFormat::Compact => render_compact(outcome),
    }
}

fn render_human(outcome: &CliOutcome) -> String {
    let mut lines = vec![
        format!("mode: {}", outcome.mode),
        format!("runtime: {}", outcome.runtime.canonical_runtime),
        format!("profile: {:?}", outcome.config.profile).to_lowercase(),
        format!("permission_mode: {:?}", outcome.config.permission_mode).to_lowercase(),
    ];

    if let Some(merged_prompt) = &outcome.input.merged_prompt {
        lines.push(format!("prompt: {}", merged_prompt.replace('\n', " | ")));
    }

    if let Some(help) = &outcome.help {
        lines.push(format!("help: {}", help.title));
        lines.extend(help.lines.iter().cloned());
    }

    if let Some(resume) = &outcome.resume {
        lines.push(format!(
            "resume: {} ({})",
            resume.session_id, resume.message
        ));
    }

    if let Some(command) = &outcome.slash_command {
        lines.push(format!("slash_command: /{}", command.canonical_name));
    }

    lines.push(format!(
        "commands: built_in={}, plugins={}",
        outcome.commands.built_in_count, outcome.commands.plugin_count
    ));
    lines.push(format!(
        "plugins: {} [{}]",
        outcome.plugins.count,
        outcome.plugins.names.join(", ")
    ));
    lines.push(format!(
        "tools: built_in={}, plugins={}",
        outcome.tools.built_in_count, outcome.tools.plugin_count
    ));

    lines.join("\n")
}

fn render_compact(outcome: &CliOutcome) -> String {
    let prompt = outcome
        .input
        .merged_prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.replace('\n', " "))
        .unwrap_or_else(|| "-".to_string());

    format!(
        "mode={} profile={:?} commands={} plugins={} tools={} prompt=\"{}\"",
        outcome.mode,
        outcome.config.profile,
        outcome.commands.names.len(),
        outcome.plugins.count,
        outcome.tools.names.len(),
        prompt
    )
    .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init::{
        CliOutcome, CommandSummary, HelpSummary, InputSummary, PluginSummary, RuntimeSummary,
        ToolSummary,
    };
    use tclaw_runtime::{PermissionMode, RuntimeConfig, RuntimePaths, RuntimeProfile};

    fn sample_outcome(format: OutputFormat) -> CliOutcome {
        CliOutcome {
            mode: "help".to_string(),
            output_format: format,
            config: RuntimeConfig {
                profile: RuntimeProfile::Host,
                paths: RuntimePaths::default(),
                permission_mode: PermissionMode::Ask,
                permission_policy: Default::default(),
                hooks_enabled: true,
                sandbox_enabled: true,
                sandbox_policy: Default::default(),
                plugin_roots: vec!["plugins".to_string()],
                mcp: Default::default(),
            },
            runtime: RuntimeSummary {
                canonical_runtime: "rust".to_string(),
                surfaces: vec!["cli: operator entrypoint".to_string()],
            },
            input: InputSummary {
                prompt: None,
                stdin: None,
                merged_prompt: None,
            },
            commands: CommandSummary {
                built_in_count: 3,
                plugin_count: 1,
                names: vec!["help".to_string(), "resume".to_string()],
            },
            plugins: PluginSummary {
                count: 1,
                names: vec!["metadata".to_string()],
            },
            tools: ToolSummary {
                built_in_count: 4,
                plugin_count: 1,
                names: vec!["fs.read_text".to_string()],
            },
            help: Some(HelpSummary {
                topic: "resume".to_string(),
                title: "Resume".to_string(),
                lines: vec!["Use /resume <session>".to_string()],
            }),
            resume: None,
            slash_command: None,
        }
    }

    #[test]
    fn json_output_contains_structured_contract() {
        let rendered = render_outcome(&sample_outcome(OutputFormat::Json));
        assert!(rendered.contains("\"mode\": \"help\""));
        assert!(rendered.contains("\"canonical_runtime\": \"rust\""));
        assert!(rendered.contains("\"topic\": \"resume\""));
    }

    #[test]
    fn human_output_stays_readable() {
        let rendered = render_outcome(&sample_outcome(OutputFormat::Human));
        assert!(rendered.contains("mode: help"));
        assert!(rendered.contains("help: Resume"));
        assert!(rendered.contains("plugins: 1 [metadata]"));
    }
}
