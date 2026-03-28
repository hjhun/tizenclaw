//! Tool declaration builder — generates LLM function declarations for all tools.

use serde_json::{json, Value};
use crate::llm::backend::LlmToolDecl;

pub struct ToolDeclarationBuilder;

impl ToolDeclarationBuilder {
    /// Append all built-in tool declarations.
    pub fn append_builtin_tools(tools: &mut Vec<LlmToolDecl>) {
        // execute_code
        tools.push(LlmToolDecl {
            name: "execute_code".into(),
            description: "Execute arbitrary Python code on the Tizen device. The code MUST print a JSON result to stdout as the last line.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "code": {"type": "string", "description": "Python code to execute on the Tizen device"}
                },
                "required": ["code"]
            }),
        });

        // switch_user
        tools.push(LlmToolDecl {
            name: "switch_user".into(),
            description: "Switch the current active user profile for the session.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "user_id": {"type": "string", "description": "The user_id to switch to"}
                },
                "required": ["user_id"]
            }),
        });

        // create_task
        tools.push(LlmToolDecl {
            name: "create_task".into(),
            description: "Create a scheduled task. Supports: 'daily HH:MM', 'interval Ns/Nm/Nh', 'once YYYY-MM-DD HH:MM', 'weekly DAY HH:MM'.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "schedule": {"type": "string", "description": "Schedule expression"},
                    "prompt": {"type": "string", "description": "The prompt to execute"}
                },
                "required": ["schedule", "prompt"]
            }),
        });

        // list_tasks
        tools.push(LlmToolDecl {
            name: "list_tasks".into(),
            description: "List all scheduled tasks.".into(),
            parameters: json!({"type": "object", "properties": {}, "required": []}),
        });

        // cancel_task
        tools.push(LlmToolDecl {
            name: "cancel_task".into(),
            description: "Cancel a scheduled task by its ID.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "task_id": {"type": "string", "description": "The task ID to cancel"}
                },
                "required": ["task_id"]
            }),
        });

        // create_session
        tools.push(LlmToolDecl {
            name: "create_session".into(),
            description: "Create a new agent session with a custom system prompt.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Short name for the session"},
                    "system_prompt": {"type": "string", "description": "Custom system prompt"}
                },
                "required": ["name", "system_prompt"]
            }),
        });

        // list_sessions
        tools.push(LlmToolDecl {
            name: "list_sessions".into(),
            description: "List all active agent sessions.".into(),
            parameters: json!({"type": "object", "properties": {}, "required": []}),
        });

        // send_to_session
        tools.push(LlmToolDecl {
            name: "send_to_session".into(),
            description: "Send a message to another agent session and receive its response.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "target_session": {"type": "string", "description": "Target session ID"},
                    "message": {"type": "string", "description": "Message to send"}
                },
                "required": ["target_session", "message"]
            }),
        });



        // ingest_document (RAG)
        tools.push(LlmToolDecl {
            name: "ingest_document".into(),
            description: "Ingest a document into the knowledge base for semantic search.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "source": {"type": "string", "description": "Source identifier (filename, URL)"},
                    "text": {"type": "string", "description": "Document text to ingest"}
                },
                "required": ["source", "text"]
            }),
        });

        // search_knowledge (RAG)
        tools.push(LlmToolDecl {
            name: "search_knowledge".into(),
            description: "Search the knowledge base using semantic similarity.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"},
                    "top_k": {"type": "integer", "description": "Number of results (default 5)"}
                },
                "required": ["query"]
            }),
        });

        // lookup_web_api
        tools.push(LlmToolDecl {
            name: "lookup_web_api".into(),
            description: "Look up Tizen Web API reference documentation. Use 'list', 'read', or 'search'.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "operation": {"type": "string", "enum": ["list", "read", "search"]},
                    "path": {"type": "string", "description": "Doc path for 'read'"},
                    "query": {"type": "string", "description": "Keyword for 'search'"}
                },
                "required": ["operation"]
            }),
        });

        // run_supervisor
        tools.push(LlmToolDecl {
            name: "run_supervisor".into(),
            description: "Decompose a complex goal into sub-tasks and delegate to specialized role agents.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "goal": {"type": "string", "description": "High-level goal"},
                    "strategy": {"type": "string", "enum": ["sequential", "parallel"]}
                },
                "required": ["goal"]
            }),
        });

        // list_agent_roles
        tools.push(LlmToolDecl {
            name: "list_agent_roles".into(),
            description: "List all configured agent roles.".into(),
            parameters: json!({"type": "object", "properties": {}, "required": []}),
        });

        // spawn_agent
        tools.push(LlmToolDecl {
            name: "spawn_agent".into(),
            description: "Create a new specialized agent with a custom role definition.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Unique name"},
                    "system_prompt": {"type": "string", "description": "System prompt"},
                    "allowed_tools": {"type": "array", "items": {"type": "string"}},
                    "max_iterations": {"type": "integer"}
                },
                "required": ["name", "system_prompt"]
            }),
        });

        // create_pipeline
        tools.push(LlmToolDecl {
            name: "create_pipeline".into(),
            description: "Create a multi-step pipeline for deterministic workflow execution.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "description": {"type": "string"},
                    "trigger": {"type": "string"},
                    "steps": {"type": "array", "items": {"type": "object"}}
                },
                "required": ["name", "steps"]
            }),
        });

        // list_pipelines / run_pipeline / delete_pipeline
        tools.push(LlmToolDecl {
            name: "list_pipelines".into(),
            description: "List all configured pipelines.".into(),
            parameters: json!({"type": "object", "properties": {}, "required": []}),
        });
        tools.push(LlmToolDecl {
            name: "run_pipeline".into(),
            description: "Execute a pipeline by ID.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "pipeline_id": {"type": "string"},
                    "input_vars": {"type": "object"}
                },
                "required": ["pipeline_id"]
            }),
        });

        // create_workflow / list_workflows / run_workflow
        tools.push(LlmToolDecl {
            name: "create_workflow".into(),
            description: "Create a workflow from Markdown text with YAML frontmatter.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "markdown": {"type": "string", "description": "Markdown with YAML frontmatter"}
                },
                "required": ["markdown"]
            }),
        });
        tools.push(LlmToolDecl {
            name: "list_workflows".into(),
            description: "List all registered workflows.".into(),
            parameters: json!({"type": "object", "properties": {}, "required": []}),
        });
        tools.push(LlmToolDecl {
            name: "run_workflow".into(),
            description: "Execute a workflow by ID.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "workflow_id": {"type": "string"},
                    "input_vars": {"type": "object"}
                },
                "required": ["workflow_id"]
            }),
        });

        // get_agent_status / list_agents
        tools.push(LlmToolDecl {
            name: "get_agent_status".into(),
            description: "Get current agent system status.".into(),
            parameters: json!({"type": "object", "properties": {}, "required": []}),
        });
        tools.push(LlmToolDecl {
            name: "list_agents".into(),
            description: "List all running agents with their status.".into(),
            parameters: json!({"type": "object", "properties": {}, "required": []}),
        });

        // memory tools
        tools.push(LlmToolDecl {
            name: "remember".into(),
            description: "Store a key-value pair in persistent memory.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": {"type": "string"},
                    "value": {"type": "string"},
                    "category": {"type": "string"}
                },
                "required": ["key", "value"]
            }),
        });
        tools.push(LlmToolDecl {
            name: "recall".into(),
            description: "Retrieve a value from persistent memory by key.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": {"type": "string"}
                },
                "required": ["key"]
            }),
        });
    }


    /// Build declarations from system CLI tools.
    pub fn build_from_system_cli(cli_tools: &[(String, String, Value)]) -> Vec<LlmToolDecl> {
        cli_tools.iter().map(|(name, desc, params)| {
            LlmToolDecl {
                name: format!("execute_cli_{}", name),
                description: desc.clone(),
                parameters: params.clone(),
            }
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_tools_count() {
        let mut tools = vec![];
        ToolDeclarationBuilder::append_builtin_tools(&mut tools);
        assert!(tools.len() >= 20, "Expected at least 20 builtin tools, got {}", tools.len());
    }

    #[test]
    fn test_builtin_tools_has_execute_code() {
        let mut tools = vec![];
        ToolDeclarationBuilder::append_builtin_tools(&mut tools);
        assert!(tools.iter().any(|t| t.name == "execute_code"));
    }

    #[test]
    fn test_builtin_tools_has_required_names() {
        let mut tools = vec![];
        ToolDeclarationBuilder::append_builtin_tools(&mut tools);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        for expected in &["execute_code", "create_task", "remember", "recall",
                          "create_session", "list_sessions", "search_knowledge"] {
            assert!(names.contains(expected), "Missing builtin tool: {}", expected);
        }
    }

    #[test]
    fn test_builtin_tools_parameters_are_objects() {
        let mut tools = vec![];
        ToolDeclarationBuilder::append_builtin_tools(&mut tools);
        for tool in &tools {
            assert_eq!(tool.parameters["type"], "object",
                "Tool '{}' parameters should be of type 'object'", tool.name);
        }
    }

    #[test]
    fn test_build_from_system_cli() {
        let cli_tools = vec![
            ("wifi".into(), "Manage WiFi".into(), json!({"type": "object", "properties": {}})),
            ("bt".into(), "Manage Bluetooth".into(), json!({"type": "object", "properties": {}})),
        ];
        let tools = ToolDeclarationBuilder::build_from_system_cli(&cli_tools);
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].name, "execute_cli_wifi");
        assert_eq!(tools[1].name, "execute_cli_bt");
        assert_eq!(tools[0].description, "Manage WiFi");
    }

    #[test]
    fn test_execute_code_has_required_code_param() {
        let mut tools = vec![];
        ToolDeclarationBuilder::append_builtin_tools(&mut tools);
        let ec = tools.iter().find(|t| t.name == "execute_code").unwrap();
        assert!(ec.parameters["properties"]["code"].is_object());
        assert_eq!(ec.parameters["required"][0], "code");
    }
}

