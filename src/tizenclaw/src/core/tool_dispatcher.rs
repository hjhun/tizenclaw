//! Tool dispatcher — routes tool calls from LLM to executors.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::{Command, Stdio};

/// A registered tool declaration.
#[derive(Clone, Debug)]
pub struct ToolDecl {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub binary_path: String,
    pub timeout_secs: u64,
    pub side_effect: String,
}

/// Executes tools by spawning CLI processes.
pub struct ToolDispatcher {
    tools: HashMap<String, ToolDecl>,
}

impl ToolDispatcher {
    pub fn new() -> Self {
        ToolDispatcher { tools: HashMap::new() }
    }

    /// Register a tool.
    pub fn register(&mut self, decl: ToolDecl) {
        self.tools.insert(decl.name.clone(), decl);
    }

    /// Load tools from all subdirectories under a root directory.
    ///
    /// Scans all immediate child directories of `root` and invokes
    /// `load_tools_from_dir()` on each one.
    pub fn load_tools_from_root(&mut self, root: &str) {
        let entries = match std::fs::read_dir(root) {
            Ok(e) => e,
            Err(e) => {
                log::warn!("Cannot read tools root '{}': {}", root, e);
                return;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let dir_str = path.to_string_lossy().to_string();
                self.load_tools_from_dir(&dir_str);
            }
        }
    }

    /// Load tools from a directory of tool.md files.
    pub fn load_tools_from_dir(&mut self, dir: &str) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Look for tool.md inside the directory
                let tool_md = path.join("tool.md");
                if tool_md.exists() {
                    if let Ok(content) = std::fs::read_to_string(&tool_md) {
                        if let Some(decl) = Self::parse_tool_md(&content, &path) {
                            self.register(decl);
                        }
                    }
                }
            }
        }
    }

    fn parse_tool_md(content: &str, tool_dir: &std::path::Path) -> Option<ToolDecl> {
        // Parse simple YAML-like frontmatter from tool.md
        let lines: Vec<&str> = content.lines().collect();
        let mut name = String::new();
        let mut description = String::new();
        let mut binary = String::new();
        let mut timeout: u64 = 30;

        for line in &lines {
            let line = line.trim();
            if line.starts_with("name:") {
                name = line[5..].trim().trim_matches('"').to_string();
            } else if line.starts_with("description:") {
                description = line[12..].trim().trim_matches('"').to_string();
            } else if line.starts_with("binary:") {
                binary = line[7..].trim().trim_matches('"').to_string();
            } else if line.starts_with("timeout:") {
                timeout = line[8..].trim().parse().unwrap_or(30);
            }
        }

        if name.is_empty() {
            name = tool_dir.file_name()?.to_str()?.to_string();
        }
        if binary.is_empty() {
            // Default: look for a binary with the tool name
            let default_bin = format!("/usr/bin/{}", name);
            if std::path::Path::new(&default_bin).exists() {
                binary = default_bin;
            }
        }

        Some(ToolDecl {
            name,
            description,
            binary_path: binary,
            timeout_secs: timeout,
            parameters: json!({"type": "object", "properties": {"args": {"type": "string"}}}),
            side_effect: "reversible".into(),
        })
    }

    /// Get all tool declarations for LLM function calling.
    pub fn get_tool_declarations(&self) -> Vec<crate::llm::backend::LlmToolDecl> {
        self.tools.values().map(|t| {
            crate::llm::backend::LlmToolDecl {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            }
        }).collect()
    }

    /// Execute a tool call.
    pub fn execute(&self, tool_name: &str, args: &Value) -> Value {
        let decl = match self.tools.get(tool_name) {
            Some(d) => d,
            None => return json!({"error": format!("Unknown tool: {}", tool_name)}),
        };

        if decl.binary_path.is_empty() {
            return json!({"error": format!("No binary path for tool: {}", tool_name)});
        }

        // Build argument list from JSON
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

        log::info!("Executing tool '{}': {} {:?}", tool_name, decl.binary_path, cmd_args);

        match Command::new(&decl.binary_path)
            .args(&cmd_args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => {
                let output = match child.wait_with_output() {
                    Ok(o) => o,
                    Err(e) => return json!({"error": format!("Process wait failed: {}", e)}),
                };
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);

                json!({
                    "exit_code": exit_code,
                    "stdout": stdout,
                    "stderr": stderr,
                    "success": output.status.success()
                })
            }
            Err(e) => json!({"error": format!("Failed to spawn: {}", e)}),
        }
    }
}
