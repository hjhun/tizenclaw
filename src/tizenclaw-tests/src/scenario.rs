use crate::client::IpcClient;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScenarioFile {
    pub name: String,
    pub steps: Vec<ScenarioStep>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScenarioStep {
    pub name: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub assertions: Vec<ScenarioAssertion>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScenarioAssertion {
    pub path: String,
    #[serde(default)]
    pub exists: bool,
    #[serde(default)]
    pub equals: Option<Value>,
    #[serde(default)]
    pub contains: Option<String>,
    #[serde(default)]
    pub greater_than: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct ScenarioStepResult {
    pub name: String,
    pub result: Value,
}

pub fn load_scenario(path: &str) -> Result<ScenarioFile, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|err| format!("Failed to read scenario '{}': {}", path, err))?;
    serde_json::from_str(&content)
        .map_err(|err| format!("Failed to parse scenario '{}': {}", path, err))
}

pub fn unique_session_id(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{}_{}_{}_{}", prefix, pid, now, n)
}

pub fn openai_oauth_regression_scenario() -> ScenarioFile {
    ScenarioFile {
        name: "openai-oauth-regression".to_string(),
        steps: vec![
            ScenarioStep {
                name: "read-openai-codex-oauth".to_string(),
                method: "get_llm_config".to_string(),
                params: json!({
                    "path": "backends.openai-codex.oauth"
                }),
                assertions: vec![
                    ScenarioAssertion {
                        path: "status".to_string(),
                        exists: false,
                        equals: Some(json!("ok")),
                        contains: None,
                        greater_than: None,
                    },
                    ScenarioAssertion {
                        path: "value.source".to_string(),
                        exists: false,
                        equals: Some(json!("codex_cli")),
                        contains: None,
                        greater_than: None,
                    },
                    ScenarioAssertion {
                        path: "value.auth_path".to_string(),
                        exists: false,
                        equals: None,
                        contains: Some(".codex/auth.json".to_string()),
                        greater_than: None,
                    },
                    ScenarioAssertion {
                        path: "value.account_id".to_string(),
                        exists: true,
                        equals: None,
                        contains: None,
                        greater_than: None,
                    },
                    ScenarioAssertion {
                        path: "value.expires_at".to_string(),
                        exists: false,
                        equals: None,
                        contains: None,
                        greater_than: Some(json!(0)),
                    },
                ],
            },
            ScenarioStep {
                name: "read-llm-runtime-status".to_string(),
                method: "get_llm_runtime".to_string(),
                params: json!({}),
                assertions: vec![
                    ScenarioAssertion {
                        path: "status".to_string(),
                        exists: false,
                        equals: Some(json!("ok")),
                        contains: None,
                        greater_than: None,
                    },
                    ScenarioAssertion {
                        path: "runtime_primary_backend".to_string(),
                        exists: false,
                        equals: Some(json!("openai-codex")),
                        contains: None,
                        greater_than: None,
                    },
                ],
            },
        ],
    }
}

pub fn run_scenario(
    client: &IpcClient,
    scenario: &ScenarioFile,
) -> Result<Vec<ScenarioStepResult>, String> {
    let mut executed = Vec::new();
    let mut placeholders = HashMap::new();

    for step in &scenario.steps {
        let params = materialize_value(&step.params, &mut placeholders);
        let response = client.call(&step.method, params).map_err(|err| {
            let message = format!(
                "Scenario '{}', step '{}' ({}): request failed: {}",
                scenario.name, step.name, step.method, err
            );
            println!("FAIL {} - {}", step.name, err);
            message
        })?;

        if let Some(error) = response.error {
            let details = serde_json::to_string(&error).unwrap_or_else(|_| error.to_string());
            let message = format!(
                "Scenario '{}', step '{}' ({}): JSON-RPC error {}",
                scenario.name, step.name, step.method, details
            );
            println!("FAIL {} - {}", step.name, details);
            return Err(message);
        }

        for assertion in &step.assertions {
            if let Err(err) = assert_result(&response.result, assertion) {
                println!("FAIL {} - {}", step.name, err);
                return Err(format!(
                    "Scenario '{}', step '{}' ({}): {}",
                    scenario.name, step.name, step.method, err
                ));
            }
        }

        println!("PASS {}", step.name);
        executed.push(ScenarioStepResult {
            name: step.name.clone(),
            result: response.result,
        });
    }

    Ok(executed)
}

fn materialize_value(value: &Value, placeholders: &mut HashMap<String, String>) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), materialize_value(value, placeholders)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| materialize_value(item, placeholders))
                .collect(),
        ),
        Value::String(raw) => {
            if let Some(prefix) = raw
                .strip_prefix("${unique_session_id:")
                .and_then(|value| value.strip_suffix('}'))
            {
                let session_id = placeholders
                    .entry(raw.clone())
                    .or_insert_with(|| unique_session_id(prefix))
                    .clone();
                Value::String(session_id)
            } else {
                Value::String(raw.clone())
            }
        }
        _ => value.clone(),
    }
}

