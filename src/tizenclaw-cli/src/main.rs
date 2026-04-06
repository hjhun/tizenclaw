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

use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicUsize, Ordering};

static CLI_SESSION_COUNTER: AtomicUsize = AtomicUsize::new(1);

/// Connect to the daemon's abstract Unix socket.
fn connect_daemon() -> Result<i32, String> {
    unsafe {
        let fd = libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0);
        if fd < 0 {
            return Err("Failed to create socket".into());
        }

        let mut addr: libc::sockaddr_un = std::mem::zeroed();
        addr.sun_family = libc::AF_UNIX as u16;
        let name = b"tizenclaw.sock";
        for (i, b) in name.iter().enumerate() {
            addr.sun_path[1 + i] = *b as libc::c_char;
        }
        let addr_len =
            (std::mem::size_of::<libc::sa_family_t>() + 1 + name.len()) as libc::socklen_t;

        if libc::connect(fd, &addr as *const _ as *const libc::sockaddr, addr_len) < 0 {
            libc::close(fd);
            return Err("Failed to connect to daemon. Is tizenclaw running?".into());
        }
        Ok(fd)
    }
}

/// Send a length-prefixed payload.
fn send_payload(fd: i32, data: &str) -> bool {
    let len_bytes = (data.len() as u32).to_be_bytes();
    unsafe {
        if libc::write(fd, len_bytes.as_ptr() as *const _, 4) != 4 {
            return false;
        }
        let mut sent: usize = 0;
        while sent < data.len() {
            let n = libc::write(fd, data.as_ptr().add(sent) as *const _, data.len() - sent);
            if n <= 0 {
                return false;
            }
            sent += n as usize;
        }
    }
    true
}

/// Receive a length-prefixed response.
fn recv_response(fd: i32) -> String {
    let mut len_buf = [0u8; 4];
    unsafe {
        if libc::recv(fd, len_buf.as_mut_ptr() as *mut _, 4, libc::MSG_WAITALL) != 4 {
            return String::new();
        }
    }
    let resp_len = u32::from_be_bytes(len_buf) as usize;
    if resp_len == 0 || resp_len > 10 * 1024 * 1024 {
        return String::new();
    }

    let mut buf = vec![0u8; resp_len];
    let mut got: usize = 0;
    while got < resp_len {
        let n = unsafe { libc::read(fd, buf.as_mut_ptr().add(got) as *mut _, resp_len - got) };
        if n <= 0 {
            break;
        }
        got += n as usize;
    }
    String::from_utf8_lossy(&buf[..got]).to_string()
}

