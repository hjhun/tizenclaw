//! Tool declaration builder — generates LLM function declarations for all tools.

use crate::llm::backend::LlmToolDecl;
use serde_json::{Value, json};
use std::collections::HashSet;

pub struct ToolDeclarationBuilder;

impl ToolDeclarationBuilder {
    /// Append built-in tool declarations dynamically based on simple intent heuristics.
    /// This drastically reduces token bloat (Token Optimization via Dynamic Tool Loading).
    pub fn append_builtin_tools(tools: &mut Vec<LlmToolDecl>, prompt: &str) {
        let p = prompt.to_lowercase();

        // 1. Meta / System Tools - always injected
        Self::push_meta_tools(tools);

        // 2. Task Intent
        if p.contains("task") || p.contains("schedule") {
            Self::push_task_tools(tools);
        }

        // 3. Memory & Knowledge Intent
        if p.contains("remember")
            || p.contains("memory")
            || p.contains("search")
            || p.contains("knowledge")
        {
            Self::push_memory_tools(tools);
        }

        // 4. Session Intent
        if p.contains("session") || p.contains("switch") || p.contains("user") {
            Self::push_session_tools(tools);
        }

        // 5. Workflow & Pipeline Intent
        if p.contains("workflow")
            || p.contains("pipeline")
            || p.contains("skill")
            || p.contains("learn")
            || p.contains("run")
        {
            Self::push_workflow_tools(tools);
        }

        // 6. Agent Role Intent
        if p.contains("agent") || p.contains("role") || p.contains("supervisor") {
            Self::push_agent_tools(tools);
        }

        // 7. Research / Search Intent
        if p.contains("search")
            || p.contains("research")
            || p.contains("weather")
            || p.contains("stock")
            || p.contains("news")
            || p.contains("conference")
            || p.contains("market")
        {
            Self::push_research_tools(tools);
        }

        // 8. Document / Data Intent
        if p.contains(".pdf")
            || p.contains(".csv")
            || p.contains(".xlsx")
            || p.contains("spreadsheet")
            || p.contains("excel")
            || p.contains("table")
            || p.contains("document")
            || p.contains("summary")
            || p.contains("pdf")
        {
            Self::push_document_tools(tools);
        }

        // 9. Image Intent
        if p.contains("image")
            || p.contains("png")
            || p.contains("jpg")
            || p.contains("jpeg")
            || p.contains("draw")
            || p.contains("illustration")
            || p.contains("photo")
        {
            Self::push_image_tools(tools);
        }

        let mut seen = HashSet::new();
        tools.retain(|tool| seen.insert(tool.name.clone()));
    }

