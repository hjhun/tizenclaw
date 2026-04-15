use std::io::Cursor;

use rusty_claude_cli::input::OutputFormat;
use rusty_claude_cli::run_with_stdio;
use tclaw_runtime::{PermissionMode, RuntimeProfile};

fn run_cli(args: &[&str]) -> rusty_claude_cli::init::CliOutcome {
    let mut input = Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();
    run_with_stdio(
        args.iter()
            .map(|arg| (*arg).to_string())
            .collect::<Vec<_>>(),
        &mut input,
        true,
        &mut output,
    )
    .expect("cli run should succeed")
}

#[test]
fn runtime_defaults_match_the_host_runtime_contract() {
    let outcome = run_cli(&["rusty-claude-cli"]);
    assert_eq!(outcome.output_format, OutputFormat::Human);
    assert_eq!(outcome.config.profile, RuntimeProfile::Host);
    assert_eq!(outcome.config.permission_mode, PermissionMode::Ask);
    assert_eq!(outcome.config.plugin_roots, vec!["plugins".to_string()]);
}

#[test]
fn explicit_flags_override_profile_permission_and_format() {
    let outcome = run_cli(&[
        "rusty-claude-cli",
        "--json",
        "--profile",
        "test",
        "--permission-mode",
        "allow-all",
        "ship",
        "it",
    ]);

    assert_eq!(outcome.output_format, OutputFormat::Json);
    assert_eq!(outcome.config.profile, RuntimeProfile::Test);
    assert_eq!(outcome.config.permission_mode, PermissionMode::AllowAll);
    assert_eq!(outcome.input.merged_prompt.as_deref(), Some("ship it"));
}
