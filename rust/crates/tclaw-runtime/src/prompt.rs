use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{config::RuntimeConfig, git_context::GitContextSnapshot};

const DEFAULT_CONTEXT_FILES: [&str; 5] = [
    "AGENTS.md",
    "CLAUDE.md",
    "README.md",
    "README",
    ".agent/rules/AGENTS.md",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromptFragmentKind {
    System,
    Config,
    Environment,
    ProjectContext,
    GitContext,
    Instruction,
    Memory,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptFragment {
    pub kind: PromptFragmentKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PromptAssembly {
    #[serde(default)]
    pub fragments: Vec<PromptFragment>,
}

impl PromptAssembly {
    pub fn render(&self) -> String {
        self.fragments
            .iter()
            .map(|fragment| fragment.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PromptEnvironmentContext {
    pub operating_system: String,
    pub current_dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_time: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub environment_variables: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextFile {
    pub absolute_path: PathBuf,
    pub relative_path: PathBuf,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectContext {
    pub project_root: PathBuf,
    pub discovery_start: PathBuf,
    #[serde(default)]
    pub context_files: Vec<ContextFile>,
}

impl ProjectContext {
    pub fn discover(start_dir: impl AsRef<Path>) -> Result<Self, PromptError> {
        let start_dir = canonicalize_existing(start_dir.as_ref())?;
        let project_root = discover_project_root(&start_dir)?;
        let candidates = DEFAULT_CONTEXT_FILES
            .iter()
            .map(|path| project_root.join(path))
            .collect::<Vec<_>>();
        let context_files = collect_context_files(&project_root, &candidates)?;

        Ok(Self {
            project_root,
            discovery_start: start_dir,
            context_files,
        })
    }

    pub fn with_additional_paths(
        mut self,
        extra_paths: impl IntoIterator<Item = PathBuf>,
    ) -> Result<Self, PromptError> {
        let merged = self
            .context_files
            .iter()
            .map(|file| file.absolute_path.clone())
            .chain(extra_paths)
            .collect::<Vec<_>>();
        self.context_files = collect_context_files(&self.project_root, &merged)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Default)]
pub struct PromptBuilder {
    system_prompt: Option<String>,
    runtime_config: Option<RuntimeConfig>,
    environment: Option<PromptEnvironmentContext>,
    project_context: Option<ProjectContext>,
    git_context: Option<GitContextSnapshot>,
    instructions: Vec<String>,
    memories: Vec<String>,
}

impl PromptBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    pub fn with_runtime_config(mut self, config: RuntimeConfig) -> Self {
        self.runtime_config = Some(config);
        self
    }

    pub fn with_environment(mut self, environment: PromptEnvironmentContext) -> Self {
        self.environment = Some(environment);
        self
    }

    pub fn with_project_context(mut self, project_context: ProjectContext) -> Self {
        self.project_context = Some(project_context);
        self
    }

    pub fn discover_project_context(
        mut self,
        start_dir: impl AsRef<Path>,
    ) -> Result<Self, PromptError> {
        self.project_context = Some(ProjectContext::discover(start_dir)?);
        Ok(self)
    }

    pub fn with_git_context(mut self, git_context: GitContextSnapshot) -> Self {
        self.git_context = Some(git_context);
        self
    }

    pub fn add_instruction(mut self, instruction: impl Into<String>) -> Self {
        self.instructions.push(instruction.into());
        self
    }

    pub fn add_memory(mut self, memory: impl Into<String>) -> Self {
        self.memories.push(memory.into());
        self
    }

    pub fn build(self) -> Result<PromptAssembly, PromptError> {
        let system_prompt = self
            .system_prompt
            .filter(|prompt| !prompt.trim().is_empty())
            .ok_or(PromptError::MissingSystemPrompt)?;

        let mut fragments = vec![PromptFragment {
            kind: PromptFragmentKind::System,
            label: Some("system".to_string()),
            content: system_prompt,
            source: None,
        }];

        if let Some(config) = self.runtime_config {
            let content =
                serde_json::to_string_pretty(&config).map_err(PromptError::SerializeConfig)?;
            fragments.push(PromptFragment {
                kind: PromptFragmentKind::Config,
                label: Some("runtime_config".to_string()),
                content: format!("## Runtime Config\n{content}"),
                source: None,
            });
        }

        if let Some(environment) = self.environment {
            fragments.push(PromptFragment {
                kind: PromptFragmentKind::Environment,
                label: Some("environment".to_string()),
                content: render_environment(&environment),
                source: None,
            });
        }

        if let Some(project_context) = self.project_context {
            fragments.extend(project_context.context_files.iter().map(|file| {
                PromptFragment {
                    kind: PromptFragmentKind::ProjectContext,
                    label: Some(
                        file.relative_path
                            .to_string_lossy()
                            .replace(std::path::MAIN_SEPARATOR, "/"),
                    ),
                    content: format!(
                        "## Project Context: {}\n{}",
                        file.relative_path.display(),
                        file.content
                    ),
                    source: Some(file.absolute_path.display().to_string()),
                }
            }));
        }

        if let Some(git_context) = self.git_context {
            fragments.push(PromptFragment {
                kind: PromptFragmentKind::GitContext,
                label: Some("git".to_string()),
                content: render_git_context(&git_context),
                source: None,
            });
        }

        if !self.instructions.is_empty() {
            fragments.push(PromptFragment {
                kind: PromptFragmentKind::Instruction,
                label: Some("instructions".to_string()),
                content: render_bulleted_section("Additional Instructions", &self.instructions),
                source: None,
            });
        }

        if !self.memories.is_empty() {
            fragments.push(PromptFragment {
                kind: PromptFragmentKind::Memory,
                label: Some("memory".to_string()),
                content: render_bulleted_section("Persisted Memory", &self.memories),
                source: None,
            });
        }

        Ok(PromptAssembly { fragments })
    }
}

pub fn collect_context_files(
    project_root: &Path,
    candidates: &[PathBuf],
) -> Result<Vec<ContextFile>, PromptError> {
    let project_root = canonicalize_existing(project_root)?;
    let mut seen = BTreeSet::new();
    let mut files = Vec::new();

    for candidate in candidates {
        if !candidate.exists() {
            continue;
        }

        let candidate = canonicalize_existing(candidate)?;
        ensure_within_root(&project_root, &candidate)?;

        if candidate.is_dir() {
            collect_context_files_from_dir(&project_root, &candidate, &mut seen, &mut files)?;
        } else if seen.insert(candidate.clone()) {
            files.push(load_context_file(&project_root, &candidate)?);
        }
    }

    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(files)
}

fn collect_context_files_from_dir(
    project_root: &Path,
    directory: &Path,
    seen: &mut BTreeSet<PathBuf>,
    files: &mut Vec<ContextFile>,
) -> Result<(), PromptError> {
    let mut entries = fs::read_dir(directory)
        .map_err(|source| PromptError::Io {
            path: directory.to_path_buf(),
            source,
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| PromptError::Io {
            path: directory.to_path_buf(),
            source,
        })?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = canonicalize_existing(&entry.path())?;
        ensure_within_root(project_root, &path)?;

        if path.is_dir() {
            collect_context_files_from_dir(project_root, &path, seen, files)?;
            continue;
        }

        if !is_supported_context_file(&path) {
            continue;
        }

        if seen.insert(path.clone()) {
            files.push(load_context_file(project_root, &path)?);
        }
    }

    Ok(())
}

fn load_context_file(project_root: &Path, path: &Path) -> Result<ContextFile, PromptError> {
    let content = fs::read_to_string(path).map_err(|source| PromptError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let relative_path = path
        .strip_prefix(project_root)
        .map_err(|_| PromptError::ContextPathOutsideProject {
            project_root: project_root.to_path_buf(),
            path: path.to_path_buf(),
        })?
        .to_path_buf();

    Ok(ContextFile {
        absolute_path: path.to_path_buf(),
        relative_path,
        content,
    })
}

fn discover_project_root(start_dir: &Path) -> Result<PathBuf, PromptError> {
    let mut current = Some(start_dir);

    while let Some(path) = current {
        if path.join(".git").exists()
            || path.join("CLAUDE.md").exists()
            || path.join("AGENTS.md").exists()
            || path.join("rust").join("Cargo.toml").exists()
            || path.join("Cargo.toml").exists()
        {
            return Ok(path.to_path_buf());
        }
        current = path.parent();
    }

    Err(PromptError::ProjectRootNotFound {
        start: start_dir.to_path_buf(),
    })
}

fn render_environment(environment: &PromptEnvironmentContext) -> String {
    let mut lines = vec![
        "## Environment".to_string(),
        format!("Operating System: {}", environment.operating_system),
        format!("Current Directory: {}", environment.current_dir),
    ];

    if let Some(current_time) = &environment.current_time {
        lines.push(format!("Current Time: {current_time}"));
    }

    if !environment.environment_variables.is_empty() {
        lines.push("Environment Variables:".to_string());
        for (key, value) in &environment.environment_variables {
            lines.push(format!("- {key}={value}"));
        }
    }

    lines.join("\n")
}

fn render_git_context(git_context: &GitContextSnapshot) -> String {
    let mut lines = vec![
        "## Git Context".to_string(),
        format!("Repository Root: {}", git_context.repository_root),
        format!("Current Branch: {}", git_context.current_branch),
        format!(
            "Has Uncommitted Changes: {}",
            git_context.has_uncommitted_changes
        ),
    ];

    if let Some(head_commit) = &git_context.head_commit {
        lines.push(format!("HEAD Commit: {head_commit}"));
    }

    lines.join("\n")
}

fn render_bulleted_section(title: &str, entries: &[String]) -> String {
    let mut lines = vec![format!("## {title}")];
    lines.extend(entries.iter().map(|entry| format!("- {entry}")));
    lines.join("\n")
}

fn is_supported_context_file(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("AGENTS.md" | "CLAUDE.md" | "README.md" | "README")
    )
}

fn ensure_within_root(project_root: &Path, candidate: &Path) -> Result<(), PromptError> {
    if candidate.starts_with(project_root) {
        Ok(())
    } else {
        Err(PromptError::ContextPathOutsideProject {
            project_root: project_root.to_path_buf(),
            path: candidate.to_path_buf(),
        })
    }
}

fn canonicalize_existing(path: &Path) -> Result<PathBuf, PromptError> {
    fs::canonicalize(path).map_err(|source| PromptError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[derive(Debug, Error)]
pub enum PromptError {
    #[error("prompt assembly requires a non-empty system prompt")]
    MissingSystemPrompt,
    #[error("could not read prompt path {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not serialize runtime config for prompt assembly: {0}")]
    SerializeConfig(#[source] serde_json::Error),
    #[error("could not discover project root from {start}")]
    ProjectRootNotFound { start: PathBuf },
    #[error("context path {path} is outside project root {project_root}")]
    ContextPathOutsideProject {
        project_root: PathBuf,
        path: PathBuf,
    },
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;
    use crate::{
        config::{RuntimeConfig, RuntimeProfile},
        permissions::PermissionMode,
    };

    fn temp_dir(test_name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("tclaw-prompt-{test_name}-{unique}"));
        fs::create_dir_all(&path).expect("create temp directory");
        path
    }

    fn cleanup_dir(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    fn write_file(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent directories");
        }
        fs::write(path, content).expect("write test file");
    }

    #[test]
    fn project_context_discovers_root_and_default_files() {
        let root = temp_dir("discover-root");
        write_file(&root.join(".git").join("HEAD"), "ref: refs/heads/main\n");
        write_file(&root.join("CLAUDE.md"), "# Claude\n");
        write_file(&root.join("AGENTS.md"), "# Agents\n");
        write_file(&root.join(".agent/rules/AGENTS.md"), "# Canonical Agents\n");
        let nested = root.join("rust/crates/tclaw-runtime/src");
        fs::create_dir_all(&nested).expect("create nested directory");

        let context = ProjectContext::discover(&nested).expect("discover project context");

        assert_eq!(context.project_root, root);
        let labels = context
            .context_files
            .iter()
            .map(|file| file.relative_path.display().to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            labels,
            vec![".agent/rules/AGENTS.md", "AGENTS.md", "CLAUDE.md"]
        );

        cleanup_dir(&context.project_root);
    }

    #[test]
    fn collect_context_files_deduplicates_duplicate_and_nested_candidates() {
        let root = temp_dir("dedupe");
        write_file(&root.join(".git").join("HEAD"), "ref: refs/heads/main\n");
        write_file(&root.join("AGENTS.md"), "# Root Agents\n");
        write_file(&root.join("CLAUDE.md"), "# Claude\n");
        write_file(&root.join("docs/AGENTS.md"), "# Docs Agents\n");
        write_file(&root.join("docs/notes.txt"), "ignore me\n");

        let files = collect_context_files(
            &root,
            &[
                root.join("AGENTS.md"),
                root.clone(),
                root.join("docs"),
                root.join("docs/AGENTS.md"),
            ],
        )
        .expect("collect context files");

        let labels = files
            .iter()
            .map(|file| file.relative_path.display().to_string())
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["AGENTS.md", "CLAUDE.md", "docs/AGENTS.md"]);

        cleanup_dir(&root);
    }

    #[test]
    fn prompt_builder_renders_deterministic_fragments() {
        let root = temp_dir("render");
        write_file(&root.join(".git").join("HEAD"), "ref: refs/heads/main\n");
        write_file(&root.join("CLAUDE.md"), "# Claude\nProject guidance.\n");
        write_file(&root.join("AGENTS.md"), "# Agents\nRepository guidance.\n");

        let project_context = ProjectContext::discover(&root).expect("discover project context");
        let prompt = PromptBuilder::new()
            .with_system_prompt("You are TizenClaw.")
            .with_runtime_config(RuntimeConfig {
                profile: RuntimeProfile::Host,
                permission_mode: PermissionMode::Ask,
                ..RuntimeConfig::default()
            })
            .with_environment(PromptEnvironmentContext {
                operating_system: "linux".to_string(),
                current_dir: root.display().to_string(),
                current_time: Some("2026-04-12T04:30:00Z".to_string()),
                environment_variables: BTreeMap::from([
                    ("HOME".to_string(), "/tmp/home".to_string()),
                    ("SHELL".to_string(), "/bin/bash".to_string()),
                ]),
            })
            .with_project_context(project_context)
            .with_git_context(GitContextSnapshot {
                repository_root: root.display().to_string(),
                current_branch: "main".to_string(),
                head_commit: Some("abc123".to_string()),
                has_uncommitted_changes: true,
            })
            .add_instruction("Use deterministic prompt fragments.")
            .add_memory("Previous session summary is available.")
            .build()
            .expect("build prompt");

        let labels = prompt
            .fragments
            .iter()
            .map(|fragment| fragment.label.clone().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(
            labels,
            vec![
                "system",
                "runtime_config",
                "environment",
                "AGENTS.md",
                "CLAUDE.md",
                "git",
                "instructions",
                "memory",
            ]
        );
        let rendered = prompt.render();
        assert!(rendered.contains("## Runtime Config"));
        assert!(rendered.contains("## Environment"));
        assert!(rendered.contains("## Project Context: AGENTS.md"));
        assert!(rendered.contains("## Git Context"));
        assert!(rendered.contains("## Persisted Memory"));

        cleanup_dir(&root);
    }
}
