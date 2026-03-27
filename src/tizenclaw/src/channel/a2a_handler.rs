//! A2A handler — Google Agent-to-Agent protocol (JSON-RPC 2.0).
//!
//! Implements:
//! - `tasks/send` — submit a task to the agent
//! - `tasks/get`  — query a task's status
//! - `tasks/cancel` — cancel a running task
//! - Agent card at `/.well-known/agent.json`

use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Mutex;

use super::{Channel, ChannelConfig};

/// A2A task status.
#[derive(Clone, Debug, PartialEq)]
enum TaskStatus {
    Submitted,
    Working,
    Completed,
    Failed,
    Cancelled,
}

impl TaskStatus {
    fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Submitted => "submitted",
            TaskStatus::Working => "working",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
            TaskStatus::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Debug)]
struct A2aTask {
    id: String,
    status: TaskStatus,
    message: Value,
    artifacts: Value,
    session_id: String,
    created_at: String,
    updated_at: String,
}

pub struct A2aHandler {
    name: String,
    agent_name: String,
    agent_description: String,
    agent_url: String,
    bearer_tokens: Vec<String>,
    tasks: Mutex<HashMap<String, A2aTask>>,
    counter: AtomicI32,
    running: bool,
}

impl A2aHandler {
    pub fn new(config: &ChannelConfig) -> Self {
        let agent_name = config.settings.get("agent_name")
            .and_then(|v| v.as_str())
            .unwrap_or("TizenClaw Agent")
            .to_string();
        let agent_url = config.settings.get("agent_url")
            .and_then(|v| v.as_str())
            .unwrap_or("http://localhost:9090")
            .to_string();

        let mut bearer_tokens = Vec::new();
        if let Some(arr) = config.settings.get("bearer_tokens").and_then(|v| v.as_array()) {
            for t in arr {
                if let Some(s) = t.as_str() {
                    bearer_tokens.push(s.to_string());
                }
            }
        }

        A2aHandler {
            name: config.name.clone(),
            agent_name,
            agent_description: "TizenClaw AI Agent System for Tizen devices".into(),
            agent_url,
            bearer_tokens,
            tasks: Mutex::new(HashMap::new()),
            counter: AtomicI32::new(0),
            running: false,
        }
    }

    /// Generate the agent card for `/.well-known/agent.json`.
    pub fn get_agent_card(&self) -> Value {
        json!({
            "name": self.agent_name,
            "description": self.agent_description,
            "url": self.agent_url,
            "version": "1.0.0",
            "protocol": "a2a",
            "protocolVersion": "0.1",
            "capabilities": {
                "streaming": false,
                "pushNotifications": false,
                "stateTransitionHistory": false
            },
            "authentication": {
                "schemes": [{"scheme": "bearer", "description": "Bearer token authentication"}]
            },
            "defaultInputModes": ["text"],
            "defaultOutputModes": ["text"],
            "skills": [
                {"id": "general", "name": "General Assistant",
                 "description": "General-purpose AI assistant for Tizen device management"},
                {"id": "device_control", "name": "Device Controller",
                 "description": "Control and monitor Tizen devices"},
                {"id": "code_execution", "name": "Code Executor",
                 "description": "Execute code in sandboxed containers"}
            ]
        })
    }

    /// Process a JSON-RPC 2.0 request.
    pub fn handle_jsonrpc(&self, request: &Value) -> Value {
        let method = request["method"].as_str().unwrap_or("");
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let params = request.get("params").cloned().unwrap_or_else(|| json!({}));

        log::info!("A2A JSON-RPC: method={}", method);

        match method {
            "tasks/send" => self.jsonrpc_result(self.task_send(&params), &id),
            "tasks/get" => self.jsonrpc_result(self.task_get(&params), &id),
            "tasks/cancel" => self.jsonrpc_result(self.task_cancel(&params), &id),
            _ => self.jsonrpc_error(-32601, &format!("Method not found: {}", method), &id),
        }
    }

