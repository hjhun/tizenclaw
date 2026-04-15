use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tclaw_runtime::{
    LspClientSpec, PermissionLevel, PermissionScope, TaskRegistrySnapshot, TeamCronRegistry,
    ToolCallRequest, ToolExecutionOutput, ToolRuntimeError, WorkerBootSpec,
};

use crate::{
    ToolManifestEntry, ToolPermissionSpec, ToolRegistration, ToolRegistry, ToolRegistryError,
    ToolSource,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextSearchMatch {
    pub line_number: usize,
    pub line: String,
}

pub trait FileToolBackend {
    fn read_text(&self, path: &str) -> Result<String, String>;
    fn write_text(&mut self, path: &str, content: &str) -> Result<(), String>;
    fn search_text(&self, path: &str, query: &str) -> Result<Vec<TextSearchMatch>, String>;
}

pub trait ShellToolBackend {
    fn execute(
        &mut self,
        program: &str,
        args: &[String],
        cwd: Option<&str>,
    ) -> Result<Value, String>;
}

pub trait FetchToolBackend {
    fn fetch_json(&mut self, url: &str) -> Result<Value, String>;
}

#[derive(Default)]
pub struct NullFileToolBackend;

impl FileToolBackend for NullFileToolBackend {
    fn read_text(&self, path: &str) -> Result<String, String> {
        Err(format!("file backend is not configured for {}", path))
    }

    fn write_text(&mut self, path: &str, _content: &str) -> Result<(), String> {
        Err(format!("file backend is not configured for {}", path))
    }

    fn search_text(&self, path: &str, _query: &str) -> Result<Vec<TextSearchMatch>, String> {
        Err(format!("file backend is not configured for {}", path))
    }
}

#[derive(Default)]
pub struct NullShellToolBackend;

impl ShellToolBackend for NullShellToolBackend {
    fn execute(
        &mut self,
        program: &str,
        _args: &[String],
        _cwd: Option<&str>,
    ) -> Result<Value, String> {
        Err(format!("shell backend is not configured for {}", program))
    }
}

#[derive(Default)]
pub struct NullFetchToolBackend;

impl FetchToolBackend for NullFetchToolBackend {
    fn fetch_json(&mut self, url: &str) -> Result<Value, String> {
        Err(format!("fetch backend is not configured for {}", url))
    }
}

#[derive(Default)]
pub struct InMemoryFileBackend {
    files: BTreeMap<String, String>,
}

impl InMemoryFileBackend {
    pub fn files(&self) -> &BTreeMap<String, String> {
        &self.files
    }
}

impl FileToolBackend for InMemoryFileBackend {
    fn read_text(&self, path: &str) -> Result<String, String> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| format!("file `{}` was not found", path))
    }

    fn write_text(&mut self, path: &str, content: &str) -> Result<(), String> {
        self.files.insert(path.to_string(), content.to_string());
        Ok(())
    }

    fn search_text(&self, path: &str, query: &str) -> Result<Vec<TextSearchMatch>, String> {
        let content = self.read_text(path)?;
        let matches = content
            .lines()
            .enumerate()
            .filter(|(_, line)| line.contains(query))
            .map(|(index, line)| TextSearchMatch {
                line_number: index + 1,
                line: line.to_string(),
            })
            .collect();
        Ok(matches)
    }
}

#[derive(Default)]
pub struct StaticShellToolBackend {
    responses: BTreeMap<String, Value>,
}

impl StaticShellToolBackend {
    pub fn with_command(program: impl Into<String>, response: Value) -> Self {
        let mut responses = BTreeMap::new();
        responses.insert(program.into(), response);
        Self { responses }
    }
}

impl ShellToolBackend for StaticShellToolBackend {
    fn execute(
        &mut self,
        program: &str,
        args: &[String],
        cwd: Option<&str>,
    ) -> Result<Value, String> {
        let mut response = self
            .responses
            .get(program)
            .cloned()
            .unwrap_or_else(|| json!({"status": 0}));
        if let Some(object) = response.as_object_mut() {
            object
                .entry("program".to_string())
                .or_insert_with(|| json!(program));
            object
                .entry("args".to_string())
                .or_insert_with(|| json!(args));
            if let Some(cwd) = cwd {
                object
                    .entry("cwd".to_string())
                    .or_insert_with(|| json!(cwd));
            }
        }
        Ok(response)
    }
}

#[derive(Default)]
pub struct StaticFetchToolBackend {
    documents: BTreeMap<String, Value>,
}

impl StaticFetchToolBackend {
    pub fn with_document(url: impl Into<String>, document: Value) -> Self {
        let mut documents = BTreeMap::new();
        documents.insert(url.into(), document);
        Self { documents }
    }
}

impl FetchToolBackend for StaticFetchToolBackend {
    fn fetch_json(&mut self, url: &str) -> Result<Value, String> {
        self.documents
            .get(url)
            .cloned()
            .ok_or_else(|| format!("document `{}` was not found", url))
    }
}

