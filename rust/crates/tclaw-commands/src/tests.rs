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
