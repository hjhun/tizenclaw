use crate::client::IpcClient;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct ScenarioFile {
    pub name: String,
    pub steps: Vec<ScenarioStep>,
}

#[derive(Debug, Deserialize)]
pub struct ScenarioStep {
    pub name: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default)]
    pub assertions: Vec<ScenarioAssertion>,
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug)]
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

pub fn run_scenario(
    client: &IpcClient,
    scenario: &ScenarioFile,
) -> Result<Vec<ScenarioStepResult>, String> {
    let mut executed = Vec::new();
    for step in &scenario.steps {
        let response = client.call(&step.method, step.params.clone())?;
        for assertion in &step.assertions {
            assert_result(&response.result, assertion)
                .map_err(|err| format!("Scenario '{}', step '{}': {}", scenario.name, step.name, err))?;
        }

        executed.push(ScenarioStepResult {
            name: step.name.clone(),
            result: response.result,
        });
    }

    Ok(executed)
}

fn assert_result(result: &Value, assertion: &ScenarioAssertion) -> Result<(), String> {
    let value = resolve_path(result, &assertion.path);
    if assertion.exists && value.is_none() {
        return Err(format!("Expected path '{}' to exist", assertion.path));
    }

    if let Some(expected) = &assertion.equals {
        let actual = value.ok_or_else(|| format!("Path '{}' was not found", assertion.path))?;
        if actual != expected {
            return Err(format!(
                "Path '{}' mismatch. expected={}, actual={}",
                assertion.path, expected, actual
            ));
        }
    }

    if let Some(expected_substring) = &assertion.contains {
        let actual = value
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Path '{}' is not a string", assertion.path))?;
        if !actual.contains(expected_substring) {
            return Err(format!(
                "Path '{}' did not contain '{}'",
                assertion.path, expected_substring
            ));
        }
    }

    if let Some(expected_min) = &assertion.greater_than {
        let actual = value.ok_or_else(|| format!("Path '{}' was not found", assertion.path))?;
        let actual_num = actual
            .as_f64()
            .ok_or_else(|| format!("Path '{}' is not numeric", assertion.path))?;
        let expected_num = expected_min
            .as_f64()
            .ok_or_else(|| format!("greater_than for '{}' is not numeric", assertion.path))?;
        if actual_num <= expected_num {
            return Err(format!(
                "Path '{}' was not greater than {} (actual={})",
                assertion.path, expected_min, actual
            ));
        }
    }

    Ok(())
}

pub fn resolve_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    if path.trim().is_empty() {
        return Some(root);
    }

    let mut cursor = root;
    for part in path.split('.').filter(|part| !part.is_empty()) {
        match cursor {
            Value::Object(map) => {
                cursor = map.get(part)?;
            }
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
    use serde_json::json;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::thread;
    use tempfile::tempdir;

    fn write_frame(stream: &mut std::os::unix::net::UnixStream, payload: &str) {
        let bytes = payload.as_bytes();
        let len = (bytes.len() as u32).to_be_bytes();
        stream.write_all(&len).unwrap();
        stream.write_all(bytes).unwrap();
    }

    fn read_frame(stream: &mut std::os::unix::net::UnixStream) -> String {
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
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_frame(&mut stream);
            let request_json: Value = serde_json::from_str(&request).unwrap();
            let method = request_json["method"].as_str().unwrap_or_default();
            let response = match method {
                "get_usage" => json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": {
                        "scope": "session",
                        "usage": {
                            "prompt_tokens": 0
                        }
                    }
                }),
                "list_registered_paths" => json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": {
                        "tool_paths": [],
                        "skill_paths": []
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
        })
    }

    #[test]
    fn resolve_path_supports_nested_objects_and_arrays() {
        let doc = json!({"a": {"b": [{"c": 7}] }});
        assert_eq!(resolve_path(&doc, "a.b.0.c"), Some(&json!(7)));
    }

    #[test]
    fn run_scenario_executes_steps_against_ipc_server() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("tizenclaw-tests.sock");
        let server = spawn_mock_server(&socket_path);

        let client = IpcClient::new(ClientOptions {
            socket_path: Some(socket_path.to_string_lossy().to_string()),
            socket_name: None,
        });
        let scenario = ScenarioFile {
            name: "mock".into(),
            steps: vec![ScenarioStep {
                name: "usage".into(),
                method: "get_usage".into(),
                params: json!({"session_id": "system-test"}),
                assertions: vec![
                    ScenarioAssertion {
                        path: "scope".into(),
                        exists: false,
                        equals: Some(json!("session")),
                        contains: None,
                        greater_than: None,
                    },
                    ScenarioAssertion {
                        path: "usage.prompt_tokens".into(),
                        exists: true,
                        equals: Some(json!(0)),
                        contains: None,
                        greater_than: None,
                    },
                ],
            }],
        };

        let results = run_scenario(&client, &scenario).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "usage");
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
    fn assert_result_rejects_values_not_greater_than_threshold() {
        let result = json!({
            "oauth": {
                "expires_at": 0
            }
        });
        let assertion = ScenarioAssertion {
            path: "oauth.expires_at".into(),
            exists: false,
            equals: None,
            contains: None,
            greater_than: Some(json!(0)),
        };

        assert!(assert_result(&result, &assertion).is_err());
    }
}
