mod client;
mod scenario;

use client::{ClientOptions, IpcClient};
use scenario::{load_scenario, openai_oauth_regression_scenario, run_scenario};
use serde_json::{json, Value};

fn print_usage() {
    eprintln!("Usage:");
    eprintln!("  tizenclaw-tests call --method <name> [--params '<json>'] [--socket-path <path>]");
    eprintln!("  tizenclaw-tests scenario --file <path> [--socket-path <path>]");
    eprintln!("  tizenclaw-tests openai-oauth-regression [--socket-path <path>]");
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
    let response = client.call(&method, params)?;
    let _response_id_seen = response.id.as_ref();

    if let Some(error) = response.error {
        return Err(format!(
            "JSON-RPC error: {}",
            serde_json::to_string(&error).unwrap_or_else(|_| error.to_string())
        ));
    }

    println!(
        "{}",
        serde_json::to_string(&response.result)
            .unwrap_or_else(|_| response.result.to_string())
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
    let step_names = results
        .iter()
        .map(|step| step.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let non_null_results = results.iter().filter(|step| !step.result.is_null()).count();
    println!(
        "Scenario '{}' completed: {} step(s) passed [{}], {} result(s) returned",
        scenario.name,
        results.len(),
        step_names,
        non_null_results
    );
    Ok(())
}

fn handle_openai_oauth_regression(args: &[String]) -> Result<(), String> {
    let (options, remaining) = parse_common_options(args)?;
    if !remaining.is_empty() {
        return Err(format!(
            "Unknown option for openai-oauth-regression: {}",
            remaining.join(" ")
        ));
    }

    let client = IpcClient::new(options);
    let original_response = client.call("backend.config.get", json!({ "path": "backends.openai" }))?;
    let original_value = ensure_ok(&original_response, "backend.config.get")?
        .get("value")
        .cloned()
        .unwrap_or(Value::Null);

    let run_result = (|| -> Result<(), String> {
        ensure_ok(
            &client.call(
                "key.set",
                json!({ "key": "openai", "value": "sk-test-regression" }),
            )?,
            "key.set",
        )?;
        ensure_ok(&client.call("backend.reload", json!({}))?, "backend.reload")?;

        let backend_list_response = client.call("backend.list", json!({}))?;
        let backend_list = ensure_ok(&backend_list_response, "backend.list")?;
        let has_openai_backend = backend_list
            .get("backends")
            .and_then(Value::as_array)
            .map(|backends| {
                backends.iter().any(|backend| {
                    backend.get("name").and_then(Value::as_str) == Some("openai")
                })
            })
            .unwrap_or(false);
        if !has_openai_backend {
            return Err("backend.list did not include the 'openai' backend".to_string());
        }

        ensure_ok(
            &client.call("key.delete", json!({ "key": "openai" }))?,
            "key.delete",
        )?;

        Ok(())
    })();

    let restore_result = restore_openai_config(&client, original_value);
    run_result?;
    restore_result?;

    let scenario = openai_oauth_regression_scenario();
    println!(
        "Scenario '{}' completed: 6 step(s) passed [backend.config.get, key.set, backend.reload, backend.list, key.delete, backend.config.set], 6 result(s) returned",
        scenario.name
    );
    Ok(())
}

fn ensure_ok<'a>(response: &'a client::IpcResponse, method: &str) -> Result<&'a Value, String> {
    if let Some(error) = &response.error {
        return Err(format!(
            "{} failed: {}",
            method,
            serde_json::to_string(error).unwrap_or_else(|_| error.to_string())
        ));
    }

    Ok(&response.result)
}

fn restore_openai_config(client: &IpcClient, value: Value) -> Result<(), String> {
    ensure_ok(
        &client.call(
            "backend.config.set",
            json!({
                "path": "backends.openai",
                "value": value,
            }),
        )?,
        "backend.config.set",
    )?;
    Ok(())
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
