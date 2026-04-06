//! tizenclaw-cli: CLI tool for interacting with TizenClaw daemon.
//!
//! Usage:
//!   tizenclaw-cli "What is the battery level?"
//!   tizenclaw-cli -s my_session "Run a skill"
//!   tizenclaw-cli --stream "Tell me about Tizen"
//!   tizenclaw-cli dashboard start
//!   tizenclaw-cli dashboard start --port 8080
//!   tizenclaw-cli dashboard stop
//!   tizenclaw-cli dashboard status
//!   tizenclaw-cli   (interactive mode)

use serde_json::Value;
use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use tizenclaw::api::TizenClaw;

static CLI_SESSION_COUNTER: AtomicUsize = AtomicUsize::new(1);

fn create_client() -> Result<TizenClaw, String> {
    let mut client = TizenClaw::new();
    client.initialize()?;
    Ok(client)
}

fn print_json(value: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
}

fn print_error_and_exit(error: &str) -> ! {
    eprintln!("Error: {}", error);
    std::process::exit(1);
}

fn generate_session_id() -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let seq = CLI_SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("cli_{}_{}", ts, seq)
}

fn parse_usage_baseline(raw: &str) -> Result<Value, String> {
    serde_json::from_str(raw).map_err(|err| format!("Invalid usage baseline JSON: {}", err))
}

