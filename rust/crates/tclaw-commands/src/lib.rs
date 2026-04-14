mod builtins;
mod parser;

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use builtins::built_in_command_manifests;
pub use parser::{
    parse_slash_command, ParsedCommandArgument, RawSlashCommand, SlashCommandParseError,
    SlashCommandParseOutcome,
};

use parser::bind_arguments;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CommandSource {
    BuiltIn,
    Plugin { plugin_name: String },
}

impl CommandSource {
    pub fn plugin_name(&self) -> Option<&str> {
        match self {
            Self::BuiltIn => None,
            Self::Plugin { plugin_name } => Some(plugin_name.as_str()),
        }
    }
}

/// Declares how a slash command participates in session resume flows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResumeBehavior {
    /// The command does not participate in resume flows.
    #[default]
    Unsupported,
    /// The command can run during a resume flow but is not resume-specific.
    Supported,
    /// The command is dedicated to resuming a recorded session.
    ResumeOnly,
}

/// Describes one positional argument accepted by a slash command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlashCommandArgHint {
    pub name: String,
    pub summary: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub repeatable: bool,
}

impl SlashCommandArgHint {
    /// Creates a required positional argument hint.
    pub fn required(name: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            summary: summary.into(),
            required: true,
            repeatable: false,
        }
    }

    /// Creates an optional positional argument hint.
    pub fn optional(name: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            summary: summary.into(),
            required: false,
            repeatable: false,
        }
    }

    /// Creates a repeatable positional argument hint.
    pub fn repeatable(name: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            summary: summary.into(),
            required: false,
            repeatable: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlashCommandMetadata {
    pub summary: String,
    #[serde(default)]
    pub argument_hints: Vec<SlashCommandArgHint>,
    #[serde(default)]
    pub resume_behavior: ResumeBehavior,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandManifestEntry {
    pub canonical_name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub source: CommandSource,
    pub metadata: SlashCommandMetadata,
}

impl CommandManifestEntry {
    pub fn new(
        canonical_name: impl Into<String>,
        source: CommandSource,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            canonical_name: canonical_name.into(),
            aliases: Vec::new(),
            source,
            metadata: SlashCommandMetadata {
                summary: summary.into(),
                argument_hints: Vec::new(),
                resume_behavior: ResumeBehavior::Unsupported,
            },
        }
    }

    pub fn with_aliases<I, S>(mut self, aliases: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.aliases = aliases.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_argument_hints(
        mut self,
        hints: impl IntoIterator<Item = SlashCommandArgHint>,
    ) -> Self {
        self.metadata.argument_hints = hints.into_iter().collect();
        self
    }

    pub fn with_resume_behavior(mut self, resume_behavior: ResumeBehavior) -> Self {
        self.metadata.resume_behavior = resume_behavior;
        self
    }

    pub fn all_names(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.canonical_name.as_str()).chain(self.aliases.iter().map(String::as_str))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InputValidationError {
    #[error("command names must not be empty")]
    EmptyName,
    #[error("command name `{name}` must not start with `/`")]
    UnexpectedSlash { name: String },
    #[error("command name `{name}` contains invalid character `{character}`")]
    InvalidCharacter { name: String, character: char },
    #[error("command name `{name}` must start with a lowercase ascii letter")]
    InvalidLeadingCharacter { name: String },
    #[error("summary must not be empty")]
    EmptySummary,
    #[error("argument hint `{name}` must not be empty")]
    EmptyArgumentHint { name: String },
    #[error("argument hint `{name}` must use a valid identifier")]
    InvalidArgumentHintName { name: String },
    #[error("required argument `{argument}` appears after an optional argument")]
    RequiredAfterOptional { argument: String },
    #[error("repeatable argument `{argument}` must be the final argument hint")]
    RepeatableNotLast { argument: String },
    #[error("duplicate argument hint `{name}`")]
    DuplicateArgumentHint { name: String },
    #[error("alias `{alias}` duplicates the canonical command name")]
    AliasDuplicatesCanonical { alias: String },
    #[error("duplicate alias `{alias}`")]
    DuplicateAlias { alias: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommandRegistryError {
    #[error(transparent)]
    Validation(#[from] InputValidationError),
    #[error("duplicate command name `{name}`")]
    DuplicateCommandName { name: String },
    #[error("command name or alias `{name}` is already registered to `{existing}`")]
    NameConflict { name: String, existing: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RegistryParseOutcome {
    Empty,
    NotSlashCommand { input: String },
    Matched(ResolvedSlashCommand),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RegistryParseError {
    #[error(transparent)]
    Parse(#[from] SlashCommandParseError),
    #[error("unknown command `{name}`")]
    UnknownCommand { name: String },
    #[error("command `{command}` is missing required argument `{argument}`")]
    MissingRequiredArgument { command: String, argument: String },
    #[error(
        "command `{command}` received too many arguments: expected at most {expected}, got {actual}"
    )]
    TooManyArguments {
        command: String,
        expected: usize,
        actual: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedSlashCommand {
    pub requested_name: String,
    pub canonical_name: String,
    pub source: CommandSource,
    pub summary: String,
    pub resume_behavior: ResumeBehavior,
    pub arguments: Vec<ParsedCommandArgument>,
}

/// Validates that a command name matches the canonical slash-command rules.
pub fn validate_command_name(name: &str) -> Result<(), InputValidationError> {
    if name.is_empty() {
        return Err(InputValidationError::EmptyName);
    }
    if name.starts_with('/') {
        return Err(InputValidationError::UnexpectedSlash {
            name: name.to_string(),
        });
    }

    let mut chars = name.chars();
    let first = chars.next().ok_or(InputValidationError::EmptyName)?;
    if !first.is_ascii_lowercase() {
        return Err(InputValidationError::InvalidLeadingCharacter {
            name: name.to_string(),
        });
    }

    for character in std::iter::once(first).chain(chars) {
        if !(character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '-' | '_' | '.'))
        {
            return Err(InputValidationError::InvalidCharacter {
                name: name.to_string(),
                character,
            });
        }
    }

    Ok(())
}

pub fn validate_argument_hints(hints: &[SlashCommandArgHint]) -> Result<(), InputValidationError> {
    let mut seen_optional = false;
    let mut seen_names = BTreeSet::new();

    for (index, hint) in hints.iter().enumerate() {
        if hint.name.trim().is_empty() {
            return Err(InputValidationError::EmptyArgumentHint {
                name: hint.name.clone(),
            });
        }
        validate_command_name(&hint.name).map_err(|_| {
            InputValidationError::InvalidArgumentHintName {
                name: hint.name.clone(),
            }
        })?;
        if hint.summary.trim().is_empty() {
            return Err(InputValidationError::EmptySummary);
        }
        if !seen_names.insert(hint.name.clone()) {
            return Err(InputValidationError::DuplicateArgumentHint {
                name: hint.name.clone(),
            });
        }
        if hint.repeatable && index + 1 != hints.len() {
            return Err(InputValidationError::RepeatableNotLast {
                argument: hint.name.clone(),
            });
        }
        if hint.required {
            if seen_optional {
                return Err(InputValidationError::RequiredAfterOptional {
                    argument: hint.name.clone(),
                });
            }
        } else {
            seen_optional = true;
        }
    }

    Ok(())
}

pub fn validate_manifest_entry(entry: &CommandManifestEntry) -> Result<(), InputValidationError> {
    validate_command_name(&entry.canonical_name)?;
    if entry.metadata.summary.trim().is_empty() {
        return Err(InputValidationError::EmptySummary);
    }

    let mut aliases = BTreeSet::new();
    for alias in &entry.aliases {
        validate_command_name(alias)?;
        if alias == &entry.canonical_name {
            return Err(InputValidationError::AliasDuplicatesCanonical {
                alias: alias.clone(),
            });
        }
        if !aliases.insert(alias.clone()) {
            return Err(InputValidationError::DuplicateAlias {
                alias: alias.clone(),
            });
        }
    }

    validate_argument_hints(&entry.metadata.argument_hints)
}

#[derive(Debug, Clone, Default)]
pub struct CommandRegistryBuilder {
    entries: Vec<CommandManifestEntry>,
}

impl CommandRegistryBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_command(mut self, entry: CommandManifestEntry) -> Self {
        self.entries.push(entry);
        self
    }

    pub fn add_commands(
        mut self,
        entries: impl IntoIterator<Item = CommandManifestEntry>,
    ) -> Self {
        self.entries.extend(entries);
        self
    }

    pub fn build(self) -> Result<CommandRegistry, CommandRegistryError> {
        CommandRegistry::from_entries(self.entries)
    }
}

/// Stores canonical slash commands and their aliases for runtime lookup.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommandRegistry {
    entries: BTreeMap<String, CommandManifestEntry>,
    aliases: BTreeMap<String, String>,
}

impl CommandRegistry {
    /// Creates a builder for assembling a registry incrementally.
    pub fn builder() -> CommandRegistryBuilder {
        CommandRegistryBuilder::new()
    }

    /// Builds a registry from canonical manifest entries and validates conflicts.
    pub fn from_entries(
        entries: impl IntoIterator<Item = CommandManifestEntry>,
    ) -> Result<Self, CommandRegistryError> {
        let mut registry = Self::default();

        for entry in entries {
            validate_manifest_entry(&entry)?;
            let canonical_name = entry.canonical_name.clone();
            if registry.entries.contains_key(&canonical_name) {
                return Err(CommandRegistryError::DuplicateCommandName {
                    name: canonical_name,
                });
            }

            if let Some(existing) = registry.aliases.get(&canonical_name) {
                return Err(CommandRegistryError::NameConflict {
                    name: canonical_name,
                    existing: existing.clone(),
                });
            }

            for alias in &entry.aliases {
                if registry.entries.contains_key(alias) {
                    return Err(CommandRegistryError::NameConflict {
                        name: alias.clone(),
                        existing: alias.clone(),
                    });
                }
                if let Some(existing) = registry.aliases.get(alias) {
                    return Err(CommandRegistryError::NameConflict {
                        name: alias.clone(),
                        existing: existing.clone(),
                    });
                }
            }

            for alias in &entry.aliases {
                registry
                    .aliases
                    .insert(alias.clone(), entry.canonical_name.clone());
            }

            registry.entries.insert(entry.canonical_name.clone(), entry);
        }

        Ok(registry)
    }

    /// Iterates over every registered canonical command manifest.
    pub fn commands(&self) -> impl Iterator<Item = &CommandManifestEntry> {
        self.entries.values()
    }

    /// Returns only built-in command manifests.
    pub fn built_in_commands(&self) -> Vec<&CommandManifestEntry> {
        self.commands()
            .filter(|entry| matches!(&entry.source, CommandSource::BuiltIn))
            .collect()
    }

    /// Returns only plugin-provided command manifests.
    pub fn plugin_commands(&self) -> Vec<&CommandManifestEntry> {
        self.commands()
            .filter(|entry| matches!(&entry.source, CommandSource::Plugin { .. }))
            .collect()
    }

    /// Looks up a canonical command name without consulting aliases.
    pub fn get(&self, canonical_name: &str) -> Option<&CommandManifestEntry> {
        self.entries.get(canonical_name)
    }

    /// Resolves either a canonical command name or one of its aliases.
    pub fn resolve(&self, requested_name: &str) -> Option<&CommandManifestEntry> {
        self.entries.get(requested_name).or_else(|| {
            self.aliases
                .get(requested_name)
                .and_then(|canonical| self.entries.get(canonical))
        })
    }

    /// Parses an input string and resolves it against the registry.
    pub fn parse(&self, input: &str) -> Result<RegistryParseOutcome, RegistryParseError> {
        match parse_slash_command(input)? {
            SlashCommandParseOutcome::Empty => Ok(RegistryParseOutcome::Empty),
            SlashCommandParseOutcome::NotSlashCommand { input } => {
                Ok(RegistryParseOutcome::NotSlashCommand { input })
            }
            SlashCommandParseOutcome::Invocation(raw) => {
                let entry = self
                    .resolve(&raw.invoked_name)
                    .ok_or_else(|| RegistryParseError::UnknownCommand {
                        name: raw.invoked_name.clone(),
                    })?;
                let arguments = bind_arguments(entry, &raw.arguments)?;
                Ok(RegistryParseOutcome::Matched(ResolvedSlashCommand {
                    requested_name: raw.invoked_name,
                    canonical_name: entry.canonical_name.clone(),
                    source: entry.source.clone(),
                    summary: entry.metadata.summary.clone(),
                    resume_behavior: entry.metadata.resume_behavior.clone(),
                    arguments,
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests;
