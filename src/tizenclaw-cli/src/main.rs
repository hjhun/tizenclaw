//! tizenclaw-cli: CLI tool for interacting with TizenClaw daemon.
//!
//! Usage:
//!   tizenclaw-cli "What is the battery level?"
//!   tizenclaw-cli -s my_session "Run a skill"
//!   tizenclaw-cli --stream "Tell me about Tizen"
//!   tizenclaw-cli   (interactive mode)

use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

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
        let addr_len = (std::mem::size_of::<libc::sa_family_t>() + 1 + name.len()) as libc::socklen_t;

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
            if n <= 0 { return false; }
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
        let n = unsafe {
            libc::read(fd, buf.as_mut_ptr().add(got) as *mut _, resp_len - got)
        };
        if n <= 0 { break; }
        got += n as usize;
    }
    String::from_utf8_lossy(&buf[..got]).to_string()
}

/// Send a JSON-RPC 2.0 request and return the response.
fn send_jsonrpc(method: &str, params: Value) -> Result<Value, String> {
    let fd = connect_daemon()?;
    let req = json!({
        "jsonrpc": "2.0",
        "method": method,
        "id": 1,
        "params": params
    });

    if !send_payload(fd, &req.to_string()) {
        unsafe { libc::close(fd); }
        return Err("Failed to send request".into());
    }

    let resp = recv_response(fd);
    unsafe { libc::close(fd); }

    if resp.is_empty() {
        return Err("Empty response from daemon".into());
    }

    serde_json::from_str(&resp).map_err(|e| format!("Invalid JSON: {}", e))
}

/// Send a prompt and print the response.
fn send_prompt(session_id: &str, prompt: &str, stream: bool) -> Result<(), String> {
    let resp = send_jsonrpc("prompt", json!({
        "session_id": session_id,
        "text": prompt,
        "stream": stream
    }))?;

    if let Some(result) = resp.get("result") {
        if let Some(text) = result.get("text").and_then(|v| v.as_str()) {
            println!("{}", text);
        } else {
            println!("{}", serde_json::to_string_pretty(&result).unwrap_or_default());
        }
    } else if let Some(err) = resp.get("error") {
        let msg = err.get("message").and_then(|v| v.as_str()).unwrap_or("Unknown error");
        eprintln!("Error: {}", msg);
    }
    Ok(())
}

/// Interactive REPL mode.
fn interactive_mode(session_id: &str, stream: bool) {
    println!("TizenClaw Interactive CLI (session: {})", session_id);
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
        if line.is_empty() { continue; }

        match line {
            "quit" | "exit" => break,
            "/help" => {
                println!("Commands:");
                println!("  /usage          Show token usage");
                println!("  /session <id>   Switch session");
                println!("  quit, exit      Exit");
                println!("  <text>          Send prompt");
            }
            cmd if cmd.starts_with("/usage") => {
                match send_jsonrpc("get_usage", json!({})) {
                    Ok(resp) => println!("{}", serde_json::to_string_pretty(&resp).unwrap_or_default()),
                    Err(e) => eprintln!("Error: {}", e),
                }
            }
            prompt => {
                if let Err(e) = send_prompt(session_id, prompt, stream) {
                    eprintln!("Error: {}", e);
                }
            }
        }
    }
}

fn print_usage() {
    eprintln!("tizenclaw-cli — TizenClaw IPC client\n");
    eprintln!("Usage:");
    eprintln!("  tizenclaw-cli [options] [prompt]\n");
    eprintln!("Options:");
    eprintln!("  -s <id>       Session ID (default: cli_test)");
    eprintln!("  --stream      Enable streaming");
    eprintln!("  --usage       Show token usage");
    eprintln!("  -h, --help    Show this help\n");
    eprintln!("If no prompt given, starts interactive mode.");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut session_id = "cli_test".to_string();
    let mut stream = false;
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
                session_id = args[i].clone();
            }
            "--stream" => stream = true,
            "--usage" => {
                match send_jsonrpc("get_usage", json!({})) {
                    Ok(resp) => println!("{}", serde_json::to_string_pretty(&resp).unwrap_or_default()),
                    Err(e) => eprintln!("Error: {}", e),
                }
                return;
            }
            _ => {
                for j in i..args.len() {
                    prompt_parts.push(args[j].clone());
                }
                break;
            }
        }
        i += 1;
    }

    let prompt = prompt_parts.join(" ");

    if !prompt.is_empty() {
        // Single-shot mode
        if let Err(e) = send_prompt(&session_id, &prompt, stream) {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    } else {
        // Interactive mode
        interactive_mode(&session_id, stream);
    }
}
