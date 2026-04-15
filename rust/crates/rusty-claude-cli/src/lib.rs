#![forbid(unsafe_code)]

pub mod init;
pub mod input;
pub mod render;

use std::io::{self, IsTerminal, Read, Write};

use init::{dispatch_cli, CliDispatchError, CliOutcome};
use input::CliInputError;
use render::render_outcome;

#[derive(Debug)]
pub enum CliRunError {
    Input(CliInputError),
    Dispatch(CliDispatchError),
    Io(io::Error),
}

impl std::fmt::Display for CliRunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Input(error) => write!(f, "{error}"),
            Self::Dispatch(error) => write!(f, "{error}"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for CliRunError {}

impl From<CliInputError> for CliRunError {
    fn from(value: CliInputError) -> Self {
        Self::Input(value)
    }
}

impl From<CliDispatchError> for CliRunError {
    fn from(value: CliDispatchError) -> Self {
        Self::Dispatch(value)
    }
}

impl From<io::Error> for CliRunError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

pub fn run_env() -> i32 {
    let args = std::env::args().collect::<Vec<_>>();
    let stdin = io::stdin();
    let stdout = io::stdout();
    let stderr = io::stderr();
    let stdin_is_terminal = stdin.is_terminal();
    let mut stdin = stdin.lock();
    let mut stdout = stdout.lock();
    let mut stderr = stderr.lock();

    match run_with_stdio(args, &mut stdin, stdin_is_terminal, &mut stdout) {
        Ok(_) => 0,
        Err(error) => {
            let _ = writeln!(stderr, "rusty-claude-cli: {error}");
            2
        }
    }
}

pub fn run_with_stdio<I, R, W>(
    args: I,
    stdin: &mut R,
    stdin_is_terminal: bool,
    stdout: &mut W,
) -> Result<CliOutcome, CliRunError>
where
    I: IntoIterator<Item = String>,
    R: Read,
    W: Write,
{
    let args = args.into_iter().collect::<Vec<_>>();
    let parsed = input::parse_args(&args)?;
    let stdin_text = input::read_piped_stdin(stdin, stdin_is_terminal)?;
    let outcome = dispatch_cli(parsed, stdin_text)?;
    let rendered = render_outcome(&outcome);
    if !rendered.is_empty() {
        stdout.write_all(rendered.as_bytes())?;
        stdout.write_all(b"\n")?;
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::input::OutputFormat;

    fn run(args: &[&str], stdin: &str, stdin_is_terminal: bool) -> CliOutcome {
        let mut input = Cursor::new(stdin.as_bytes().to_vec());
        let mut output = Vec::new();
        run_with_stdio(
            args.iter()
                .map(|arg| (*arg).to_string())
                .collect::<Vec<_>>(),
            &mut input,
            stdin_is_terminal,
            &mut output,
        )
        .expect("cli run should succeed")
    }

    #[test]
    fn config_defaults_follow_runtime_defaults() {
        let outcome = run(&["rusty-claude-cli"], "", true);
        assert_eq!(outcome.config.profile, tclaw_runtime::RuntimeProfile::Host);
        assert_eq!(
            outcome.config.permission_mode,
            tclaw_runtime::PermissionMode::Ask
        );
        assert_eq!(outcome.config.plugin_roots, vec!["plugins".to_string()]);
    }

    #[test]
    fn slash_resume_routes_to_resume_mode() {
        let outcome = run(&["rusty-claude-cli", "/resume", "session-42"], "", true);
        assert_eq!(outcome.mode, "resume");
        let resume = outcome.resume.expect("resume result");
        assert_eq!(resume.session_id, "session-42");
        assert!(resume.accepted);
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
    fn compact_output_contract_is_single_line() {
        let mut input = Cursor::new(Vec::<u8>::new());
        let mut output = Vec::new();
        let outcome = run_with_stdio(
            vec![
                "rusty-claude-cli".to_string(),
                "--compact".to_string(),
                "summarize".to_string(),
                "status".to_string(),
            ],
            &mut input,
            true,
            &mut output,
        )
        .expect("cli run should succeed");
        assert_eq!(outcome.output_format, OutputFormat::Compact);

        let rendered = String::from_utf8(output).expect("utf8");
        assert!(!rendered.trim().is_empty());
        assert_eq!(rendered.trim().lines().count(), 1);
        assert!(rendered.contains("mode=prompt"));
    }
}
