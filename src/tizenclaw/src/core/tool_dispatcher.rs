//! Tool dispatcher — routes tool calls from LLM to executors.

#![allow(clippy::all)]

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

/// A registered tool declaration.
#[derive(Clone, Debug)]
pub struct ToolDecl {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub binary_path: String,
    pub prepend_args: Vec<String>,
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
        ToolDispatcher {
            tools: HashMap::new(),
        }
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
            if let Some(decl) = Self::parse_decl_from_dir(path) {
                self.register(decl);
            }
            self.load_tools_from_dir(root);
            self.load_tools_from_root(root);
        }
    }

    /// Load tools from a directory containing sub-directories with tool descriptors.
    ///
    /// Each immediate child directory is scanned for `tool.md`.
    /// Generated `index.md` files are documentation and must not be
    /// registered as executable tools.
    pub fn load_tools_from_dir(&mut self, dir: &str) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let md_path = path.join("tool.md");
                if md_path.exists() {
                    if let Ok(content) = std::fs::read_to_string(&md_path) {
                        if let Some(decl) = Self::parse_tool_md(&content, &path) {
                            log::debug!(
                                "ToolDispatcher: registered '{}' from {:?}",
                                decl.name,
                                md_path
                            );
                            self.register(decl);
                        }
                    }
                }
            }
        }
    }

    fn parse_decl_from_dir(path: &Path) -> Option<ToolDecl> {
        let md_path = path.join("tool.md");
        if !md_path.exists() {
            return None;
        }
        let content = std::fs::read_to_string(&md_path).ok()?;
        Self::parse_tool_md(&content, path)
    }

    fn parse_tool_md(content: &str, tool_dir: &std::path::Path) -> Option<ToolDecl> {
        // Parse simple YAML-like frontmatter or markdown headers from tool.md
        let lines: Vec<&str> = content.lines().collect();
        let mut name = String::new();
        let mut description = String::new();
        let mut binary = String::new();
        let mut runtime = String::new();
        let mut script = String::new();
        let mut timeout: u64 = 30;

        for line in &lines {
            let line = line.trim();
            if line.starts_with("name:") {
                name = line[5..].trim().trim_matches('"').to_string();
            } else if line.starts_with("description:") {
                description = line[12..].trim().trim_matches('"').to_string();
            } else if line.starts_with("binary:") {
                binary = line[7..].trim().trim_matches('"').to_string();
            } else if line.starts_with("runtime:") {
                runtime = line[8..].trim().trim_matches('"').to_string();
            } else if line.starts_with("script:") {
                script = line[7..].trim().trim_matches('"').to_string();
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
        let clean_name: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        name = clean_name.trim_matches('_').to_string();

        if name.is_empty() {
            name = "unknown_tool".into();
        }
        let (binary, prepend_args) =
            Self::resolve_tool_command(tool_dir, &original_name, &binary, &runtime, &script);

        if binary.is_empty() {
            log::warn!(
                "ToolDispatcher: binary not found for tool '{}' — \
                 no co-located executable and not on PATH. \
                 Set 'binary: <path>' in the tool descriptor to fix this.",
                original_name
            );
        }

        Some(ToolDecl {
            name,
            description,
            binary_path: binary,
            prepend_args,
            timeout_secs: timeout,
            parameters: json!({"type": "object", "properties": {"args": {"type": "string"}}}),
            side_effect: "reversible".into(),
        })
    }

    fn resolve_tool_command(
        tool_dir: &std::path::Path,
        original_name: &str,
        binary: &str,
        runtime: &str,
        script: &str,
    ) -> (String, Vec<String>) {
        let explicit_script = Self::resolve_relative_path(tool_dir, script);
        let inferred_script = Self::find_local_tool_candidate(tool_dir, original_name);
        let selected_script = explicit_script.or(inferred_script);

        if !runtime.trim().is_empty() {
            let runtime_bin = Self::resolve_runtime_binary(runtime);
            let prepend_args = selected_script
                .map(|path| vec![path.to_string_lossy().to_string()])
                .unwrap_or_default();
            return (runtime_bin, prepend_args);
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

            if script_path.extension().is_some() {
                return (script_path.to_string_lossy().to_string(), Vec::new());
            }
        }

        (
            Self::resolve_binary_path(tool_dir, original_name),
            Vec::new(),
        )
    }

    fn resolve_relative_path(
        tool_dir: &std::path::Path,
        value: &str,
    ) -> Option<std::path::PathBuf> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return None;
        }

        let path = std::path::Path::new(trimmed);
        Some(if path.is_absolute() {
            path.to_path_buf()
        } else {
            tool_dir.join(path)
        })
    }

    fn find_local_tool_candidate(
        tool_dir: &std::path::Path,
        original_name: &str,
    ) -> Option<std::path::PathBuf> {
        let candidates = [
            original_name.to_string(),
            format!("{}.py", original_name),
            format!("{}.js", original_name),
            format!("{}.mjs", original_name),
            format!("{}.cjs", original_name),
            format!("{}.sh", original_name),
            format!("{}.bash", original_name),
        ];

        candidates
            .iter()
            .map(|candidate| tool_dir.join(candidate))
            .find(|path| path.is_file())
    }

    fn resolve_binary_path(tool_dir: &std::path::Path, binary: &str) -> String {
        let trimmed = binary.trim();
        if trimmed.is_empty() {
            return String::new();
        }

        let path = std::path::Path::new(trimmed);
        if path.is_absolute() {
            return trimmed.to_string();
        }

        let local_path = tool_dir.join(path);
        if local_path.exists() {
            return local_path.to_string_lossy().to_string();
        }

        Self::lookup_on_path(trimmed)
    }

    fn resolve_runtime_binary(runtime: &str) -> String {
        let trimmed = runtime.trim();
        if trimmed.is_empty() {
            return String::new();
        }

        let mapped = match trimmed.to_ascii_lowercase().as_str() {
            "python" | "python3" => "python3",
            "node" | "nodejs" => "node",
            "bash" => "bash",
            "sh" => "sh",
            _ => trimmed,
        };

        Self::lookup_on_path(mapped)
    }

    fn lookup_on_path(name: &str) -> String {
        let which_out = std::process::Command::new("which").arg(name).output();
        if let Ok(out) = which_out {
            if out.status.success() {
                let found = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !found.is_empty() {
                    return found;
                }
            }
        }
        name.to_string()
    }

    fn infer_runtime_for_path(path: &std::path::Path) -> Option<String> {
        if let Some(runtime) = Self::infer_runtime_from_shebang(path) {
            return Some(runtime);
        }
        Self::infer_runtime_from_extension(path)
    }

    fn infer_runtime_from_shebang(path: &std::path::Path) -> Option<String> {
        let first_line = std::fs::read_to_string(path)
            .ok()?
            .lines()
            .next()?
            .trim()
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

    fn infer_runtime_from_extension(path: &std::path::Path) -> Option<String> {
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("py") => Some("python3".into()),
            Some("js") | Some("mjs") | Some("cjs") => Some("node".into()),
            Some("sh") | Some("bash") => Some("bash".into()),
            _ => None,
        }
    }

    /// Get all tool declarations for LLM function calling.
    pub fn get_tool_declarations(&self) -> Vec<crate::llm::backend::LlmToolDecl> {
        self.tools
            .values()
            .map(|t| crate::llm::backend::LlmToolDecl {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            })
            .collect()
    }

    /// Get tool declarations filtered by intent keywords.
    pub fn get_tool_declarations_filtered(
        &self,
        keywords: &[String],
    ) -> Vec<crate::llm::backend::LlmToolDecl> {
        if keywords.is_empty() {
            return self.get_tool_declarations();
        }

        let filtered = self
            .tools
            .values()
            .filter(|t| {
                let name_lower = t.name.to_lowercase();
                let desc_lower = t.description.to_lowercase();
                keywords.iter().any(|k| {
                    let kl = k.to_lowercase();
                    name_lower.contains(&kl) || desc_lower.contains(&kl)
                })
            })
            .map(|t| crate::llm::backend::LlmToolDecl {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            })
            .collect::<Vec<_>>();

        if filtered.is_empty() {
            self.get_tool_declarations()
        } else {
            filtered
        }
    }

    /// Execute a tool call.
    pub async fn execute(
        &self,
        tool_name: &str,
        args: &Value,
        workdir: Option<&std::path::Path>,
    ) -> Value {
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

        let mut exec_args = decl.prepend_args.clone();
        exec_args.extend(cmd_args);

        log::debug!(
            "Executing tool '{}': {} {:?}",
            tool_name,
            decl.binary_path,
            exec_args
        );

        let engine = crate::infra::container_engine::ContainerEngine::new();
        let args_ref: Vec<&str> = exec_args.iter().map(|s| s.as_str()).collect();

        let cwd = workdir.map(|path| path.to_string_lossy().to_string());
        match engine
            .execute_oneshot(&decl.binary_path, &args_ref, cwd.as_deref())
            .await
        {
            Ok(val) => val,
            Err(e) => json!({"error": format!("Failed to execute via IPC: {}", e)}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ToolDecl, ToolDispatcher};
    use serde_json::json;
    use std::fs;

    #[test]
    fn parse_tool_md_infers_python_runtime_from_extension() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("demo.py"), "print('ok')\n").unwrap();

        let content = "# demo\n";
        let decl = ToolDispatcher::parse_tool_md(content, dir.path()).unwrap();

        assert!(decl.binary_path.ends_with("python3"));
        assert_eq!(
            decl.prepend_args,
            vec![dir.path().join("demo.py").to_string_lossy().to_string()]
        );
    }

    #[test]
    fn parse_tool_md_infers_node_runtime_from_shebang() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("demo"),
            "#!/usr/bin/env node\nconsole.log('ok');\n",
        )
        .unwrap();

        let content = "# demo\n";
        let decl = ToolDispatcher::parse_tool_md(content, dir.path()).unwrap();

        assert!(decl.binary_path.ends_with("node"));
        assert_eq!(
            decl.prepend_args,
            vec![dir.path().join("demo").to_string_lossy().to_string()]
        );
    }

    #[test]
    fn parse_tool_md_prefers_explicit_runtime_and_script() {
        let dir = tempfile::tempdir().unwrap();
        let script_path = dir.path().join("worker.sh");
        fs::write(&script_path, "echo ok\n").unwrap();

        let content = "\
name: demo
runtime: bash
script: worker.sh
";
        let decl = ToolDispatcher::parse_tool_md(content, dir.path()).unwrap();

        assert!(decl.binary_path.ends_with("bash"));
        assert_eq!(
            decl.prepend_args,
            vec![script_path.to_string_lossy().to_string()]
        );
    }

    #[test]
    fn load_tools_from_dir_ignores_index_only_directories() {
        let root = tempfile::tempdir().unwrap();
        let docs_dir = root.path().join("cli");
        let tool_dir = root.path().join("demo");

        fs::create_dir_all(&docs_dir).unwrap();
        fs::create_dir_all(&tool_dir).unwrap();
        fs::write(
            docs_dir.join("index.md"),
            "# CLI Tools Index\n\nThis is documentation only.\n",
        )
        .unwrap();
        fs::write(tool_dir.join("tool.md"), "# demo\n").unwrap();
        fs::write(tool_dir.join("demo"), "#!/bin/sh\necho ok\n").unwrap();

        let mut dispatcher = ToolDispatcher::new();
        dispatcher.load_tools_from_dir(root.path().to_str().unwrap());

        let names = dispatcher
            .get_tool_declarations()
            .into_iter()
            .map(|decl| decl.name)
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["demo".to_string()]);
    }

    #[test]
    fn get_tool_declarations_filtered_falls_back_to_all_when_keywords_missing() {
        let mut dispatcher = ToolDispatcher::new();
        dispatcher.register(ToolDecl {
            name: "battery_tool".into(),
            description: "Inspect battery health".into(),
            parameters: json!({"type": "object"}),
            binary_path: "/bin/echo".into(),
            prepend_args: Vec::new(),
            timeout_secs: 30,
            side_effect: "none".into(),
        });
        dispatcher.register(ToolDecl {
            name: "calendar_tool".into(),
            description: "Inspect schedule".into(),
            parameters: json!({"type": "object"}),
            binary_path: "/bin/echo".into(),
            prepend_args: Vec::new(),
            timeout_secs: 30,
            side_effect: "none".into(),
        });

        let empty_keywords = dispatcher.get_tool_declarations_filtered(&[]);
        let miss_keywords = dispatcher.get_tool_declarations_filtered(&["nonexistent".into()]);

        assert_eq!(empty_keywords.len(), 2);
        assert_eq!(miss_keywords.len(), 2);
    }
}
