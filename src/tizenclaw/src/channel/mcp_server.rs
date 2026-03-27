//! MCP Server — Anthropic Model Context Protocol (JSON-RPC 2.0 over stdio).
//!
//! Implements the MCP server protocol for exposing TizenClaw tools
//! to external MCP clients (e.g., Claude Desktop, Cursor).
//!
//! Protocol version: 2024-11-05
//! Transport: stdio (line-delimited JSON-RPC)

use serde_json::{json, Value};
use crate::core::skill_manifest::SkillManifest;

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_VERSION: &str = "1.0.0";
const SKILLS_DIR: &str = "/opt/usr/share/tizen-tools/skills";

/// A tool discovered from SKILL.md or built-in.
struct McpToolInfo {
    name: String,
    description: String,
    input_schema: Value,
    is_skill: bool,
}

pub struct McpServer {
    tools: Vec<McpToolInfo>,
}

impl McpServer {
    pub fn new() -> Self {
        let mut server = McpServer { tools: Vec::new() };
        server.discover_tools();
        server
    }

    /// Scan `/opt/usr/share/tizen-tools/skills` for SKILL.md manifests
    /// and register them as MCP tools.
    fn discover_tools(&mut self) {
        self.tools.clear();

        // Scan skill manifests
        if let Ok(entries) = std::fs::read_dir(SKILLS_DIR) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }

                let dir_name = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if dir_name.starts_with('.') || dir_name == "mcp_server" {
                    continue;
                }

                if let Some(manifest) = SkillManifest::load(&path) {
                    log::info!("MCP: Discovered tool: {}", manifest.name);
                    self.tools.push(McpToolInfo {
                        name: manifest.name,
                        description: manifest.description,
                        input_schema: manifest.input_schema,
                        is_skill: true,
                    });
                }
            }
        }

        // Add synthetic tool: ask_tizenclaw
        self.tools.push(McpToolInfo {
            name: "ask_tizenclaw".into(),
            description: "Send a natural language prompt to the TizenClaw AI Agent. \
                         The agent will plan and execute actions using available tools \
                         to fulfill the request.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "The user's request in natural language"
                    }
                },
                "required": ["prompt"]
            }),
            is_skill: false,
        });

        log::info!("MCP: Total tools discovered: {}", self.tools.len());
    }

    /// Run the stdio JSON-RPC 2.0 loop (blocking).
    ///
    /// Reads line-delimited JSON from stdin, writes responses to stdout.
    /// This is intended to be called when the daemon is launched with
    /// `--mcp-stdio` flag.
    pub fn run_stdio<F>(&self, process_prompt: F)
    where
        F: Fn(&str, &str) -> String,  // (session_id, prompt) -> result
    {
        log::info!("MCP Server started (stdio mode)");
        use std::io::BufRead;

        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            if line.is_empty() {
                continue;
            }

            let request: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    log::error!("MCP: JSON parse error: {}", e);
                    let err_resp = json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": {"code": -32700, "message": "Parse error"}
                    });
                    println!("{}", err_resp);
                    continue;
                }
            };

            let response = self.process_request(&request, &process_prompt);
            if !response.is_null() {
                println!("{}", response);
            }
        }

        log::info!("MCP Server stdio loop ended");
    }

    /// Process a single JSON-RPC 2.0 request.
    fn process_request<F>(&self, request: &Value, process_prompt: &F) -> Value
    where
        F: Fn(&str, &str) -> String,
    {
        let method = request["method"].as_str().unwrap_or("");
        let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
        let req_id = request.get("id").cloned().unwrap_or(Value::Null);

        match method {
            "initialize" => {
                let result = self.handle_initialize();
                json!({"jsonrpc": "2.0", "id": req_id, "result": result})
            }
            "notifications/initialized" => {
                // Notification — no response
                Value::Null
            }
            "tools/list" => {
                let result = self.handle_tools_list();
                json!({"jsonrpc": "2.0", "id": req_id, "result": result})
            }
            "tools/call" => {
                let result = self.handle_tools_call(&params, process_prompt);
                json!({"jsonrpc": "2.0", "id": req_id, "result": result})
            }
            _ => {
                json!({
                    "jsonrpc": "2.0",
                    "id": req_id,
                    "error": {"code": -32601, "message": "Method not found"}
                })
            }
        }
    }

    fn handle_initialize(&self) -> Value {
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {"tools": {}},
            "serverInfo": {
                "name": "TizenClaw-MCP-Server",
                "version": SERVER_VERSION
            }
        })
    }

    fn handle_tools_list(&self) -> Value {
        let tools: Vec<Value> = self.tools.iter().map(|t| {
            json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": t.input_schema
            })
        }).collect();

        json!({"tools": tools})
    }

    fn handle_tools_call<F>(&self, params: &Value, process_prompt: &F) -> Value
    where
        F: Fn(&str, &str) -> String,
    {
        let tool_name = params["name"].as_str().unwrap_or("");
        let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));

        let found = self.tools.iter().find(|t| t.name == tool_name);
        let tool = match found {
            Some(t) => t,
            None => {
                return json!({
                    "isError": true,
                    "content": [{"type": "text", "text": format!("Tool not found: {}", tool_name)}]
                });
            }
        };

        log::info!("MCP: Calling tool: {}", tool_name);

        if !tool.is_skill {
            // ask_tizenclaw: route through agentic loop
            let prompt = arguments["prompt"].as_str().unwrap_or("");
            if prompt.is_empty() {
                return json!({
                    "isError": true,
                    "content": [{"type": "text", "text": "Missing 'prompt' argument"}]
                });
            }

            let result = process_prompt("mcp_session", prompt);
            return json!({
                "content": [{"type": "text", "text": result}]
            });
        }

        // Direct skill execution
        let result = process_prompt("mcp_skill", &format!("[SKILL:{}] {}", tool_name, arguments));
        json!({
            "content": [{"type": "text", "text": result}]
        })
    }
}