pub struct GlobalToolContext {
    pub file_backend: Box<dyn FileToolBackend>,
    pub shell_backend: Box<dyn ShellToolBackend>,
    pub fetch_backend: Box<dyn FetchToolBackend>,
    pub task_registry: TaskRegistrySnapshot,
    pub cron_registry: TeamCronRegistry,
    pub workers: Vec<WorkerBootSpec>,
    pub lsp_clients: Vec<LspClientSpec>,
}

impl Default for GlobalToolContext {
    fn default() -> Self {
        Self {
            file_backend: Box::new(NullFileToolBackend),
            shell_backend: Box::new(NullShellToolBackend),
            fetch_backend: Box::new(NullFetchToolBackend),
            task_registry: TaskRegistrySnapshot::default(),
            cron_registry: TeamCronRegistry::default(),
            workers: Vec::new(),
            lsp_clients: Vec::new(),
        }
    }
}

pub fn built_in_tool_registry() -> Result<ToolRegistry<GlobalToolContext>, ToolRegistryError> {
    let mut registry = ToolRegistry::new();
    for registration in built_in_tool_registrations() {
        registry.register(registration)?;
    }
    Ok(registry)
}

fn built_in_tool_registrations() -> Vec<ToolRegistration<GlobalToolContext>> {
    vec![
        ToolRegistration::new(
            manifest(
                "fs.read_text",
                "Read UTF-8 text from a file-like backend",
                json!({
                    "type": "object",
                    "required": ["path"],
                    "properties": {
                        "path": {"type": "string"}
                    }
                }),
                ToolPermissionSpec::new(PermissionScope::Read, PermissionLevel::Low)
                    .with_reason("read text through the file tool backend"),
            )
            .with_aliases(["read_file"])
            .with_tags(["file", "fs"]),
            |call: &ToolCallRequest, ctx: &mut GlobalToolContext| {
                let path = required_string(call, "path")?;
                let content = ctx
                    .file_backend
                    .read_text(&path)
                    .map_err(tool_error(&call.name))?;
                Ok(ToolExecutionOutput {
                    tool_call_id: call.id.clone(),
                    output: json!({"path": path, "content": content}),
                    summary: Some("read file content".to_string()),
                })
            },
        ),
        ToolRegistration::new(
            manifest(
                "fs.write_text",
                "Write UTF-8 text into a file-like backend",
                json!({
                    "type": "object",
                    "required": ["path", "content"],
                    "properties": {
                        "path": {"type": "string"},
                        "content": {"type": "string"}
                    }
                }),
                ToolPermissionSpec::new(PermissionScope::Write, PermissionLevel::Standard)
                    .with_reason("write text through the file tool backend"),
            )
            .with_tags(["file", "fs"]),
            |call: &ToolCallRequest, ctx: &mut GlobalToolContext| {
                let path = required_string(call, "path")?;
                let content = required_string(call, "content")?;
                ctx.file_backend
                    .write_text(&path, &content)
                    .map_err(tool_error(&call.name))?;
                Ok(ToolExecutionOutput {
                    tool_call_id: call.id.clone(),
                    output: json!({"path": path, "written": true}),
                    summary: Some("wrote file content".to_string()),
                })
            },
        ),
        ToolRegistration::new(
            manifest(
                "fs.search_text",
                "Search text content inside a file-like backend",
                json!({
                    "type": "object",
                    "required": ["path", "query"],
                    "properties": {
                        "path": {"type": "string"},
                        "query": {"type": "string"}
                    }
                }),
                ToolPermissionSpec::new(PermissionScope::Read, PermissionLevel::Low)
                    .with_reason("search text through the file tool backend"),
            )
            .with_tags(["file", "search"]),
            |call: &ToolCallRequest, ctx: &mut GlobalToolContext| {
                let path = required_string(call, "path")?;
                let query = required_string(call, "query")?;
                let matches = ctx
                    .file_backend
                    .search_text(&path, &query)
                    .map_err(tool_error(&call.name))?;
                Ok(ToolExecutionOutput {
                    tool_call_id: call.id.clone(),
                    output: json!({"path": path, "query": query, "matches": matches}),
                    summary: Some("searched file content".to_string()),
                })
            },
        ),
        ToolRegistration::new(
            manifest(
                "shell.exec",
                "Execute a shell command through the shell backend",
                json!({
                    "type": "object",
                    "required": ["program"],
                    "properties": {
                        "program": {"type": "string"},
                        "args": {"type": "array", "items": {"type": "string"}},
                        "cwd": {"type": "string"}
                    }
                }),
                ToolPermissionSpec::new(PermissionScope::Execute, PermissionLevel::Sensitive)
                    .with_reason("execute a shell command"),
            )
            .with_tags(["shell", "process"]),
            |call: &ToolCallRequest, ctx: &mut GlobalToolContext| {
                let program = required_string(call, "program")?;
                let args = string_array(call, "args")?;
                let cwd = optional_string(call, "cwd")?;
                let response = ctx
                    .shell_backend
                    .execute(&program, &args, cwd.as_deref())
                    .map_err(tool_error(&call.name))?;
                Ok(ToolExecutionOutput {
                    tool_call_id: call.id.clone(),
                    output: response,
                    summary: Some(format!("executed {}", program)),
                })
            },
        ),
        ToolRegistration::new(
            manifest(
                "net.fetch_json",
                "Fetch a JSON document through the fetch backend",
                json!({
                    "type": "object",
                    "required": ["url"],
                    "properties": {
                        "url": {"type": "string"}
                    }
                }),
                ToolPermissionSpec::new(PermissionScope::Network, PermissionLevel::Standard)
                    .with_reason("fetch a remote JSON document"),
            )
            .with_tags(["network", "fetch"]),
            |call: &ToolCallRequest, ctx: &mut GlobalToolContext| {
                let url = required_string(call, "url")?;
                let document = ctx
                    .fetch_backend
                    .fetch_json(&url)
                    .map_err(tool_error(&call.name))?;
                Ok(ToolExecutionOutput {
                    tool_call_id: call.id.clone(),
                    output: json!({"url": url, "document": document}),
                    summary: Some("fetched JSON document".to_string()),
                })
            },
        ),
        ToolRegistration::new(
            manifest(
                "registry.list_tasks",
                "Expose the current task registry snapshot",
                json!({"type": "object", "properties": {}}),
                ToolPermissionSpec::new(PermissionScope::Read, PermissionLevel::Low),
            )
            .with_tags(["registry", "tasks"]),
            |call: &ToolCallRequest, ctx: &mut GlobalToolContext| {
                Ok(ToolExecutionOutput {
                    tool_call_id: call.id.clone(),
                    output: serde_json::to_value(&ctx.task_registry).unwrap_or_else(|_| json!({})),
                    summary: Some("listed task registry".to_string()),
                })
            },
        ),
        ToolRegistration::new(
            manifest(
                "automation.list_workers",
                "Expose registered worker boot specifications",
                json!({"type": "object", "properties": {}}),
                ToolPermissionSpec::new(PermissionScope::Read, PermissionLevel::Low),
            )
            .with_tags(["automation", "workers"]),
            |call: &ToolCallRequest, ctx: &mut GlobalToolContext| {
                Ok(ToolExecutionOutput {
                    tool_call_id: call.id.clone(),
                    output: json!({"workers": ctx.workers}),
                    summary: Some("listed workers".to_string()),
                })
            },
        ),
        ToolRegistration::new(
            manifest(
                "automation.list_lsp_clients",
                "Expose configured LSP client specifications",
                json!({"type": "object", "properties": {}}),
                ToolPermissionSpec::new(PermissionScope::Read, PermissionLevel::Low),
            )
            .with_tags(["automation", "lsp"]),
            |call: &ToolCallRequest, ctx: &mut GlobalToolContext| {
                Ok(ToolExecutionOutput {
                    tool_call_id: call.id.clone(),
                    output: json!({"clients": ctx.lsp_clients}),
                    summary: Some("listed lsp clients".to_string()),
                })
            },
        ),
        ToolRegistration::new(
            manifest(
                "automation.list_cron_entries",
                "Expose the team cron registry",
                json!({"type": "object", "properties": {}}),
                ToolPermissionSpec::new(PermissionScope::Read, PermissionLevel::Low),
            )
            .with_tags(["automation", "cron"]),
            |call: &ToolCallRequest, ctx: &mut GlobalToolContext| {
                Ok(ToolExecutionOutput {
                    tool_call_id: call.id.clone(),
                    output: json!({"entries": ctx.cron_registry.entries}),
                    summary: Some("listed cron entries".to_string()),
                })
            },
        ),
    ]
}

