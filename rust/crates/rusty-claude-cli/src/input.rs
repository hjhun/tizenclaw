use std::io::{self, Read};

use serde::Serialize;
use tclaw_runtime::{PermissionMode, RuntimeProfile};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Human,
    Json,
    Compact,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliMode {
    Auto,
    Help { topic: Option<String> },
    ListCommands,
    ListPlugins,
    ListTools,
    PrintConfig,
    Resume {
        session_id: String,
        note: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCli {
    pub output_format: OutputFormat,
    pub mode: CliMode,
    pub prompt: Option<String>,
    pub profile_override: Option<RuntimeProfile>,
    pub permission_override: Option<PermissionMode>,
}

impl Default for ParsedCli {
    fn default() -> Self {
        Self {
            output_format: OutputFormat::Human,
            mode: CliMode::Auto,
            prompt: None,
            profile_override: None,
            permission_override: None,
        }
    }
}

#[derive(Debug)]
pub enum CliInputError {
    MissingValue { flag: String },
    UnknownFlag { flag: String },
    InvalidProfile { value: String },
    InvalidPermissionMode { value: String },
    UnexpectedArgument { value: String },
}

impl std::fmt::Display for CliInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingValue { flag } => write!(f, "missing value for `{flag}`"),
            Self::UnknownFlag { flag } => write!(f, "unknown flag `{flag}`"),
            Self::InvalidProfile { value } => {
                write!(f, "invalid profile `{value}`; expected host, tizen, or test")
            }
            Self::InvalidPermissionMode { value } => write!(
                f,
                "invalid permission mode `{value}`; expected ask, allow-all, deny-all, or repo-policy"
            ),
            Self::UnexpectedArgument { value } => {
                write!(f, "unexpected extra argument `{value}` for the selected mode")
            }
        }
    }
}

impl std::error::Error for CliInputError {}

pub fn parse_args(args: &[String]) -> Result<ParsedCli, CliInputError> {
    let mut parsed = ParsedCli::default();
    let mut items = args.iter().skip(1).peekable();
    let mut prompt_tokens = Vec::new();

    while let Some(item) = items.next() {
        match item.as_str() {
            "--json" => parsed.output_format = OutputFormat::Json,
            "--compact" => parsed.output_format = OutputFormat::Compact,
            "--human" => parsed.output_format = OutputFormat::Human,
            "-h" | "--help" => {
                let topic = optional_non_flag(&mut items);
                parsed.mode = CliMode::Help { topic };
            }
            "--help-topic" => {
                let topic = required_value("--help-topic", &mut items)?;
                parsed.mode = CliMode::Help { topic: Some(topic) };
            }
            "--profile" => {
                let value = required_value("--profile", &mut items)?;
                parsed.profile_override = Some(parse_profile(&value)?);
            }
            "--permission-mode" => {
                let value = required_value("--permission-mode", &mut items)?;
                parsed.permission_override = Some(parse_permission_mode(&value)?);
            }
            "--print-config" => parsed.mode = CliMode::PrintConfig,
            "--list-commands" => parsed.mode = CliMode::ListCommands,
            "--list-plugins" => parsed.mode = CliMode::ListPlugins,
            "--list-tools" => parsed.mode = CliMode::ListTools,
            "--resume" => {
                let session_id = required_value("--resume", &mut items)?;
                let note = remaining_as_text(&mut items);
                parsed.mode = CliMode::Resume { session_id, note };
                break;
            }
            "--" => {
                prompt_tokens.extend(items.cloned());
                break;
            }
            "help" if prompt_tokens.is_empty() && matches!(parsed.mode, CliMode::Auto) => {
                parsed.mode = CliMode::Help {
                    topic: items.next().cloned(),
                };
                if let Some(extra) = items.next() {
                    return Err(CliInputError::UnexpectedArgument {
                        value: extra.clone(),
                    });
                }
                break;
            }
            "commands" if prompt_tokens.is_empty() && matches!(parsed.mode, CliMode::Auto) => {
                parsed.mode = CliMode::ListCommands;
            }
            "plugins" if prompt_tokens.is_empty() && matches!(parsed.mode, CliMode::Auto) => {
                parsed.mode = CliMode::ListPlugins;
            }
            "tools" if prompt_tokens.is_empty() && matches!(parsed.mode, CliMode::Auto) => {
                parsed.mode = CliMode::ListTools;
            }
            "resume" if prompt_tokens.is_empty() && matches!(parsed.mode, CliMode::Auto) => {
                let session_id = required_value("resume", &mut items)?;
                let note = remaining_as_text(&mut items);
                parsed.mode = CliMode::Resume { session_id, note };
                break;
            }
            flag if flag.starts_with('-') => {
                return Err(CliInputError::UnknownFlag {
                    flag: flag.to_string(),
                })
            }
            value => prompt_tokens.push(value.to_string()),
        }
    }

    if !prompt_tokens.is_empty() {
        parsed.prompt = Some(prompt_tokens.join(" "));
    }

    Ok(parsed)
}

