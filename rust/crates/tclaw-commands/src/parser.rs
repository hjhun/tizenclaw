use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{CommandManifestEntry, RegistryParseError};

/// Represents the raw slash command name and its tokenized arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawSlashCommand {
    pub invoked_name: String,
    pub arguments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SlashCommandParseOutcome {
    Empty,
    NotSlashCommand { input: String },
    Invocation(RawSlashCommand),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SlashCommandParseError {
    #[error("slash command name is missing")]
    MissingCommandName,
    #[error("unterminated quoted argument")]
    UnterminatedQuote,
    #[error("unexpected escape sequence at end of input")]
    DanglingEscape,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedCommandArgument {
    pub hint_name: Option<String>,
    pub value: String,
}

/// Parses a slash command string into its raw invocation representation.
pub fn parse_slash_command(input: &str) -> Result<SlashCommandParseOutcome, SlashCommandParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(SlashCommandParseOutcome::Empty);
    }
    if !trimmed.starts_with('/') {
        return Ok(SlashCommandParseOutcome::NotSlashCommand {
            input: trimmed.to_string(),
        });
    }

    let tokens = tokenize(trimmed)?;
    let mut parts = tokens.into_iter();
    let command = parts
        .next()
        .ok_or(SlashCommandParseError::MissingCommandName)?;
    let invoked_name = command
        .strip_prefix('/')
        .ok_or(SlashCommandParseError::MissingCommandName)?;

    if invoked_name.is_empty() {
        return Err(SlashCommandParseError::MissingCommandName);
    }

    Ok(SlashCommandParseOutcome::Invocation(RawSlashCommand {
        invoked_name: invoked_name.to_string(),
        arguments: parts.collect(),
    }))
}

pub(crate) fn bind_arguments(
    entry: &CommandManifestEntry,
    raw_arguments: &[String],
) -> Result<Vec<ParsedCommandArgument>, RegistryParseError> {
    let mut bound = Vec::new();
    let mut raw_index = 0usize;

    for hint in &entry.metadata.argument_hints {
        if hint.repeatable {
            let remaining = &raw_arguments[raw_index..];
            if hint.required && remaining.is_empty() {
                return Err(RegistryParseError::MissingRequiredArgument {
                    command: entry.canonical_name.clone(),
                    argument: hint.name.clone(),
                });
            }
            for value in remaining {
                bound.push(ParsedCommandArgument {
                    hint_name: Some(hint.name.clone()),
                    value: value.clone(),
                });
            }
            raw_index = raw_arguments.len();
            continue;
        }

        match raw_arguments.get(raw_index) {
            Some(value) => {
                bound.push(ParsedCommandArgument {
                    hint_name: Some(hint.name.clone()),
                    value: value.clone(),
                });
                raw_index += 1;
            }
            None if hint.required => {
                return Err(RegistryParseError::MissingRequiredArgument {
                    command: entry.canonical_name.clone(),
                    argument: hint.name.clone(),
                });
            }
            None => {}
        }
    }

    if raw_index < raw_arguments.len() {
        return Err(RegistryParseError::TooManyArguments {
            command: entry.canonical_name.clone(),
            expected: entry.metadata.argument_hints.len(),
            actual: raw_arguments.len(),
        });
    }

    Ok(bound)
}

fn tokenize(input: &str) -> Result<Vec<String>, SlashCommandParseError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars();
    let mut quote: Option<char> = None;

    while let Some(character) = chars.next() {
        match (quote, character) {
            (Some(active_quote), ch) if ch == active_quote => {
                quote = None;
            }
            (Some(_), '\\') => {
                let escaped = chars.next().ok_or(SlashCommandParseError::DanglingEscape)?;
                current.push(escaped);
            }
            (Some(_), ch) => current.push(ch),
            (None, '"' | '\'') => {
                quote = Some(character);
            }
            (None, ch) if ch.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            (None, '\\') => {
                let escaped = chars.next().ok_or(SlashCommandParseError::DanglingEscape)?;
                current.push(escaped);
            }
            (None, ch) => current.push(ch),
        }
    }

    if quote.is_some() {
        return Err(SlashCommandParseError::UnterminatedQuote);
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    Ok(tokens)
}