fn assert_result(result: &Value, assertion: &ScenarioAssertion) -> Result<(), String> {
    let value = navigate_path(result, &assertion.path);

    if assertion.exists {
        if matches!(value, None | Some(Value::Null)) {
            return Err(format!(
                "assertion failed: path '{}': expected exists=true, got {}",
                assertion.path,
                describe_actual(value)
            ));
        }
    }

    if let Some(expected) = &assertion.equals {
        let actual = value.ok_or_else(|| {
            format!(
                "assertion failed: path '{}': expected equals={}, got null",
                assertion.path, expected
            )
        })?;
        if actual != expected {
            return Err(format!(
                "assertion failed: path '{}': expected equals={}, got {}",
                assertion.path, expected, actual
            ));
        }
    }

    if let Some(expected_substring) = &assertion.contains {
        let actual = value.ok_or_else(|| {
            format!(
                "assertion failed: path '{}': expected contains='{}', got null",
                assertion.path, expected_substring
            )
        })?;
        let actual_string = actual
            .as_str()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| actual.to_string());
        if !actual_string.contains(expected_substring) {
            return Err(format!(
                "assertion failed: path '{}': expected contains='{}', got {}",
                assertion.path,
                expected_substring,
                actual
            ));
        }
    }

    if let Some(expected_min) = &assertion.greater_than {
        let actual = value.ok_or_else(|| {
            format!(
                "assertion failed: path '{}': expected greater_than={}, got null",
                assertion.path, expected_min
            )
        })?;
        let actual_num = actual.as_f64().ok_or_else(|| {
            format!(
                "assertion failed: path '{}': expected greater_than={}, got {}",
                assertion.path, expected_min, actual
            )
        })?;
        let expected_num = expected_min.as_f64().ok_or_else(|| {
            format!(
                "assertion failed: path '{}': invalid greater_than value {}",
                assertion.path, expected_min
            )
        })?;
        if actual_num <= expected_num {
            return Err(format!(
                "assertion failed: path '{}': expected greater_than={}, got {}",
                assertion.path, expected_min, actual
            ));
        }
    }

    Ok(())
}

fn describe_actual(value: Option<&Value>) -> String {
    value
        .map(Value::to_string)
        .unwrap_or_else(|| "null".to_string())
}