    fn push_meta_tools(tools: &mut Vec<LlmToolDecl>) {
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
        tools.push(LlmToolDecl {
            name: "lookup_web_api".into(),
            description:
                "Look up Tizen Web API reference documentation. Use 'list', 'read', or 'search'."
                    .into(),
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
        tools.push(LlmToolDecl {
            name: "send_outbound_message".into(),
            description: "Send a user-facing outbound update to one or more channels such as web_dashboard or telegram.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "channels": {
                        "type": "array",
                        "description": "Target channel names",
                        "items": {
                            "type": "string",
                            "enum": ["web_dashboard", "telegram"]
                        },
                        "minItems": 1
                    },
                    "message": {
                        "type": "string",
                        "description": "Main message body to deliver"
                    },
                    "title": {
                        "type": "string",
                        "description": "Optional short title for dashboards or rich notifications"
                    },
                    "session_id": {
                        "type": "string",
                        "description": "Optional session id associated with the outbound update"
                    }
                },
                "required": ["channels", "message"]
            }),
        });
        tools.push(LlmToolDecl {
            name: "run_generated_code".into(),
            description: "Write generated Python, Node.js, or Bash code under the device-owned codes directory and execute it immediately. Use this for executable scripts only. Do not use it for HTML/CSS/JS browser apps or webview content; use generate_web_app for those.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "runtime": {
                        "type": "string",
                        "enum": ["python", "python3", "node", "bash"],
                        "description": "Interpreter used to execute the generated code"
                    },
                    "name": {
                        "type": "string",
                        "description": "Optional human-readable script name used in the saved filename"
                    },
                    "code": {"type": "string", "description": "Full source code to write into a reusable script file before execution"},
                    "args": {"type": "string", "description": "Optional command-line arguments passed to the generated script as a single shell-style string"}
                },
                "required": ["runtime", "code"]
            }),
        });
        tools.push(LlmToolDecl {
            name: "manage_generated_code".into(),
            description: "List or delete generated code files stored under the device-owned codes directory. Use this when the user asks to inspect, clean up, or remove generated scripts.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "operation": {
                        "type": "string",
                        "enum": ["list", "delete", "delete_all"],
                        "description": "Management action to perform on stored generated code"
                    },
                    "name": {
                        "type": "string",
                        "description": "Exact filename to delete when operation is 'delete'"
                    }
                },
                "required": ["operation"]
            }),
        });
    }

    fn push_task_tools(tools: &mut Vec<LlmToolDecl>) {
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
        tools.push(LlmToolDecl {
            name: "list_tasks".into(),
            description: "List all scheduled tasks.".into(),
            parameters: json!({"type": "object", "properties": {}, "required": []}),
        });
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
    }

    fn push_memory_tools(tools: &mut Vec<LlmToolDecl>) {
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
        tools.push(LlmToolDecl {
            name: "forget".into(),
            description: "Delete a key-value pair from persistent memory.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "key": {"type": "string"}
                },
                "required": ["key"]
            }),
        });
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
    }

    fn push_session_tools(tools: &mut Vec<LlmToolDecl>) {
        tools.push(LlmToolDecl {
            name: "create_session".into(),
            description: "Create a new agent session with a custom system prompt or an existing role profile.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Short name for the session"},
                    "system_prompt": {"type": "string", "description": "Custom system prompt"},
                    "role": {"type": "string", "description": "Optional built-in or dynamic role name"},
                    "prompt_mode": {"type": "string", "enum": ["full", "minimal"]},
                    "reasoning_policy": {"type": "string", "enum": ["native", "tagged"]}
                },
                "required": ["name"]
            }),
        });
        tools.push(LlmToolDecl {
            name: "list_sessions".into(),
            description: "List all active agent sessions.".into(),
            parameters: json!({"type": "object", "properties": {}, "required": []}),
        });
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
    }

    fn push_workflow_tools(tools: &mut Vec<LlmToolDecl>) {
        tools.push(LlmToolDecl {
            name: "create_pipeline".into(),
            description: "Create a multi-step pipeline for deterministic workflow execution."
                .into(),
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
        tools.push(LlmToolDecl {
            name: "create_skill".into(),
            description: "Create a reusable Anthropic-style textual skill. The daemon normalizes the skill name and writes a canonical SKILL.md workflow document.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Requested skill identifier; it will be normalized to lowercase letters, numbers, and hyphens."},
                    "description": {"type": "string", "description": "Third-person discovery description for Anthropic skill selection."},
                    "content": {"type": "string", "description": "Markdown body for the skill. The daemon will rebuild the YAML frontmatter."}
                },
                "required": ["name", "description", "content"]
            }),
        });
        tools.push(LlmToolDecl {
            name: "read_skill".into(),
            description: "Read the exact markdown content of a previously created textual skill."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Skill identifier to read"}
                },
                "required": ["name"]
            }),
        });
        tools.push(LlmToolDecl {
            name: "list_skill_references".into(),
            description:
                "List the packaged Anthropic skill-reference documents installed on the device."
                    .into(),
            parameters: json!({"type": "object", "properties": {}, "required": []}),
        });
        tools.push(LlmToolDecl {
            name: "read_skill_reference".into(),
            description: "Read a packaged Anthropic skill-reference document such as SKILL_BEST_PRACTICE.md before creating or revising a skill.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Reference document file name or stem. Empty uses the default best-practice guide."}
                },
                "required": []
            }),
        });
        tools.push(LlmToolDecl {
            name: "generate_web_app".into(),
            description: "Generate or update a web application served by the web dashboard at /apps/<app_id>/. Supports HTML/CSS/JS files, optional asset downloads, bridge tool allowlists, and best-effort bridge or webview launch on Tizen.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "app_id": {
                        "type": "string",
                        "description": "Unique identifier for the app (lowercase alphanumeric + underscore, max 64 chars)"
                    },
                    "title": {
                        "type": "string",
                        "description": "Display title for the web app"
                    },
                    "html": {
                        "type": "string",
                        "description": "Complete HTML content. Can be a single-file app or reference style.css and app.js"
                    },
                    "css": {
                        "type": "string",
                        "description": "Optional separate CSS stylesheet saved as style.css"
                    },
                    "js": {
                        "type": "string",
                        "description": "Optional separate JavaScript code saved as app.js"
                    },
                    "assets": {
                        "type": "array",
                        "description": "Optional external assets to download. Each item is {url, filename}. Max 10MB per file.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "url": {"type": "string", "description": "Asset download URL"},
                                "filename": {"type": "string", "description": "Local filename such as logo.png"}
                            }
                        }
                    },
                    "allowed_tools": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Optional tool names this app may call via the bridge API"
                    }
                },
                "required": ["app_id", "title", "html"]
            }),
        });
    }

    fn push_agent_tools(tools: &mut Vec<LlmToolDecl>) {
        tools.push(LlmToolDecl {
            name: "run_supervisor".into(),
            description:
                "Decompose a complex goal into sub-tasks and delegate to specialized role agents."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "goal": {"type": "string", "description": "High-level goal"},
                    "strategy": {"type": "string", "enum": ["sequential", "parallel"]}
                },
                "required": ["goal"]
            }),
        });
        tools.push(LlmToolDecl {
            name: "list_agent_roles".into(),
            description: "List all configured agent roles.".into(),
            parameters: json!({"type": "object", "properties": {}, "required": []}),
        });
        tools.push(LlmToolDecl {
            name: "spawn_agent".into(),
            description:
                "Create a new specialized agent role definition that can be used by new sessions."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {"type": "string", "description": "Unique name"},
                    "system_prompt": {"type": "string", "description": "System prompt"},
                    "allowed_tools": {"type": "array", "items": {"type": "string"}},
                    "max_iterations": {"type": "integer"},
                    "description": {"type": "string"},
                    "type": {"type": "string", "enum": ["worker", "supervisor"]},
                    "auto_start": {"type": "boolean"},
                    "can_delegate_to": {"type": "array", "items": {"type": "string"}},
                    "prompt_mode": {"type": "string", "enum": ["full", "minimal"]},
                    "reasoning_policy": {"type": "string", "enum": ["native", "tagged"]}
                },
                "required": ["name", "system_prompt"]
            }),
        });
    }

    fn push_research_tools(tools: &mut Vec<LlmToolDecl>) {
        tools.push(LlmToolDecl {
            name: "web_search".into(),
            description: "Search the web using the configured search provider stack and return normalized result snippets.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"},
                    "engine": {"type": "string", "description": "Optional engine override"},
                    "limit": {"type": "integer", "description": "Maximum number of results to keep", "minimum": 1, "maximum": 10}
                },
                "required": ["query"]
            }),
        });
        tools.push(LlmToolDecl {
            name: "validate_web_search".into(),
            description: "Inspect search configuration and report which engines are ready to use."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "engine": {"type": "string", "description": "Optional engine name to validate"}
                },
                "required": []
            }),
        });
    }

    fn push_document_tools(tools: &mut Vec<LlmToolDecl>) {
        tools.push(LlmToolDecl {
            name: "extract_document_text".into(),
            description: "Extract readable text from a local document such as TXT, Markdown, JSON, CSV, or PDF.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Document path to read"},
                    "output_path": {"type": "string", "description": "Optional text output file path"},
                    "max_chars": {"type": "integer", "description": "Optional maximum number of characters to return inline", "minimum": 1}
                },
                "required": ["path"]
            }),
        });
        tools.push(LlmToolDecl {
            name: "inspect_tabular_data".into(),
            description: "Inspect CSV or XLSX files and return sheet, header, row count, and preview information.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Tabular file path"},
                    "preview_rows": {"type": "integer", "description": "Preview row count per sheet", "minimum": 1, "maximum": 20}
                },
                "required": ["path"]
            }),
        });
    }

    fn push_image_tools(tools: &mut Vec<LlmToolDecl>) {
        tools.push(LlmToolDecl {
            name: "generate_image".into(),
            description: "Generate an image from a text prompt and save it into the active workdir.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "prompt": {"type": "string", "description": "Image prompt"},
                    "path": {"type": "string", "description": "Relative or absolute output path for the image file"},
                    "size": {"type": "string", "description": "Optional image size such as 1024x1024"},
                    "background": {"type": "string", "description": "Optional background preference"}
                },
                "required": ["prompt", "path"]
            }),
        });
    }

    /// Build declarations from system CLI tools.
    pub fn build_from_system_cli(cli_tools: &[(String, String, Value)]) -> Vec<LlmToolDecl> {
        cli_tools
            .iter()
            .map(|(name, desc, params)| LlmToolDecl {
                name: format!("execute_cli_{}", name),
                description: desc.clone(),
                parameters: params.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_tools_dynamic() {
        let mut tools = vec![];
        ToolDeclarationBuilder::append_builtin_tools(&mut tools, "what is my agent status?");
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"get_agent_status"));
        assert!(names.contains(&"send_outbound_message"));
        assert!(names.contains(&"run_generated_code"));
        assert!(names.contains(&"manage_generated_code"));
        // Task tools shouldn't be here since task intent is missing
        assert!(!names.contains(&"create_task"));

        let mut tools2 = vec![];
        ToolDeclarationBuilder::append_builtin_tools(&mut tools2, "create a new task");
        let names2: Vec<&str> = tools2.iter().map(|t| t.name.as_str()).collect();
        assert!(names2.contains(&"create_task"));
    }

    #[test]
    fn test_build_from_system_cli() {
        let cli_tools = vec![(
            "wifi".into(),
            "Manage WiFi".into(),
            json!({"type": "object", "properties": {}}),
        )];
        let tools = ToolDeclarationBuilder::build_from_system_cli(&cli_tools);
        assert_eq!(tools[0].name, "execute_cli_wifi");
    }
}
