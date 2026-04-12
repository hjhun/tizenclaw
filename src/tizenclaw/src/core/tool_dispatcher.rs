//! Tool dispatcher — routes tool calls from LLM to executors.

use crate::llm::backend::LlmToolDecl;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::time::{timeout, Duration};

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const MAX_TOOL_OUTPUT_BYTES: usize = 10 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolAuditMetadata {
    pub runtime: Option<String>,
    pub script_path: Option<String>,
    pub wrapper_kind: String,
    pub trust_mode: String,
    pub shell_wrapper: bool,
    pub inline_command_carrier: bool,
}

impl Default for ToolAuditMetadata {
    fn default() -> Self {
        Self {
            runtime: None,
            script_path: None,
            wrapper_kind: "direct_binary".into(),
            trust_mode: "direct_binary_only".into(),
            shell_wrapper: false,
            inline_command_carrier: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ToolDecl {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub binary_path: String,
    pub prepend_args: Vec<String>,
    pub timeout_secs: u64,
    pub side_effect: String,
    pub audit: ToolAuditMetadata,
}

pub struct ToolDispatcher {
    tools: HashMap<String, ToolDecl>,
}

#[derive(Debug, Deserialize, Default)]
struct ToolDescriptor {
    name: Option<String>,
    description: Option<String>,
    parameters: Option<Value>,
    binary_path: Option<String>,
    prepend_args: Option<Vec<String>>,
    timeout_secs: Option<u64>,
    side_effect: Option<String>,
}

impl Default for ToolDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolDispatcher {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register(&mut self, decl: ToolDecl) {
        self.tools.insert(decl.name.clone(), decl);
    }

    pub fn unregister(&mut self, name: &str) {
        self.tools.remove(name);
    }

    pub fn get(&self, name: &str) -> Option<&ToolDecl> {
        self.tools.get(name)
    }

    pub fn list(&self) -> Vec<&ToolDecl> {
        let mut tools = self.tools.values().collect::<Vec<_>>();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        tools
    }

    pub fn declarations_for_llm(&self) -> Vec<LlmToolDecl> {
        self.list()
            .into_iter()
            .map(|decl| LlmToolDecl {
                name: decl.name.clone(),
                description: decl.description.clone(),
                parameters: decl.parameters.clone(),
            })
            .collect()
    }

    pub fn get_tool_declarations(&self) -> Vec<LlmToolDecl> {
        self.declarations_for_llm()
    }

    pub fn get_tool_declarations_filtered(&self, keywords: &[String]) -> Vec<LlmToolDecl> {
        if keywords.is_empty() {
            return self.declarations_for_llm();
        }

        let lowered = keywords
            .iter()
            .map(|keyword| keyword.to_ascii_lowercase())
            .collect::<Vec<_>>();

        let filtered = self
            .list()
            .into_iter()
            .filter(|decl| {
                let name = decl.name.to_ascii_lowercase();
                let description = decl.description.to_ascii_lowercase();
                lowered
                    .iter()
                    .any(|keyword| name.contains(keyword) || description.contains(keyword))
            })
            .map(|decl| LlmToolDecl {
                name: decl.name.clone(),
                description: decl.description.clone(),
                parameters: decl.parameters.clone(),
            })
            .collect::<Vec<_>>();

        if filtered.is_empty() {
            self.declarations_for_llm()
        } else {
            filtered
        }
    }

    pub fn load_from_dir(dir: &Path) -> Vec<ToolDecl> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) => {
                log::warn!(
                    "ToolDispatcher: cannot read descriptor dir '{}': {}",
                    dir.display(),
                    error
                );
                return Vec::new();
            }
        };

        let mut loaded = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }

            let raw = match fs::read_to_string(&path) {
                Ok(raw) => raw,
                Err(error) => {
                    log::warn!(
                        "ToolDispatcher: failed to read descriptor '{}': {}",
                        path.display(),
                        error
                    );
                    continue;
                }
            };

            let descriptor = match serde_json::from_str::<ToolDescriptor>(&raw) {
                Ok(descriptor) => descriptor,
                Err(error) => {
                    log::warn!(
                        "ToolDispatcher: invalid JSON descriptor '{}': {}",
                        path.display(),
                        error
                    );
                    continue;
                }
            };

