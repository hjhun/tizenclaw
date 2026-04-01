//! Tool dispatcher — routes tool calls from LLM to executors.

#![allow(clippy::all)]

use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::process::Command;

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

impl Default for ToolDispatcher {
    fn default() -> Self {
        Self::new()
    }
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
        // Parse simple YAML-like frontmatter or markdown headers from tool.md
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
            } else if line.starts_with("# ") && name.is_empty() {
                // Fallback to markdown header logic
                name = line[2..].trim().to_string();
            }
        }
        
        let full_desc = content.trim();
        description = if full_desc.len() > 1536 {
            full_desc[0..1536].to_string()
        } else {
            full_desc.to_string()
        };

        if name.is_empty() {
            name = tool_dir.file_name()?.to_str()?.to_string();
        }

        let original_name = name.clone();

        // Sanitize name for OpenAI function calling rules (^[a-zA-Z0-9_-]+$)
        let clean_name: String = name.chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        name = clean_name.trim_matches('_').to_string();
        
        if name.is_empty() {
            name = "unknown_tool".into();
        }
        if binary.is_empty() {
            // Priority 1: Check if binary exists inside tool's own directory
            let local_bin = tool_dir.join(&original_name);
            if local_bin.exists() {
                binary = local_bin.to_string_lossy().to_string();
            } else {
                // Priority 2: Check Tizen specific CLI path
                let tizen_bin = format!("/opt/usr/share/tizen-tools/cli/{}", original_name);
                if std::path::Path::new(&tizen_bin).exists() {
                    binary = tizen_bin;
                } else {
                    // Priority 3: Check system bin
                    let default_bin = format!("/usr/bin/{}", original_name);
                    if std::path::Path::new(&default_bin).exists() {
                        binary = default_bin;
                    } else {
                        // Fallback to tool dir name
                        let dir_name = tool_dir.file_name().unwrap_or_default().to_string_lossy().to_string();
                        let dir_bin = format!("/usr/bin/{}", dir_name);
                        let tizen_dir_bin = format!("/opt/usr/share/tizen-tools/cli/{}", dir_name);
                        
                        if std::path::Path::new(&tizen_dir_bin).exists() {
                            binary = tizen_dir_bin;
                        } else if std::path::Path::new(&dir_bin).exists() {
                            binary = dir_bin;
                        } else {
                            // Fallback to local path string anyway
                            binary = local_bin.to_string_lossy().to_string();
                        }
                    }
                }
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
    pub async fn execute(&self, tool_name: &str, args: &Value) -> Value {
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
            let mut current = String::new();
            let mut in_quotes = false;
            let mut quote_char = '\0';
            for c in args_str.chars() {
                if in_quotes {
                    if c == quote_char {
                        in_quotes = false;
                    } else {
                        current.push(c);
                    }
                } else {
                    if c == ' ' || c == '\t' || c == '\n' {
                        if !current.is_empty() {
                            cmd_args.push(current.clone());
                            current.clear();
                        }
                    } else if c == '"' || c == '\'' {
                        in_quotes = true;
                        quote_char = c;
                    } else {
                        current.push(c);
                    }
                }
            }
            if !current.is_empty() {
                cmd_args.push(current);
            }
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

        let engine = crate::infra::container_engine::ContainerEngine::new();
        let args_ref: Vec<&str> = cmd_args.iter().map(|s| s.as_str()).collect();

        match engine.execute_oneshot(&decl.binary_path, &args_ref).await {
            Ok(val) => val,
            Err(e) => json!({"error": format!("Failed to execute via IPC: {}", e)}),
        }
    }
}
