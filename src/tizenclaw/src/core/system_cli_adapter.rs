//! System CLI adapter — registers system-provided CLI tools as agent tools.

use serde_json::{json, Value};
use std::collections::HashMap;
use crate::llm::backend::LlmToolDecl;

#[derive(Clone, Debug)]
pub struct SystemCliTool {
    pub name: String,
    pub description: String,
    pub binary_path: String,
    pub parameters: Value,
    pub category: String,
}

pub struct SystemCliAdapter {
    tools: HashMap<String, SystemCliTool>,
}

impl SystemCliAdapter {
    pub fn new() -> Self {
        SystemCliAdapter { tools: HashMap::new() }
    }

    pub fn initialize(&mut self, config_path: &str) {
        // Load from JSON config
        if let Ok(content) = std::fs::read_to_string(config_path) {
            if let Ok(config) = serde_json::from_str::<Value>(&content) {
                if let Some(tools) = config["tools"].as_array() {
                    for t in tools {
                        let name = t["name"].as_str().unwrap_or("").to_string();
                        if name.is_empty() { continue; }
                        self.tools.insert(name.clone(), SystemCliTool {
                            name: name.clone(),
                            description: t["description"].as_str().unwrap_or("").to_string(),
                            binary_path: t["binary_path"].as_str().unwrap_or("").to_string(),
                            parameters: t.get("parameters").cloned()
                                .unwrap_or(json!({"type": "object", "properties": {}})),
                            category: t["category"].as_str().unwrap_or("system").to_string(),
                        });
                    }
                }
            }
        }

        // Also scan /usr/bin/tizenclaw-* for auto-discovered tools
        self.scan_auto_tools();

        log::info!("SystemCliAdapter: {} tools registered", self.tools.len());
    }

    fn scan_auto_tools(&mut self) {
        let entries = match std::fs::read_dir("/usr/bin") {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("tizenclaw-") { continue; }
            if self.tools.contains_key(&name) { continue; }

            let tool_name = name.replace("tizenclaw-", "");
            self.tools.insert(tool_name.clone(), SystemCliTool {
                name: tool_name.clone(),
                description: format!("System CLI tool: {}", name),
                binary_path: format!("/usr/bin/{}", name),
                parameters: json!({"type": "object", "properties": {"args": {"type": "string"}}}),
                category: "system".into(),
            });
        }
    }

    pub fn register_tool(&mut self, tool: SystemCliTool) {
        self.tools.insert(tool.name.clone(), tool);
    }

    pub fn unregister_tool(&mut self, name: &str) {
        self.tools.remove(name);
    }

    pub fn get_tool_declarations(&self) -> Vec<LlmToolDecl> {
        self.tools.values().map(|t| LlmToolDecl {
            name: format!("execute_cli_{}", t.name),
            description: t.description.clone(),
            parameters: t.parameters.clone(),
        }).collect()
    }

    pub fn execute(&self, tool_name: &str, args: &Value) -> Value {
        let name = tool_name.strip_prefix("execute_cli_").unwrap_or(tool_name);
        let tool = match self.tools.get(name) {
            Some(t) => t,
            None => return json!({"error": format!("Unknown system CLI tool: {}", name)}),
        };

        let mut cmd_args: Vec<String> = vec![];
        if let Some(args_str) = args.get("args").and_then(|v| v.as_str()) {
            cmd_args = args_str.split_whitespace().map(|s| s.to_string()).collect();
        } else if let Some(obj) = args.as_object() {
            for (k, v) in obj {
                cmd_args.push(format!("--{}", k));
                match v {
                    Value::String(s) => cmd_args.push(s.clone()),
                    other => cmd_args.push(other.to_string()),
                }
            }
        }

        match std::process::Command::new(&tool.binary_path)
            .args(&cmd_args)
            .output()
        {
            Ok(output) => {
                json!({
                    "exit_code": output.status.code().unwrap_or(-1),
                    "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
                    "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
                    "success": output.status.success()
                })
            }
            Err(e) => json!({"error": format!("Failed to execute: {}", e)}),
        }
    }
}
