mod client;
mod scenario;

use client::{ClientOptions, IpcClient};
use scenario::{load_scenario, run_scenario};
use serde_json::Value;

fn print_usage() {
    eprintln!("Usage:");
    eprintln!("  tizenclaw-tests call --method <name> [--params '<json>'] [--socket-path <path>] [--socket-name <name>]");
    eprintln!("  tizenclaw-tests scenario --file <path> [--socket-path <path>] [--socket-name <name>]");
    eprintln!("  tizenclaw-tests openai-oauth-regression [--socket-path <path>] [--socket-name <name>]");
}

fn parse_json(value: Option<String>) -> Result<Value, String> {
    match value {
        Some(raw) => serde_json::from_str(&raw)
            .map_err(|err| format!("Invalid JSON payload '{}': {}", raw, err)),
        None => Ok(serde_json::json!({})),
    }
}

fn parse_common_options(args: &[String]) -> Result<(ClientOptions, Vec<String>), String> {
    let mut socket_name = None;
    let mut socket_path = None;
    let mut remaining = Vec::new();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--socket-name" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "--socket-name requires a value".to_string())?;
                socket_name = Some(value.clone());
            }
            "--socket-path" => {
                i += 1;
                let value = args
                    .get(i)
                    .ok_or_else(|| "--socket-path requires a value".to_string())?;
                socket_path = Some(value.clone());
            }
            other => remaining.push(other.to_string()),
        }
        i += 1;
    }

    Ok((
        ClientOptions {
            socket_name,
            socket_path,
        },
        remaining,
    ))
}

fn handle_call(args: &[String]) -> Result<(), String> {
    let (options, remaining) = parse_common_options(args)?;
    let mut method = None;
    let mut params = None;
    let mut i = 0usize;
    while i < remaining.len() {
        match remaining[i].as_str() {
            "--method" => {
                i += 1;
                method = remaining.get(i).cloned();
            }
            "--params" => {
                i += 1;
                params = remaining.get(i).cloned();
            }
            other => return Err(format!("Unknown option for call: {}", other)),
        }
        i += 1;
    }

    let method = method.ok_or_else(|| "--method is required".to_string())?;
    let params = parse_json(params)?;
    let client = IpcClient::new(options);
    let result = client.call(&method, params)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "method": method,
            "result": result.result,
            "streamed_chunks": result.streamed_chunks,
        }))
        .unwrap()
    );
    Ok(())
}

fn handle_scenario(args: &[String]) -> Result<(), String> {
    let (options, remaining) = parse_common_options(args)?;
    let mut scenario_file = None;
    let mut i = 0usize;
    while i < remaining.len() {
        match remaining[i].as_str() {
            "--file" => {
                i += 1;
                scenario_file = remaining.get(i).cloned();
            }
            other => return Err(format!("Unknown option for scenario: {}", other)),
        }
        i += 1;
    }

    let scenario_file = scenario_file.ok_or_else(|| "--file is required".to_string())?;
    let scenario = load_scenario(&scenario_file)?;
    let client = IpcClient::new(options);
    let results = run_scenario(&client, &scenario)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "scenario": scenario.name,
            "steps": results.into_iter().map(|step| serde_json::json!({
                "name": step.name,
                "result": step.result,
            })).collect::<Vec<_>>()
        }))
        .unwrap()
    );
    Ok(())
}

fn handle_openai_oauth_regression(args: &[String]) -> Result<(), String> {
    let mut scenario_args = vec![
        "--file".to_string(),
        "tests/system/openai_oauth_regression.json".to_string(),
    ];
    scenario_args.extend(args.iter().cloned());
    handle_scenario(&scenario_args)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        print_usage();
        std::process::exit(1);
    }

    let command = &args[0];
    let result = match command.as_str() {
        "call" => handle_call(&args[1..]),
        "scenario" => handle_scenario(&args[1..]),
        "openai-oauth-regression" => handle_openai_oauth_regression(&args[1..]),
        _ => {
            print_usage();
            Err(format!("Unknown command: {}", command))
        }
    };

    if let Err(err) = result {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
}
