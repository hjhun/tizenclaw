use std::io::Cursor;

use rusty_claude_cli::run_with_stdio;

fn run_cli(
    args: &[&str],
    stdin: &str,
    stdin_is_terminal: bool,
) -> (rusty_claude_cli::init::CliOutcome, String) {
    let mut input = Cursor::new(stdin.as_bytes().to_vec());
    let mut output = Vec::new();
    let outcome = run_with_stdio(
        args.iter()
            .map(|arg| (*arg).to_string())
            .collect::<Vec<_>>(),
        &mut input,
        stdin_is_terminal,
        &mut output,
    )
    .expect("cli run should succeed");
    (outcome, String::from_utf8(output).expect("utf8"))
}

#[test]
fn json_output_stays_machine_readable() {
    let (_outcome, rendered) = run_cli(
        &["rusty-claude-cli", "--json", "summarize", "status"],
        "",
        true,
    );

    assert!(rendered.contains("\"mode\": \"prompt\""));
    assert!(rendered.contains("\"canonical_runtime\": \"rust\""));
    assert!(rendered.contains("\"plugins\""));
}

#[test]
fn human_output_exposes_operator_facing_sections() {
    let (_outcome, rendered) = run_cli(
        &["rusty-claude-cli", "--human", "summarize", "status"],
        "",
        true,
    );

    assert!(rendered.contains("mode: prompt"));
    assert!(rendered.contains("runtime: rust"));
    assert!(rendered.contains("commands: built_in="));
}