            let Some(name) = descriptor
                .name
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
            else {
                log::warn!(
                    "ToolDispatcher: skipping descriptor '{}' with missing name",
                    path.display()
                );
                continue;
            };

            let Some(binary_path) = descriptor
                .binary_path
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
            else {
                log::warn!(
                    "ToolDispatcher: skipping descriptor '{}' with missing binary_path",
                    path.display()
                );
                continue;
            };

            let resolved_binary = Self::resolve_descriptor_binary_path(dir, &binary_path);
            let prepend_args = descriptor.prepend_args.unwrap_or_default();
            let audit = Self::infer_audit_metadata(
                Path::new(&resolved_binary),
                &prepend_args,
                descriptor.description.as_deref(),
            );

            loaded.push(ToolDecl {
                name,
                description: descriptor.description.unwrap_or_default(),
                parameters: descriptor
                    .parameters
                    .unwrap_or_else(Self::default_parameters_schema),
                binary_path: resolved_binary,
                prepend_args,
                timeout_secs: descriptor.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS),
                side_effect: descriptor.side_effect.unwrap_or_else(|| "none".into()),
                audit,
            });
        }

        loaded.sort_by(|left, right| left.name.cmp(&right.name));
        loaded
    }

    pub fn load_tools_from_paths<'a, I>(&mut self, roots: I)
    where
        I: IntoIterator<Item = &'a str>,
    {
        for root in roots {
            self.load_tools_from_path(root);
        }
    }

    pub fn load_tools_from_path(&mut self, root: &str) {
        let path = Path::new(root);
        if !path.exists() {
            log::warn!("ToolDispatcher: path '{}' does not exist", root);
            return;
        }

        if path.is_dir() {
            for decl in Self::load_from_dir(path) {
                self.register(decl);
            }

            if let Some(decl) = Self::parse_decl_from_dir(path) {
                self.register(decl);
            }

            self.load_tools_from_dir(root);
            self.load_tools_from_root(root);
        }
    }

    pub fn load_tools_from_root(&mut self, root: &str) {
        let entries = match fs::read_dir(root) {
            Ok(entries) => entries,
            Err(error) => {
                log::warn!(
                    "ToolDispatcher: cannot read tools root '{}': {}",
                    root,
                    error
                );
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                self.load_tools_from_dir(&path.to_string_lossy());
            }
        }
    }

    pub fn load_tools_from_dir(&mut self, dir: &str) {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let md_path = path.join("tool.md");
            if !md_path.exists() {
                continue;
            }

            match fs::read_to_string(&md_path) {
                Ok(content) => {
                    if let Some(decl) = Self::parse_tool_md(&content, &path) {
                        self.register(decl);
                    }
                }
                Err(error) => {
                    log::warn!(
                        "ToolDispatcher: failed to read descriptor '{}': {}",
                        md_path.display(),
                        error
                    );
                }
            }
        }
    }

    pub fn audit_summary(&self) -> Value {
        let mut entries = self
            .list()
            .into_iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "binary_path": tool.binary_path,
                    "script_path": tool.audit.script_path,
                    "runtime": tool.audit.runtime,
                    "wrapper_kind": tool.audit.wrapper_kind,
                    "trust_mode": tool.audit.trust_mode,
                    "shell_wrapper": tool.audit.shell_wrapper,
                    "inline_command_carrier": tool.audit.inline_command_carrier,
                    "prepend_args": tool.prepend_args,
                })
            })
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            left.get("name")
                .and_then(Value::as_str)
                .cmp(&right.get("name").and_then(Value::as_str))
        });

        json!({
            "total_count": entries.len(),
            "shell_wrapper_count": entries.iter().filter(|entry| entry["shell_wrapper"].as_bool().unwrap_or(false)).count(),
            "runtime_wrapper_count": entries.iter().filter(|entry| {
                matches!(
                    entry["wrapper_kind"].as_str(),
                    Some("shell_wrapper" | "python_script")
                )
            }).count(),
            "inline_command_carrier_count": entries.iter().filter(|entry| {
                entry["inline_command_carrier"].as_bool().unwrap_or(false)
            }).count(),
            "missing_binary_count": entries.iter().filter(|entry| {
                entry["binary_path"].as_str().map(|value| value.trim().is_empty()).unwrap_or(true)
            }).count(),
            "entries": entries,
        })
    }

    pub fn side_effect_for_tool(&self, tool_name: &str) -> Option<&str> {
        self.tools
            .get(tool_name)
            .map(|decl| decl.side_effect.as_str())
    }

    pub async fn execute(
        &self,
        name: &str,
        args: &Value,
        timeout_override: Option<u64>,
    ) -> Result<Value, String> {
        self.execute_in_dir(name, args, timeout_override, None)
            .await
    }

    pub async fn execute_in_dir(
        &self,
        name: &str,
        args: &Value,
        timeout_override: Option<u64>,
        workdir: Option<&Path>,
    ) -> Result<Value, String> {
        let decl = self
            .get(name)
            .ok_or_else(|| format!("Unknown tool: {}", name))?;
        let binary_path = Path::new(&decl.binary_path);

        Self::verify_executable(binary_path)
            .map_err(|error| format!("Tool '{}' binary check failed: {}", name, error))?;

        let mut command = Command::new(binary_path);
        command
            .args(&decl.prepend_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        if let Some(workdir) = workdir {
            command.current_dir(workdir);
        }

        log::debug!(
            "ToolDispatcher: executing '{}' via '{}' with prepend args {:?}",
            name,
            decl.binary_path,
            decl.prepend_args
        );

        let mut child = command
            .spawn()
            .map_err(|error| format!("Failed to spawn tool '{}': {}", name, error))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| format!("Tool '{}' stdout capture unavailable", name))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| format!("Tool '{}' stderr capture unavailable", name))?;

        let stdout_task =
            tokio::spawn(async move { read_stream_limited(stdout, MAX_TOOL_OUTPUT_BYTES).await });
        let stderr_task =
            tokio::spawn(async move { read_stream_limited(stderr, MAX_TOOL_OUTPUT_BYTES).await });

        let stdin_payload = serde_json::to_vec(args)
            .map_err(|error| format!("Failed to serialize tool args for '{}': {}", name, error))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(&stdin_payload)
                .await
                .map_err(|error| format!("Failed to write stdin for '{}': {}", name, error))?;
            stdin
                .write_all(b"\n")
                .await
                .map_err(|error| format!("Failed to terminate stdin for '{}': {}", name, error))?;
            stdin
                .shutdown()
                .await
                .map_err(|error| format!("Failed to close stdin for '{}': {}", name, error))?;
        }

        let timeout_secs = timeout_override.unwrap_or(decl.timeout_secs);
        let status = match timeout(Duration::from_secs(timeout_secs), child.wait()).await {
            Ok(result) => {
                result.map_err(|error| format!("Failed waiting for tool '{}': {}", name, error))?
            }
            Err(_) => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                return Err(format!(
                    "Tool '{}' timed out after {} seconds",
                    name, timeout_secs
                ));
            }
        };

        let stdout_bytes = stdout_task
            .await
            .map_err(|error| format!("Tool '{}' stdout task failed: {}", name, error))??;
        let stderr_bytes = stderr_task
            .await
            .map_err(|error| format!("Tool '{}' stderr task failed: {}", name, error))??;

        let stdout_text = String::from_utf8_lossy(&stdout_bytes).to_string();
        let stderr_text = String::from_utf8_lossy(&stderr_bytes).to_string();
        if !stderr_text.trim().is_empty() {
            log::debug!("ToolDispatcher: '{}' stderr: {}", name, stderr_text.trim());
        }

        if !status.success() {
            let message = if stderr_text.trim().is_empty() {
                format!(
                    "Tool '{}' exited with status {}",
                    name,
                    status
                        .code()
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "signal".into())
                )
            } else {
                stderr_text.trim().to_string()
            };
            return Err(message);
        }

        match serde_json::from_str::<Value>(&stdout_text) {
            Ok(value) => Ok(value),
            Err(_) => Ok(json!({ "output": stdout_text })),
        }
    }

    fn default_parameters_schema() -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn resolve_descriptor_binary_path(dir: &Path, binary_path: &str) -> String {
        let path = Path::new(binary_path);
        if path.is_absolute() {
            path.to_string_lossy().to_string()
        } else {
            dir.join(path).to_string_lossy().to_string()
        }
    }

    fn parse_decl_from_dir(path: &Path) -> Option<ToolDecl> {
        let md_path = path.join("tool.md");
        if !md_path.exists() {
            return None;
        }

        let content = fs::read_to_string(&md_path).ok()?;
        Self::parse_tool_md(&content, path)
    }

    fn parse_tool_md(content: &str, tool_dir: &Path) -> Option<ToolDecl> {
        let mut name = String::new();
        let mut binary = String::new();
        let mut runtime = String::new();
        let mut script = String::new();
        let mut timeout_secs = DEFAULT_TIMEOUT_SECS;
        let mut side_effect = "reversible".to_string();

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if let Some(value) = line.strip_prefix("name:") {
                name = value.trim().trim_matches('"').to_string();
            } else if let Some(value) = line.strip_prefix("binary:") {
                binary = value.trim().trim_matches('"').to_string();
            } else if let Some(value) = line.strip_prefix("runtime:") {
                runtime = value.trim().trim_matches('"').to_string();
            } else if let Some(value) = line.strip_prefix("script:") {
                script = value.trim().trim_matches('"').to_string();
            } else if let Some(value) = line.strip_prefix("timeout:") {
                timeout_secs = value.trim().parse().unwrap_or(DEFAULT_TIMEOUT_SECS);
            } else if let Some(value) = line.strip_prefix("side_effect:") {
                side_effect = value.trim().trim_matches('"').to_string();
            } else if let Some(value) = line.strip_prefix("# ") {
                if name.is_empty() {
                    name = value.trim().to_string();
                }
            }
        }

        if name.is_empty() {
            name = tool_dir.file_name()?.to_string_lossy().to_string();
        }

        let sanitized_name = Self::sanitize_tool_name(&name);
        let (binary_path, prepend_args) =
            Self::resolve_tool_command(tool_dir, &name, &binary, &runtime, &script);
        let audit = Self::infer_audit_metadata(Path::new(&binary_path), &prepend_args, None);

        Some(ToolDecl {
            name: sanitized_name,
            description: content.trim().chars().take(1536).collect(),
            parameters: json!({"type": "object", "properties": {"args": {"type": "string"}}}),
            binary_path,
            prepend_args,
            timeout_secs,
            side_effect,
            audit,
        })
    }

    fn sanitize_tool_name(name: &str) -> String {
        let sanitized = name
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                    ch
                } else {
                    '_'
                }
            })
            .collect::<String>()
            .trim_matches('_')
            .to_string();

        if sanitized.is_empty() {
            "unknown_tool".into()
        } else {
            sanitized
        }
    }

    fn resolve_tool_command(
        tool_dir: &Path,
        original_name: &str,
        binary: &str,
        runtime: &str,
        script: &str,
    ) -> (String, Vec<String>) {
        let explicit_script = Self::resolve_relative_path(tool_dir, script);
        let inferred_script = Self::find_local_tool_candidate(tool_dir, original_name);
        let selected_script = explicit_script.or(inferred_script);

        if !runtime.trim().is_empty() {
            let runtime_binary = Self::resolve_runtime_binary(runtime);
            let prepend_args = selected_script
                .map(|path| vec![path.to_string_lossy().to_string()])
                .unwrap_or_default();
            return (runtime_binary, prepend_args);
        }

        if !binary.trim().is_empty() {
            return (Self::resolve_binary_path(tool_dir, binary), Vec::new());
        }

        if let Some(script_path) = selected_script {
            if let Some(inferred_runtime) = Self::infer_runtime_for_path(&script_path) {
                return (
                    Self::resolve_runtime_binary(&inferred_runtime),
                    vec![script_path.to_string_lossy().to_string()],
                );
            }

            return (script_path.to_string_lossy().to_string(), Vec::new());
        }

        (
            Self::resolve_binary_path(tool_dir, original_name),
            Vec::new(),
        )
    }

    fn resolve_relative_path(tool_dir: &Path, value: &str) -> Option<PathBuf> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return None;
        }

        let path = Path::new(trimmed);
        Some(if path.is_absolute() {
            path.to_path_buf()
        } else {
            tool_dir.join(path)
        })
    }

    fn find_local_tool_candidate(tool_dir: &Path, original_name: &str) -> Option<PathBuf> {
        [
            original_name.to_string(),
            format!("{}.py", original_name),
            format!("{}.js", original_name),
            format!("{}.mjs", original_name),
            format!("{}.cjs", original_name),
            format!("{}.sh", original_name),
            format!("{}.bash", original_name),
        ]
        .iter()
        .map(|candidate| tool_dir.join(candidate))
        .find(|candidate| candidate.is_file())
    }

    fn resolve_binary_path(tool_dir: &Path, binary: &str) -> String {
        let trimmed = binary.trim();
        if trimmed.is_empty() {
            return String::new();
        }

        let path = Path::new(trimmed);
        if path.is_absolute() {
            return path.to_string_lossy().to_string();
        }

        let local = tool_dir.join(path);
        if local.exists() {
            return local.to_string_lossy().to_string();
        }

        Self::lookup_on_path(trimmed)
    }

    fn resolve_runtime_binary(runtime: &str) -> String {
        let normalized = Self::normalize_runtime_label(runtime);
        Self::lookup_on_path(&normalized)
    }

    fn lookup_on_path(name: &str) -> String {
        match std::process::Command::new("which").arg(name).output() {
            Ok(output) if output.status.success() => {
                let resolved = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if resolved.is_empty() {
                    name.to_string()
                } else {
                    resolved
                }
            }
            _ => name.to_string(),
        }
    }

    fn infer_audit_metadata(
        binary_path: &Path,
        prepend_args: &[String],
        description_hint: Option<&str>,
    ) -> ToolAuditMetadata {
        let script_path = prepend_args
            .iter()
            .find(|arg| Self::looks_like_path(arg))
            .cloned()
            .or_else(|| {
                if Self::path_looks_like_script(binary_path) {
                    Some(binary_path.to_string_lossy().to_string())
                } else {
                    None
                }
            });

        let runtime = script_path
            .as_deref()
            .and_then(|path| Self::infer_runtime_for_path(Path::new(path)))
            .or_else(|| Self::infer_runtime_for_path(binary_path))
            .or_else(|| Self::infer_runtime_from_binary_name(binary_path));

        let shell_wrapper = matches!(runtime.as_deref(), Some("bash" | "sh"));
        let inline_command_carrier = prepend_args
            .iter()
            .any(|arg| matches!(arg.as_str(), "-c" | "-lc" | "/c"))
            || description_hint
                .map(|value| value.contains("inline command"))
                .unwrap_or(false);

        let wrapper_kind = if shell_wrapper {
            "shell_wrapper"
        } else if matches!(runtime.as_deref(), Some("python3")) && script_path.is_some() {
            "python_script"
        } else {
            "direct_binary"
        };

        let trust_mode = if shell_wrapper {
            "shell_allowed"
        } else {
            "direct_binary_only"
        };

        ToolAuditMetadata {
            runtime,
            script_path,
            wrapper_kind: wrapper_kind.into(),
            trust_mode: trust_mode.into(),
            shell_wrapper,
            inline_command_carrier,
        }
    }

    fn looks_like_path(value: &str) -> bool {
        value.contains('/') || value.contains('\\') || Path::new(value).extension().is_some()
    }

    fn path_looks_like_script(path: &Path) -> bool {
        if !path.is_file() {
            return false;
        }

        Self::infer_runtime_from_extension(path).is_some()
            || Self::infer_runtime_from_shebang(path).is_some()
    }

    fn infer_runtime_for_path(path: &Path) -> Option<String> {
        Self::infer_runtime_from_shebang(path).or_else(|| Self::infer_runtime_from_extension(path))
    }

    fn infer_runtime_from_binary_name(path: &Path) -> Option<String> {
        let file_name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
        if file_name.starts_with("python") {
            Some("python3".into())
        } else if file_name == "node" || file_name == "nodejs" {
            Some("node".into())
        } else if file_name == "bash" {
            Some("bash".into())
        } else if file_name == "sh" {
            Some("sh".into())
        } else {
            None
        }
    }

    fn infer_runtime_from_shebang(path: &Path) -> Option<String> {
        let first_line = fs::read_to_string(path)
            .ok()?
            .lines()
            .next()?
            .to_ascii_lowercase();
        if !first_line.starts_with("#!") {
            return None;
        }

        if first_line.contains("python") {
            Some("python3".into())
        } else if first_line.contains("node") {
            Some("node".into())
        } else if first_line.contains("bash") {
            Some("bash".into())
        } else if first_line.contains("/sh") || first_line.ends_with(" sh") {
            Some("sh".into())
        } else {
            None
        }
    }

    fn infer_runtime_from_extension(path: &Path) -> Option<String> {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("py") => Some("python3".into()),
            Some("js" | "mjs" | "cjs") => Some("node".into()),
            Some("sh" | "bash") => Some("bash".into()),
            _ => None,
        }
    }

    fn normalize_runtime_label(runtime: &str) -> String {
        match runtime.trim().to_ascii_lowercase().as_str() {
            "python" | "python3" => "python3",
            "node" | "nodejs" => "node",
            "bash" => "bash",
            "sh" => "sh",
            other => other,
        }
        .to_string()
    }

    fn verify_executable(binary_path: &Path) -> Result<(), String> {
        let metadata = fs::metadata(binary_path).map_err(|error| error.to_string())?;
        if !metadata.is_file() {
            return Err(format!("'{}' is not a file", binary_path.display()));
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o111 == 0 {
                return Err(format!("'{}' is not executable", binary_path.display()));
            }
        }

        Ok(())
    }
}

