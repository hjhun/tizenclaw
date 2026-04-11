use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResumeBehavior {
    #[default]
    Unsupported,
    Supported,
    ResumeOnly,
}

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
    pub fn required(name: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            summary: summary.into(),
            required: true,
            repeatable: false,
        }
    }

    pub fn optional(name: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            summary: summary.into(),
            required: false,
            repeatable: false,
        }
    }

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedSlashCommand {
    pub requested_name: String,
    pub canonical_name: String,
    pub source: CommandSource,
    pub summary: String,
    pub resume_behavior: ResumeBehavior,
    pub arguments: Vec<ParsedCommandArgument>,
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
    let first = chars
        .next()
        .ok_or(InputValidationError::EmptyName)?;
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommandRegistry {
    entries: BTreeMap<String, CommandManifestEntry>,
    aliases: BTreeMap<String, String>,
}

impl CommandRegistry {
    pub fn builder() -> CommandRegistryBuilder {
        CommandRegistryBuilder::new()
    }

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

    pub fn commands(&self) -> impl Iterator<Item = &CommandManifestEntry> {
        self.entries.values()
    }

    pub fn built_in_commands(&self) -> Vec<&CommandManifestEntry> {
        self.commands()
            .filter(|entry| matches!(&entry.source, CommandSource::BuiltIn))
            .collect()
    }

    pub fn plugin_commands(&self) -> Vec<&CommandManifestEntry> {
        self.commands()
            .filter(|entry| matches!(&entry.source, CommandSource::Plugin { .. }))
            .collect()
    }

    pub fn get(&self, canonical_name: &str) -> Option<&CommandManifestEntry> {
        self.entries.get(canonical_name)
    }

    pub fn resolve(&self, requested_name: &str) -> Option<&CommandManifestEntry> {
        self.entries.get(requested_name).or_else(|| {
            self.aliases
                .get(requested_name)
                .and_then(|canonical| self.entries.get(canonical))
        })
    }

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

fn bind_arguments(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_registry() -> CommandRegistry {
        CommandRegistry::from_entries(built_in_command_manifests()).expect("valid built-ins")
    }

    #[test]
    fn parses_plain_text_without_treating_it_as_command() {
        let outcome = parse_slash_command("hello world").expect("parse outcome");
        assert_eq!(
            outcome,
            SlashCommandParseOutcome::NotSlashCommand {
                input: "hello world".to_string()
            }
        );
    }

    #[test]
    fn parses_quoted_slash_command_arguments() {
        let outcome = parse_slash_command("/resume abc \"needs review\"").expect("parse outcome");
        assert_eq!(
            outcome,
            SlashCommandParseOutcome::Invocation(RawSlashCommand {
                invoked_name: "resume".to_string(),
                arguments: vec!["abc".to_string(), "needs review".to_string()],
            })
        );
    }

    #[test]
    fn reports_unterminated_quotes() {
        let error = parse_slash_command("/resume \"abc").expect_err("parse should fail");
        assert_eq!(error, SlashCommandParseError::UnterminatedQuote);
    }

    #[test]
    fn resolves_aliases_through_the_registry() {
        let registry = test_registry();
        let outcome = registry.parse("/continue session-42").expect("registry parse");

        assert_eq!(
            outcome,
            RegistryParseOutcome::Matched(ResolvedSlashCommand {
                requested_name: "continue".to_string(),
                canonical_name: "resume".to_string(),
                source: CommandSource::BuiltIn,
                summary: "Resume a recorded session or continuation point".to_string(),
                resume_behavior: ResumeBehavior::ResumeOnly,
                arguments: vec![ParsedCommandArgument {
                    hint_name: Some("session".to_string()),
                    value: "session-42".to_string(),
                }],
            })
        );
    }

    #[test]
    fn reports_missing_required_arguments() {
        let registry = test_registry();
        let error = registry.parse("/resume").expect_err("validation should fail");
        assert_eq!(
            error,
            RegistryParseError::MissingRequiredArgument {
                command: "resume".to_string(),
                argument: "session".to_string(),
            }
        );
    }

    #[test]
    fn rejects_invalid_command_names() {
        let error = validate_command_name("Resume").expect_err("validation should fail");
        assert_eq!(
            error,
            InputValidationError::InvalidLeadingCharacter {
                name: "Resume".to_string(),
            }
        );
    }

    #[test]
    fn rejects_required_arguments_after_optional_ones() {
        let error = validate_argument_hints(&[
            SlashCommandArgHint::optional("first", "optional"),
            SlashCommandArgHint::required("second", "required"),
        ])
        .expect_err("validation should fail");

        assert_eq!(
            error,
            InputValidationError::RequiredAfterOptional {
                argument: "second".to_string(),
            }
        );
    }

    #[test]
    fn separates_plugin_commands_from_built_ins() {
        let plugin_command = CommandManifestEntry::new(
            "metadata.sync",
            CommandSource::Plugin {
                plugin_name: "metadata".to_string(),
            },
            "Synchronize metadata-backed command state",
        );
        let registry = CommandRegistry::from_entries(
            built_in_command_manifests()
                .into_iter()
                .chain(std::iter::once(plugin_command)),
        )
        .expect("registry");

        assert_eq!(registry.built_in_commands().len(), 3);
        assert_eq!(registry.plugin_commands().len(), 1);
        assert_eq!(
            registry.plugin_commands()[0].source.plugin_name(),
            Some("metadata")
        );
    }
}