fn manifest(
    name: &str,
    description: &str,
    input_schema: Value,
    permissions: ToolPermissionSpec,
) -> ToolManifestEntry {
    ToolManifestEntry::new(name, ToolSource::BuiltIn, description, input_schema)
        .with_permissions(permissions)
}

fn required_string(call: &ToolCallRequest, field: &str) -> Result<String, ToolRuntimeError> {
    call.input
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ToolRuntimeError::Execution {
            tool_name: call.name.clone(),
            message: format!("missing required string field `{}`", field),
        })
}

fn optional_string(
    call: &ToolCallRequest,
    field: &str,
) -> Result<Option<String>, ToolRuntimeError> {
    match call.input.get(field) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(ToolRuntimeError::Execution {
            tool_name: call.name.clone(),
            message: format!("field `{}` must be a string", field),
        }),
    }
}

fn string_array(call: &ToolCallRequest, field: &str) -> Result<Vec<String>, ToolRuntimeError> {
    match call.input.get(field) {
        None => Ok(Vec::new()),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| ToolRuntimeError::Execution {
                        tool_name: call.name.clone(),
                        message: format!("field `{}` must only contain strings", field),
                    })
            })
            .collect(),
        Some(_) => Err(ToolRuntimeError::Execution {
            tool_name: call.name.clone(),
            message: format!("field `{}` must be an array of strings", field),
        }),
    }
}

fn tool_error(tool_name: &str) -> impl Fn(String) -> ToolRuntimeError + '_ {
    move |message| ToolRuntimeError::Execution {
        tool_name: tool_name.to_string(),
        message,
    }
}