async fn read_stream_limited<R>(mut reader: R, limit: usize) -> Result<Vec<u8>, String>
where
    R: AsyncRead + Unpin,
{
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8192];

    loop {
        let read = reader
            .read(&mut buffer)
            .await
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }

        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > limit {
            return Err(format!("Tool output exceeded {} bytes", limit));
        }
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{ToolAuditMetadata, ToolDecl, ToolDispatcher};
    use libtizenclaw_core::framework::paths::PlatformPaths;
    use serde_json::json;
    use std::fs;

    fn executable_permissions() -> fs::Permissions {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::Permissions::from_mode(0o755)
        }

        #[cfg(not(unix))]
        {
            fs::metadata(".").unwrap().permissions()
        }
    }

    #[test]
    fn load_from_dir_skips_invalid_descriptors() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("bad.json"), "{not valid json").unwrap();
        fs::write(
            dir.path().join("missing_name.json"),
            json!({
                "binary_path": "/bin/echo"
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            dir.path().join("valid.json"),
            json!({
                "name": "battery",
                "description": "Get battery",
                "parameters": {"type": "object", "properties": {}, "required": []},
                "binary_path": "/bin/echo",
                "timeout_secs": 10,
                "side_effect": "none"
            })
            .to_string(),
        )
        .unwrap();

        let decls = ToolDispatcher::load_from_dir(dir.path());

        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].name, "battery");
        assert_eq!(decls[0].timeout_secs, 10);
    }

    #[test]
    fn load_from_dir_infers_python_audit_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("battery.py");
        fs::write(&script, "#!/usr/bin/env python3\nprint('{}')\n").unwrap();
        fs::set_permissions(&script, executable_permissions()).unwrap();

        fs::write(
            dir.path().join("battery.json"),
            json!({
                "name": "battery",
                "description": "Get battery",
                "parameters": {"type": "object", "properties": {}, "required": []},
                "binary_path": script,
                "timeout_secs": 10,
                "side_effect": "none"
            })
            .to_string(),
        )
        .unwrap();

        let mut decls = ToolDispatcher::load_from_dir(dir.path());
        let decl = decls.remove(0);
        assert_eq!(
            decl.audit,
            ToolAuditMetadata {
                runtime: Some("python3".into()),
                script_path: Some(script.to_string_lossy().to_string()),
                wrapper_kind: "python_script".into(),
                trust_mode: "direct_binary_only".into(),
                shell_wrapper: false,
                inline_command_carrier: false,
            }
        );
    }

    #[tokio::test]
    async fn execute_returns_json_output() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("echo_json.sh");
        fs::write(
            &script,
            "#!/usr/bin/env bash\nread payload\nprintf '%s' \"$payload\"\n",
        )
        .unwrap();
        fs::set_permissions(&script, executable_permissions()).unwrap();

        let mut dispatcher = ToolDispatcher::new();
        dispatcher.register(ToolDecl {
            name: "get_battery_level".into(),
            description: "Get battery".into(),
            parameters: json!({"type": "object"}),
            binary_path: script.to_string_lossy().to_string(),
            prepend_args: Vec::new(),
            timeout_secs: 5,
            side_effect: "none".into(),
            audit: ToolAuditMetadata::default(),
        });

        let output = dispatcher
            .execute("get_battery_level", &json!({"level": 87}), None)
            .await
            .unwrap();
        assert_eq!(output, json!({"level": 87}));
    }

    #[tokio::test]
    async fn execute_wraps_non_json_stdout() {
        let mut dispatcher = ToolDispatcher::new();
        dispatcher.register(ToolDecl {
            name: "plain".into(),
            description: "Plain".into(),
            parameters: json!({"type": "object"}),
            binary_path: "/bin/echo".into(),
            prepend_args: vec!["hello".into()],
            timeout_secs: 5,
            side_effect: "none".into(),
            audit: ToolAuditMetadata::default(),
        });

        let output = dispatcher.execute("plain", &json!({}), None).await.unwrap();
        assert_eq!(output, json!({"output": "hello\n"}));
    }

    #[tokio::test]
    async fn execute_reports_missing_binary() {
        let mut dispatcher = ToolDispatcher::new();
        dispatcher.register(ToolDecl {
            name: "missing".into(),
            description: "Missing".into(),
            parameters: json!({"type": "object"}),
            binary_path: "/definitely/missing/tool".into(),
            prepend_args: Vec::new(),
            timeout_secs: 5,
            side_effect: "none".into(),
            audit: ToolAuditMetadata::default(),
        });

        let error = dispatcher
            .execute("missing", &json!({}), None)
            .await
            .unwrap_err();
        assert!(error.contains("binary check failed"));
    }

    #[tokio::test]
    async fn execute_times_out_and_kills_process() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("started.txt");
        let script = dir.path().join("sleep.sh");
        fs::write(
            &script,
            format!(
                "#!/usr/bin/env bash\nprintf 'started' > '{}'\nsleep 5\nprintf '{{}}'\n",
                marker.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&script, executable_permissions()).unwrap();

        let mut dispatcher = ToolDispatcher::new();
        dispatcher.register(ToolDecl {
            name: "slow".into(),
            description: "Slow".into(),
            parameters: json!({"type": "object"}),
            binary_path: script.to_string_lossy().to_string(),
            prepend_args: Vec::new(),
            timeout_secs: 1,
            side_effect: "none".into(),
            audit: ToolAuditMetadata::default(),
        });

        let error = dispatcher
            .execute("slow", &json!({}), None)
            .await
            .unwrap_err();
        assert!(error.contains("timed out"));
        assert!(marker.exists());
    }

    #[test]
    fn declarations_for_llm_returns_registered_tools() {
        let mut dispatcher = ToolDispatcher::new();
        dispatcher.register(ToolDecl {
            name: "battery".into(),
            description: "Battery".into(),
            parameters: json!({"type": "object"}),
            binary_path: "/bin/echo".into(),
            prepend_args: Vec::new(),
            timeout_secs: 30,
            side_effect: "none".into(),
            audit: ToolAuditMetadata::default(),
        });

        let decls = dispatcher.declarations_for_llm();
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].name, "battery");
    }

    #[tokio::test]
    async fn execute_in_dir_respects_workdir() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = PlatformPaths::from_base(temp.path().join("runtime"));
        let tools_dir = runtime.tools_dir.clone();
        fs::create_dir_all(&tools_dir).unwrap();
        let script = tools_dir.join("pwd.sh");
        fs::write(
            &script,
            "#!/usr/bin/env bash\nread _payload\npwd | tr -d '\\n' | xargs printf '{\"cwd\":\"%s\"}'\n",
        )
        .unwrap();
        fs::set_permissions(&script, executable_permissions()).unwrap();

        let mut dispatcher = ToolDispatcher::new();
        dispatcher.register(ToolDecl {
            name: "pwd".into(),
            description: "pwd".into(),
            parameters: json!({"type": "object"}),
            binary_path: script.to_string_lossy().to_string(),
            prepend_args: Vec::new(),
            timeout_secs: 5,
            side_effect: "none".into(),
            audit: ToolAuditMetadata::default(),
        });

        let result = dispatcher
            .execute_in_dir("pwd", &json!({}), None, Some(runtime.data_dir.as_path()))
            .await
            .unwrap();
        assert_eq!(
            result["cwd"],
            runtime.data_dir.to_string_lossy().to_string()
        );
    }
}