pub fn navigate_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    if path.trim().is_empty() {
        return Some(root);
    }

    let mut cursor = root;
    for part in path.split('.').filter(|part| !part.is_empty()) {
        match cursor {
            Value::Object(map) => cursor = map.get(part)?,
            Value::Array(items) => {
                let index = part.parse::<usize>().ok()?;
                cursor = items.get(index)?;
            }
            _ => return None,
        }
    }

    Some(cursor)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::{ClientOptions, IpcClient};
    use std::io::{Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::thread;
    use tempfile::tempdir;

    fn write_frame(stream: &mut UnixStream, payload: &str) {
        let bytes = payload.as_bytes();
        let len = (bytes.len() as u32).to_be_bytes();
        stream.write_all(&len).unwrap();
        stream.write_all(bytes).unwrap();
    }

    fn read_frame(stream: &mut UnixStream) -> String {
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).unwrap();
        let len = u32::from_be_bytes(len_buf) as usize;
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).unwrap();
        String::from_utf8(payload).unwrap()
    }

    fn spawn_mock_server(socket_path: &std::path::Path) -> std::thread::JoinHandle<()> {
        let listener = UnixListener::bind(socket_path).unwrap();
        thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_frame(&mut stream);
                let request_json: Value = serde_json::from_str(&request).unwrap();
                let method = request_json["method"].as_str().unwrap_or_default();
                let response = match method {
                    "process_prompt" => json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "result": {
                            "text": "session ready",
                            "session_id": request_json["params"]["session_id"].clone()
                        }
                    }),
                    "session.status" => json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "result": {
                            "status": "ok",
                            "session_id": request_json["params"]["session_id"].clone(),
                            "session": {
                                "message_file_count": 2,
                                "resume_ready": true
                            }
                        }
                    }),
                    other => json!({
                        "jsonrpc": "2.0",
                        "id": 1,
                        "error": {
                            "message": format!("unexpected method: {}", other)
                        }
                    }),
                };
                write_frame(&mut stream, &response.to_string());
            }
        })
    }

    #[test]
    fn navigate_path_supports_nested_objects_and_arrays() {
        let doc = json!({"a": {"b": [{"c": 7}] }});
        assert_eq!(navigate_path(&doc, "a.b.0.c"), Some(&json!(7)));
    }

    #[test]
    fn run_scenario_executes_steps_against_ipc_server() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("tizenclaw-tests.sock");
        let server = spawn_mock_server(&socket_path);

        let session_id = unique_session_id("system-test");
        let prompt_session_id = session_id.clone();
        let status_session_id = session_id.clone();
        let client = IpcClient::new(ClientOptions {
            socket_path: Some(socket_path.to_string_lossy().to_string()),
            socket_name: None,
        });
        let scenario = ScenarioFile {
            name: "mock".into(),
            steps: vec![
                ScenarioStep {
                    name: "prompt".into(),
                    method: "process_prompt".into(),
                    params: json!({
                        "prompt": "say hello",
                        "session_id": prompt_session_id
                    }),
                    assertions: vec![ScenarioAssertion {
                        path: "text".into(),
                        exists: true,
                        equals: None,
                        contains: Some("ready".into()),
                        greater_than: None,
                    }],
                },
                ScenarioStep {
                    name: "status".into(),
                    method: "session.status".into(),
                    params: json!({
                        "session_id": status_session_id
                    }),
                    assertions: vec![ScenarioAssertion {
                        path: "session.message_file_count".into(),
                        exists: false,
                        equals: None,
                        contains: None,
                        greater_than: Some(json!(0)),
                    }],
                },
            ],
        };

        let results = run_scenario(&client, &scenario).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "prompt");
        assert_eq!(results[1].name, "status");
        server.join().unwrap();
    }

    #[test]
    fn assert_result_supports_greater_than_for_numeric_regressions() {
        let result = json!({
            "oauth": {
                "expires_at": 42
            }
        });
        let assertion = ScenarioAssertion {
            path: "oauth.expires_at".into(),
            exists: false,
            equals: None,
            contains: None,
            greater_than: Some(json!(0)),
        };

        assert!(assert_result(&result, &assertion).is_ok());
    }

    #[test]
    fn assert_result_reports_actual_value_for_missing_path() {
        let result = json!({
            "status": "ok"
        });
        let assertion = ScenarioAssertion {
            path: "tools".into(),
            exists: true,
            equals: None,
            contains: None,
            greater_than: None,
        };

        let err = assert_result(&result, &assertion).unwrap_err();
        assert_eq!(
            err,
            "assertion failed: path 'tools': expected exists=true, got null"
        );
    }

    #[test]
    fn run_scenario_surfaces_step_context_on_request_failure() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("req-fail.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _req = read_frame(&mut stream);
            // Send invalid JSON to trigger client.call error
            write_frame(&mut stream, "not-valid-json");
        });

        let client = IpcClient::new(ClientOptions {
            socket_path: Some(socket_path.to_string_lossy().to_string()),
            socket_name: None,
        });
        let scenario = ScenarioFile {
            name: "req-fail-scenario".into(),
            steps: vec![ScenarioStep {
                name: "failing-step".into(),
                method: "invoke_method".into(),
                params: json!({}),
                assertions: vec![],
            }],
        };

        let err = run_scenario(&client, &scenario).unwrap_err();
        assert!(
            err.contains("req-fail-scenario"),
            "error missing scenario name: {err}"
        );
        assert!(
            err.contains("failing-step"),
            "error missing step name: {err}"
        );
        assert!(
            err.contains("invoke_method"),
            "error missing method name: {err}"
        );
        server.join().unwrap();
    }

    #[test]
    fn run_scenario_surfaces_step_context_on_jsonrpc_error() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("jsonrpc-err.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _req = read_frame(&mut stream);
            write_frame(
                &mut stream,
                &json!({"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Method not found"}}).to_string(),
            );
        });

        let client = IpcClient::new(ClientOptions {
            socket_path: Some(socket_path.to_string_lossy().to_string()),
            socket_name: None,
        });
        let scenario = ScenarioFile {
            name: "rpc-error-scenario".into(),
            steps: vec![ScenarioStep {
                name: "rpc-error-step".into(),
                method: "unknown_method".into(),
                params: json!({}),
                assertions: vec![],
            }],
        };

        let err = run_scenario(&client, &scenario).unwrap_err();
        assert!(
            err.contains("rpc-error-scenario"),
            "error missing scenario name: {err}"
        );
        assert!(
            err.contains("rpc-error-step"),
            "error missing step name: {err}"
        );
        assert!(
            err.contains("unknown_method"),
            "error missing method name: {err}"
        );
        server.join().unwrap();
    }

    #[test]
    fn run_scenario_surfaces_step_context_on_assertion_failure() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("ctx-fail.sock");
        let listener = UnixListener::bind(&socket_path).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _req = read_frame(&mut stream);
            write_frame(
                &mut stream,
                &json!({"jsonrpc":"2.0","id":1,"result":{"status":"ok"}}).to_string(),
            );
        });

        let client = IpcClient::new(ClientOptions {
            socket_path: Some(socket_path.to_string_lossy().to_string()),
            socket_name: None,
        });
        let scenario = ScenarioFile {
            name: "ctx-scenario".into(),
            steps: vec![ScenarioStep {
                name: "check-tools".into(),
                method: "list_tools".into(),
                params: json!({}),
                assertions: vec![ScenarioAssertion {
                    path: "tools".into(),
                    exists: true,
                    equals: None,
                    contains: None,
                    greater_than: None,
                }],
            }],
        };

        let err = run_scenario(&client, &scenario).unwrap_err();
        assert!(
            err.contains("ctx-scenario"),
            "error missing scenario name: {err}"
        );
        assert!(
            err.contains("check-tools"),
            "error missing step name: {err}"
        );
        assert!(
            err.contains("list_tools"),
            "error missing method name: {err}"
        );
        server.join().unwrap();
    }

    #[test]
    fn unique_session_id_generates_unique_values() {
        let first = unique_session_id("scenario");
        let second = unique_session_id("scenario");
        assert_ne!(first, second);
        assert!(first.starts_with("scenario_"));
        assert!(second.starts_with("scenario_"));
    }

    #[test]
    fn materialize_value_reuses_cached_unique_session_placeholders() {
        let mut placeholders = HashMap::new();
        let value = json!({
            "a": "${unique_session_id:test}",
            "b": ["${unique_session_id:test}"]
        });

        let materialized = materialize_value(&value, &mut placeholders);
        let first = materialized["a"].as_str().unwrap();
        let second = materialized["b"][0].as_str().unwrap();
        assert_eq!(first, second);
    }
}