    /// Validate bearer token.
    pub fn validate_bearer_token(&self, token: &str) -> bool {
        if self.bearer_tokens.is_empty() {
            return true; // Dev mode: allow all
        }
        self.bearer_tokens.iter().any(|t| t == token)
    }

    fn generate_task_id(&self) -> String {
        let seq = self.counter.fetch_add(1, Ordering::SeqCst);
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        format!("a2a-{:x}-{}", ts, seq)
    }

    fn timestamp_now(&self) -> String {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Simple ISO 8601 approximation
        let s = secs % 60;
        let m = (secs / 60) % 60;
        let h = (secs / 3600) % 24;
        format!("1970-01-01T{:02}:{:02}:{:02}Z", h, m, s)
    }

    fn task_send(&self, params: &Value) -> Value {
        let message = match params.get("message") {
            Some(m) => m.clone(),
            None => return json!({"error": "message is required"}),
        };

        // Extract text from message parts
        let text = if let Some(parts) = message.get("parts").and_then(|v| v.as_array()) {
            parts.iter()
                .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("")
        } else {
            message["text"].as_str().unwrap_or("").to_string()
        };

        if text.is_empty() {
            return json!({"error": "No text content in message"});
        }

        let task_id = self.generate_task_id();
        let now = self.timestamp_now();

        let task = A2aTask {
            id: task_id.clone(),
            status: TaskStatus::Completed,
            message,
            artifacts: json!([{"type": "text", "text": format!("Task {} received: {}", task_id, text)}]),
            session_id: format!("a2a_{}", task_id),
            created_at: now.clone(),
            updated_at: now.clone(),
        };

        if let Ok(mut tasks) = self.tasks.lock() {
            tasks.insert(task_id.clone(), task);
        }

        json!({
            "id": task_id,
            "status": "completed",
            "artifacts": [{"type": "text", "text": format!("Task received: {}", text)}],
            "created_at": now,
            "updated_at": now
        })
    }

    fn task_get(&self, params: &Value) -> Value {
        let task_id = match params["id"].as_str() {
            Some(id) => id,
            None => return json!({"error": "id is required"}),
        };

        let tasks = match self.tasks.lock() {
            Ok(t) => t,
            Err(_) => return json!({"error": "internal error"}),
        };

        match tasks.get(task_id) {
            Some(task) => json!({
                "id": task.id,
                "status": task.status.as_str(),
                "artifacts": task.artifacts,
                "created_at": task.created_at,
                "updated_at": task.updated_at
            }),
            None => json!({"error": format!("Task not found: {}", task_id)}),
        }
    }

    fn task_cancel(&self, params: &Value) -> Value {
        let task_id = match params["id"].as_str() {
            Some(id) => id,
            None => return json!({"error": "id is required"}),
        };

        let mut tasks = match self.tasks.lock() {
            Ok(t) => t,
            Err(_) => return json!({"error": "internal error"}),
        };

        match tasks.get_mut(task_id) {
            Some(task) => {
                if task.status == TaskStatus::Completed || task.status == TaskStatus::Failed || task.status == TaskStatus::Cancelled {
                    return json!({"error": format!("Cannot cancel task in terminal state: {}", task.status.as_str())});
                }
                task.status = TaskStatus::Cancelled;
                task.updated_at = self.timestamp_now();
                json!({
                    "id": task_id,
                    "status": "cancelled",
                    "updated_at": task.updated_at
                })
            }
            None => json!({"error": format!("Task not found: {}", task_id)}),
        }
    }

    fn jsonrpc_result(&self, result: Value, id: &Value) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "result": result})
    }

    fn jsonrpc_error(&self, code: i32, message: &str, id: &Value) -> Value {
        json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
    }
}

impl Channel for A2aHandler {
    fn name(&self) -> &str { &self.name }
    fn start(&mut self) -> bool { self.running = true; log::info!("A2A handler started"); true }
    fn stop(&mut self) { self.running = false; }
    fn is_running(&self) -> bool { self.running }
    fn send_message(&self, _msg: &str) -> Result<(), String> { Ok(()) }
}