/// Send a JSON-RPC 2.0 request and return the response.
fn send_jsonrpc(method: &str, params: Value) -> Result<(Value, bool), String> {
    let fd = connect_daemon()?;
    let req = json!({
        "jsonrpc": "2.0",
        "method": method,
        "id": 1,
        "params": params
    });

    if !send_payload(fd, &req.to_string()) {
        unsafe {
            libc::close(fd);
        }
        return Err("Failed to send request".into());
    }

    let mut stream_received = false;

    loop {
        let resp = recv_response(fd);
        if resp.is_empty() {
            unsafe {
                libc::close(fd);
            }
            return Err("Empty response from daemon".into());
        }

        let parsed: Value = match serde_json::from_str(&resp) {
            Ok(v) => v,
            Err(e) => {
                unsafe {
                    libc::close(fd);
                }
                return Err(format!("Invalid JSON: {}", e));
            }
        };

        if parsed.get("method").and_then(|v| v.as_str()) == Some("stream_chunk") {
            stream_received = true;
            if let Some(chunk) = parsed
                .get("params")
                .and_then(|p| p.get("chunk"))
                .and_then(|c| c.as_str())
            {
                print!("{}", chunk);
                std::io::stdout().flush().ok();
            }
            continue;
        }

        unsafe {
            libc::close(fd);
        }
        return Ok((parsed, stream_received));
    }
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

fn show_usage(session_id: Option<&str>, baseline: Option<&Value>) {
    let mut params = json!({});
    if let Some(session_id) = session_id.filter(|value| !value.trim().is_empty()) {
        params["session_id"] = Value::String(session_id.to_string());
    }
    if let Some(baseline) = baseline {
        params["baseline"] = baseline.clone();
    }

    match send_jsonrpc("get_usage", params) {
        Ok((resp, _)) => {
            if let Some(result) = resp.get("result") {
                println!(
                    "{}",
                    serde_json::to_string_pretty(result).unwrap_or_default()
                );
            } else if let Some(err) = resp.get("error") {
                eprintln!(
                    "Error: {}",
                    err.get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                );
            }
        }
        Err(e) => eprintln!("Error: {}", e),
    }
}

/// Send a prompt and print the response.
fn send_prompt(session_id: &str, prompt: &str, stream: bool) -> Result<String, String> {
    let (resp, stream_received) = send_jsonrpc(
        "prompt",
        json!({"session_id": session_id, "text": prompt, "stream": stream}),
    )?;
    let mut resolved_session_id = session_id.to_string();

    if let Some(result) = resp.get("result") {
        if let Some(actual_session_id) = result.get("session_id").and_then(|v| v.as_str()) {
            resolved_session_id = actual_session_id.to_string();
        }
        if let Some(text) = result.get("text").and_then(|v| v.as_str()) {
            if !stream_received {
                println!("{}", text);
            } else {
                println!();
            }
        } else {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).unwrap_or_default()
            );
        }
    } else if let Some(err) = resp.get("error") {
        let msg = err
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown error");
        eprintln!("Error: {}", msg);
    }
    Ok(resolved_session_id)
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
fn cmd_dashboard(command: &str) {
    let (action, port) = parse_dashboard_command(command);

    match action.as_str() {
        "start" => {
            let mut params = json!({"name": "web_dashboard"});
            if let Some(port) = port {
                params["settings"] = json!({ "port": port });
            }
            match send_jsonrpc("start_channel", params) {
                Ok((resp, _)) => {
                    if resp.get("result").is_some() {
                        if let Some(port) = port {
                            println!("Dashboard started on port {}.", port);
                        } else {
                            println!("Dashboard started.");
                        }
                    } else if let Some(err) = resp.get("error") {
                        eprintln!(
                            "Error: {}",
                            err.get("message")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                        );
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
        }
        "stop" => match send_jsonrpc("stop_channel", json!({"name": "web_dashboard"})) {
            Ok((resp, _)) => {
                if resp.get("result").is_some() {
                    println!("Dashboard stopped.");
                } else if let Some(err) = resp.get("error") {
                    eprintln!(
                        "Error: {}",
                        err.get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                    );
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        },
        "status" => match send_jsonrpc("channel_status", json!({"name": "web_dashboard"})) {
            Ok((resp, _)) => {
                if let Some(result) = resp.get("result") {
                    let running = result
                        .get("running")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    println!("Dashboard: {}", if running { "running" } else { "stopped" });
                } else if let Some(err) = resp.get("error") {
                    eprintln!(
                        "Error: {}",
                        err.get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                    );
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
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
fn interactive_mode(explicit_session_id: Option<&str>, stream: bool) {
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
                show_usage(explicit_session_id, None);
            }
            cmd if cmd.starts_with("/dashboard ") => {
                let action = cmd.trim_start_matches("/dashboard ").trim();
                cmd_dashboard(action);
            }
            prompt => {
                let session_id = explicit_session_id
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(generate_session_id);
                if let Err(e) = send_prompt(&session_id, prompt, stream) {
                    eprintln!("Error: {}", e);
                }
            }
        }
    }
}

fn cmd_config_get(path: Option<&str>) {
    let params = match path {
        Some(path) => json!({ "path": path }),
        None => json!({}),
    };
    match send_jsonrpc("get_llm_config", params) {
        Ok((resp, _)) => {
            if let Some(result) = resp.get("result") {
                println!(
                    "{}",
                    serde_json::to_string_pretty(result).unwrap_or_default()
                );
            } else if let Some(err) = resp.get("error") {
                eprintln!(
                    "Error: {}",
                    err.get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                );
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_config_set(path: &str, raw_value: &str, strict_json: bool) {
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

    match send_jsonrpc("set_llm_config", json!({ "path": path, "value": value })) {
        Ok((resp, _)) => {
            if let Some(result) = resp.get("result") {
                println!(
                    "{}",
                    serde_json::to_string_pretty(result).unwrap_or_default()
                );
            } else if let Some(err) = resp.get("error") {
                eprintln!(
                    "Error: {}",
                    err.get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                );
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_config_unset(path: &str) {
    match send_jsonrpc("unset_llm_config", json!({ "path": path })) {
        Ok((resp, _)) => {
            if let Some(result) = resp.get("result") {
                println!(
                    "{}",
                    serde_json::to_string_pretty(result).unwrap_or_default()
                );
            } else if let Some(err) = resp.get("error") {
                eprintln!(
                    "Error: {}",
                    err.get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                );
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_config_reload() {
    match send_jsonrpc("reload_llm_backends", json!({})) {
        Ok((resp, _)) => {
            if let Some(result) = resp.get("result") {
                println!(
                    "{}",
                    serde_json::to_string_pretty(result).unwrap_or_default()
                );
            } else if let Some(err) = resp.get("error") {
                eprintln!(
                    "Error: {}",
                    err.get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                );
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_config(args: &[String]) {
    match args.first().map(String::as_str) {
        Some("get") => {
            cmd_config_get(args.get(1).map(String::as_str));
        }
        Some("set") => {
            if args.len() < 3 {
                eprintln!("Usage: tizenclaw-cli config set <path> <value> [--strict-json]");
                std::process::exit(1);
            }
            let strict_json = args[3..]
                .iter()
                .any(|arg| arg == "--strict-json" || arg == "--json");
            cmd_config_set(&args[1], &args[2], strict_json);
        }
        Some("unset") => {
            if args.len() < 2 {
                eprintln!("Usage: tizenclaw-cli config unset <path>");
                std::process::exit(1);
            }
            cmd_config_unset(&args[1]);
        }
        Some("reload") => {
            cmd_config_reload();
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
    eprintln!("tizenclaw-cli — TizenClaw IPC client\n");
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

fn cmd_register(kind: &str, path: &str) {
    match send_jsonrpc("register_path", json!({"kind": kind, "path": path})) {
        Ok((resp, _)) => {
            if let Some(result) = resp.get("result") {
                println!(
                    "{}",
                    serde_json::to_string_pretty(result).unwrap_or_default()
                );
            } else if let Some(err) = resp.get("error") {
                eprintln!(
                    "Error: {}",
                    err.get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                );
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_unregister(kind: &str, path: &str) {
    match send_jsonrpc("unregister_path", json!({"kind": kind, "path": path})) {
        Ok((resp, _)) => {
            if let Some(result) = resp.get("result") {
                println!(
                    "{}",
                    serde_json::to_string_pretty(result).unwrap_or_default()
                );
            } else if let Some(err) = resp.get("error") {
                eprintln!(
                    "Error: {}",
                    err.get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                );
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_list_registrations() {
    match send_jsonrpc("list_registered_paths", json!({})) {
        Ok((resp, _)) => {
            if let Some(result) = resp.get("result") {
                println!(
                    "{}",
                    serde_json::to_string_pretty(result).unwrap_or_default()
                );
            } else if let Some(err) = resp.get("error") {
                eprintln!(
                    "Error: {}",
                    err.get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                );
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
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
                i += 1;
                let mut command = args[i].clone();
                i += 1;
                while i < args.len() {
                    command.push(' ');
                    command.push_str(&args[i]);
                    i += 1;
                }
                cmd_dashboard(&command);
                return;
            }
            "register" if i + 2 < args.len() => {
                cmd_register(&args[i + 1], &args[i + 2]);
                return;
            }
            "unregister" if i + 2 < args.len() => {
                cmd_unregister(&args[i + 1], &args[i + 2]);
                return;
            }
            "list" if i + 1 < args.len() && args[i + 1] == "registrations" => {
                cmd_list_registrations();
                return;
            }
            "config" => {
                cmd_config(&args[i + 1..]);
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

    if usage_requested {
        show_usage(session_id.as_deref(), usage_baseline.as_ref());
        return;
    }

    let prompt = prompt_parts.join(" ");

    if !prompt.is_empty() {
        let resolved_session_id = session_id.unwrap_or_else(generate_session_id);
        if let Err(e) = send_prompt(&resolved_session_id, &prompt, stream) {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    } else {
        let explicit = if explicit_session_id {
            session_id.as_deref()
        } else {
            None
        };
        interactive_mode(explicit, stream);
    }
}
