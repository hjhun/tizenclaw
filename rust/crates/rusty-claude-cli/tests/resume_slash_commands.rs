use std::io::Cursor;

use rusty_claude_cli::run_with_stdio;

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
fn slash_resume_command_routes_to_resume_contract() {
    let outcome = run_cli(&["rusty-claude-cli", "/resume", "session-42", "continue"]);

    assert_eq!(outcome.mode, "resume");
    assert_eq!(
        outcome.resume.as_ref().expect("resume result").session_id,
        "session-42"
    );
    assert_eq!(
        outcome
            .slash_command
            .as_ref()
            .expect("slash command")
            .canonical_name,
        "resume"
    );
}

#[test]
fn explicit_resume_flag_preserves_operator_note() {
    let outcome = run_cli(&[
        "rusty-claude-cli",
        "--resume",
        "session-9",
        "needs",
        "review",
    ]);

    let resume = outcome.resume.expect("resume");
    assert_eq!(resume.session_id, "session-9");
    assert!(resume.message.contains("needs review"));
}
