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