pub fn read_piped_stdin<R: Read>(
    reader: &mut R,
    stdin_is_terminal: bool,
) -> io::Result<Option<String>> {
    if stdin_is_terminal {
        return Ok(None);
    }

    let mut buffer = String::new();
    reader.read_to_string(&mut buffer)?;
    let trimmed = buffer.trim();
    if trimmed.is_empty() {
        Ok(None)
    } else {
        Ok(Some(trimmed.to_string()))
    }
}

pub fn merge_prompt_and_stdin(prompt: Option<&str>, stdin: Option<&str>) -> Option<String> {
    match (prompt.map(str::trim).filter(|value| !value.is_empty()), stdin) {
        (Some(prompt), Some(stdin)) => Some(format!("{prompt}\n\n{stdin}")),
        (Some(prompt), None) => Some(prompt.to_string()),
        (None, Some(stdin)) => Some(stdin.to_string()),
        (None, None) => None,
    }
}

fn parse_profile(value: &str) -> Result<RuntimeProfile, CliInputError> {
    match value {
        "host" => Ok(RuntimeProfile::Host),
        "tizen" => Ok(RuntimeProfile::Tizen),
        "test" => Ok(RuntimeProfile::Test),
        _ => Err(CliInputError::InvalidProfile {
            value: value.to_string(),
        }),
    }
}

fn parse_permission_mode(value: &str) -> Result<PermissionMode, CliInputError> {
    match value {
        "ask" => Ok(PermissionMode::Ask),
        "allow-all" => Ok(PermissionMode::AllowAll),
        "deny-all" => Ok(PermissionMode::DenyAll),
        "repo-policy" => Ok(PermissionMode::RepoPolicy),
        _ => Err(CliInputError::InvalidPermissionMode {
            value: value.to_string(),
        }),
    }
}

fn required_value<'a, I>(flag: &str, items: &mut std::iter::Peekable<I>) -> Result<String, CliInputError>
where
    I: Iterator<Item = &'a String>,
{
    items
        .next()
        .cloned()
        .ok_or_else(|| CliInputError::MissingValue {
            flag: flag.to_string(),
        })
}

fn optional_non_flag<'a, I>(items: &mut std::iter::Peekable<I>) -> Option<String>
where
    I: Iterator<Item = &'a String>,
{
    match items.peek() {
        Some(value) if !value.starts_with('-') => items.next().cloned(),
        _ => None,
    }
}

fn remaining_as_text<'a, I>(items: &mut std::iter::Peekable<I>) -> Option<String>
where
    I: Iterator<Item = &'a String>,
{
    let remaining = items.cloned().collect::<Vec<_>>();
    if remaining.is_empty() {
        None
    } else {
        Some(remaining.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flags_and_prompt() {
        let args = vec![
            "rusty-claude-cli".to_string(),
            "--json".to_string(),
            "--profile".to_string(),
            "test".to_string(),
            "--permission-mode".to_string(),
            "allow-all".to_string(),
            "ship".to_string(),
            "it".to_string(),
        ];

        let parsed = parse_args(&args).expect("parse");
        assert_eq!(parsed.output_format, OutputFormat::Json);
        assert_eq!(parsed.profile_override, Some(RuntimeProfile::Test));
        assert_eq!(parsed.permission_override, Some(PermissionMode::AllowAll));
        assert_eq!(parsed.prompt, Some("ship it".to_string()));
    }

    #[test]
    fn merges_prompt_and_piped_stdin() {
        let merged = merge_prompt_and_stdin(Some("draft release note"), Some("stdin details"));
        assert_eq!(
            merged,
            Some("draft release note\n\nstdin details".to_string())
        );
    }

    #[test]
    fn reads_piped_stdin_only_when_not_terminal() {
        let mut cursor = io::Cursor::new(b"hello from stdin".to_vec());
        let text = read_piped_stdin(&mut cursor, false).expect("stdin read");
        assert_eq!(text, Some("hello from stdin".to_string()));

        let mut cursor = io::Cursor::new(b"ignored".to_vec());
        let text = read_piped_stdin(&mut cursor, true).expect("stdin read");
        assert_eq!(text, None);
    }
}