fn show_usage(client: &TizenClaw, session_id: Option<&str>, baseline: Option<&Value>) {
    match client.get_usage(session_id, baseline) {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn send_prompt(
    client: &TizenClaw,
    session_id: &str,
    prompt: &str,
    stream: bool,
) -> Result<String, String> {
    let response = if stream {
        client.process_prompt_streaming(session_id, prompt, |chunk| {
            print!("{}", chunk);
            io::stdout().flush().ok();
        })?
    } else {
        let text = client.process_prompt(session_id, prompt)?;
        tizenclaw::api::PromptResponse {
            session_id: session_id.to_string(),
            text,
            stream_received: false,
        }
    };

    if !response.stream_received {
        println!("{}", response.text);
    } else {
        println!();
    }

    Ok(response.session_id)
}

fn parse_dashboard_command(input: &str) -> (String, Option<u16>) {
    let mut parts = input.split_whitespace();
    let action = parts.next().unwrap_or("").to_string();
    let mut port = None;

    while let Some(part) = parts.next() {
        if part == "--port" {
            let value = parts.next().unwrap_or("");
            match value.parse::<u16>() {
                Ok(parsed) if parsed > 0 => port = Some(parsed),
                _ => {
                    eprintln!("Error: invalid dashboard port '{}'", value);
                    std::process::exit(1);
                }
            }
        } else {
            eprintln!(
                "Unknown dashboard option '{}'. Use: start [--port N] | stop | status",
                part
            );
            std::process::exit(1);
        }
    }

    (action, port)
}

/// Handle `tizenclaw-cli dashboard <action> [--port N]`.
fn cmd_dashboard(client: &TizenClaw, command: &str) {
    let (action, port) = parse_dashboard_command(command);

    match action.as_str() {
        "start" => match client.start_dashboard(port) {
            Ok(_) => {
                if let Some(port) = port {
                    println!("Dashboard started on port {}.", port);
                } else {
                    println!("Dashboard started.");
                }
            }
            Err(error) => print_error_and_exit(&error),
        },
        "stop" => match client.stop_dashboard() {
            Ok(_) => println!("Dashboard stopped."),
            Err(error) => print_error_and_exit(&error),
        },
        "status" => match client.dashboard_status() {
            Ok(result) => {
                let running = result
                    .get("running")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                println!("Dashboard: {}", if running { "running" } else { "stopped" });
            }
            Err(error) => print_error_and_exit(&error),
        },
        _ => {
            eprintln!(
                "Unknown dashboard action '{}'. Use: start [--port N] | stop | status",
                action
            );
            std::process::exit(1);
        }
    }
}

/// Interactive REPL mode.
fn interactive_mode(client: &TizenClaw, explicit_session_id: Option<&str>, stream: bool) {
    match explicit_session_id {
        Some(session_id) => println!("TizenClaw Interactive CLI (session: {})", session_id),
        None => println!("TizenClaw Interactive CLI (new session per prompt)"),
    }
    println!("Type 'quit' or 'exit' to leave. Type '/help' for commands.\n");

    let stdin = io::stdin();
    loop {
        print!("tizenclaw> ");
        io::stdout().flush().ok();

        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match line {
            "quit" | "exit" => break,
            "/help" => {
                println!("Commands:");
                println!("  /usage            Show token usage");
                println!("  /dashboard start [--port N] Start web dashboard");
                println!("  /dashboard stop   Stop web dashboard");
                println!("  /dashboard status Show dashboard status");
                println!("  -s <id>           Re-run CLI with a fixed session");
                println!("  quit, exit        Exit");
                println!("  <text>            Send prompt");
            }
            cmd if cmd.starts_with("/usage") => {
                show_usage(client, explicit_session_id, None);
            }
            cmd if cmd.starts_with("/dashboard ") => {
                let action = cmd.trim_start_matches("/dashboard ").trim();
                cmd_dashboard(client, action);
            }
            prompt => {
                let session_id = explicit_session_id
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(generate_session_id);
                if let Err(error) = send_prompt(client, &session_id, prompt, stream) {
                    eprintln!("Error: {}", error);
                }
            }
        }
    }
}

fn cmd_config_get(client: &TizenClaw, path: Option<&str>) {
    match client.get_llm_config(path) {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_config_set(client: &TizenClaw, path: &str, raw_value: &str, strict_json: bool) {
    let value = if strict_json {
        match serde_json::from_str::<Value>(raw_value) {
            Ok(value) => value,
            Err(err) => {
                eprintln!("Error: invalid JSON value: {}", err);
                std::process::exit(1);
            }
        }
    } else {
        Value::String(raw_value.to_string())
    };

    match client.set_llm_config(path, value) {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_config_unset(client: &TizenClaw, path: &str) {
    match client.unset_llm_config(path) {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_config_reload(client: &TizenClaw) {
    match client.reload_llm_backends() {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_config(client: &TizenClaw, args: &[String]) {
    match args.first().map(String::as_str) {
        Some("get") => {
            cmd_config_get(client, args.get(1).map(String::as_str));
        }
        Some("set") => {
            if args.len() < 3 {
                eprintln!("Usage: tizenclaw-cli config set <path> <value> [--strict-json]");
                std::process::exit(1);
            }
            let strict_json = args[3..]
                .iter()
                .any(|arg| arg == "--strict-json" || arg == "--json");
            cmd_config_set(client, &args[1], &args[2], strict_json);
        }
        Some("unset") => {
            if args.len() < 2 {
                eprintln!("Usage: tizenclaw-cli config unset <path>");
                std::process::exit(1);
            }
            cmd_config_unset(client, &args[1]);
        }
        Some("reload") => {
            cmd_config_reload(client);
        }
        _ => {
            eprintln!("Usage:");
            eprintln!("  tizenclaw-cli config get [path]");
            eprintln!("  tizenclaw-cli config set <path> <value> [--strict-json]");
            eprintln!("  tizenclaw-cli config unset <path>");
            eprintln!("  tizenclaw-cli config reload");
            std::process::exit(1);
        }
    }
}

fn print_usage() {
    eprintln!("tizenclaw-cli — TizenClaw CLI\n");
    eprintln!("Usage:");
    eprintln!("  tizenclaw-cli [options] [prompt]\n");
    eprintln!("Options:");
    eprintln!("  -s <id>           Reuse a fixed session ID");
    eprintln!("  --no-stream       Disable real-time streaming");
    eprintln!("  --usage           Show token usage");
    eprintln!("  --usage-baseline  JSON baseline for usage delta");
    eprintln!("  -h, --help        Show this help\n");
    eprintln!("Dashboard commands:");
    eprintln!("  tizenclaw-cli dashboard start [--port N]");
    eprintln!("                                   Start the web dashboard");
    eprintln!("  tizenclaw-cli dashboard stop    Stop the web dashboard");
    eprintln!("  tizenclaw-cli dashboard status  Show dashboard status\n");
    eprintln!("Registration commands:");
    eprintln!("  tizenclaw-cli register tool <path>");
    eprintln!("  tizenclaw-cli register skill <path>");
    eprintln!("  tizenclaw-cli unregister tool <path>");
    eprintln!("  tizenclaw-cli unregister skill <path>");
    eprintln!("  tizenclaw-cli list registrations\n");
    eprintln!("LLM config commands:");
    eprintln!("  tizenclaw-cli config get [path]");
    eprintln!("  tizenclaw-cli config set <path> <value> [--strict-json]");
    eprintln!("  tizenclaw-cli config unset <path>");
    eprintln!("  tizenclaw-cli config reload\n");
    eprintln!("If no prompt given, starts interactive mode.");
}

fn cmd_register(client: &TizenClaw, kind: &str, path: &str) {
    match client.register_path(kind, path) {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_unregister(client: &TizenClaw, kind: &str, path: &str) {
    match client.unregister_path(kind, path) {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn cmd_list_registrations(client: &TizenClaw) {
    match client.list_registered_paths() {
        Ok(result) => print_json(&result),
        Err(error) => print_error_and_exit(&error),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut session_id: Option<String> = None;
    let mut explicit_session_id = false;
    let mut stream = true;
    let mut usage_requested = false;
    let mut usage_baseline: Option<Value> = None;
    let mut prompt_parts: Vec<String> = vec![];
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_usage();
                return;
            }
            "-s" if i + 1 < args.len() => {
                i += 1;
                session_id = Some(args[i].clone());
                explicit_session_id = true;
            }
            "--no-stream" => stream = false,
            "--usage" => {
                usage_requested = true;
            }
            "--usage-baseline" if i + 1 < args.len() => {
                i += 1;
                usage_baseline = Some(parse_usage_baseline(&args[i]).unwrap_or_else(|err| {
                    eprintln!("{}", err);
                    std::process::exit(1);
                }));
            }
            "--usage-baseline" => {
                eprintln!("Usage: tizenclaw-cli --usage-baseline '<json>'");
                std::process::exit(1);
            }
            "dashboard" if i + 1 < args.len() => {
                let client = create_client().unwrap_or_else(|err| print_error_and_exit(&err));
                i += 1;
                let mut command = args[i].clone();
                i += 1;
                while i < args.len() {
                    command.push(' ');
                    command.push_str(&args[i]);
                    i += 1;
                }
                cmd_dashboard(&client, &command);
                return;
            }
            "register" if i + 2 < args.len() => {
                let client = create_client().unwrap_or_else(|err| print_error_and_exit(&err));
                cmd_register(&client, &args[i + 1], &args[i + 2]);
                return;
            }
            "unregister" if i + 2 < args.len() => {
                let client = create_client().unwrap_or_else(|err| print_error_and_exit(&err));
                cmd_unregister(&client, &args[i + 1], &args[i + 2]);
                return;
            }
            "list" if i + 1 < args.len() && args[i + 1] == "registrations" => {
                let client = create_client().unwrap_or_else(|err| print_error_and_exit(&err));
                cmd_list_registrations(&client);
                return;
            }
            "config" => {
                let client = create_client().unwrap_or_else(|err| print_error_and_exit(&err));
                cmd_config(&client, &args[i + 1..]);
                return;
            }
            "dashboard" => {
                eprintln!("Usage: tizenclaw-cli dashboard <start [--port N]|stop|status>");
                std::process::exit(1);
            }
            "register" => {
                eprintln!("Usage: tizenclaw-cli register <tool|skill> <path>");
                std::process::exit(1);
            }
            "unregister" => {
                eprintln!("Usage: tizenclaw-cli unregister <tool|skill> <path>");
                std::process::exit(1);
            }
            _ => {
                for arg in args.iter().skip(i) {
                    prompt_parts.push(arg.clone());
                }
                break;
            }
        }
        i += 1;
    }

    let client = create_client().unwrap_or_else(|err| print_error_and_exit(&err));

    if usage_requested {
        show_usage(&client, session_id.as_deref(), usage_baseline.as_ref());
        return;
    }

    let prompt = prompt_parts.join(" ");

    if !prompt.is_empty() {
        let resolved_session_id = session_id.unwrap_or_else(generate_session_id);
        if let Err(error) = send_prompt(&client, &resolved_session_id, &prompt, stream) {
            eprintln!("{}", error);
            std::process::exit(1);
        }
    } else {
        let explicit = if explicit_session_id {
            session_id.as_deref()
        } else {
            None
        };
        interactive_mode(&client, explicit, stream);
    }
}
