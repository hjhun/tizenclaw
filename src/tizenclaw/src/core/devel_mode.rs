use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEVEL_TASK_ID: &str = "devel-autonomous-cycle";
const DEVEL_TASK_FILE_NAME: &str = "devel-autonomous-cycle.md";
const DEVEL_STATUS_DONE: &str = "done";
const LEGACY_DEFAULT_TASK_SUFFIXES: &[&str] = &[
    "-daily-health-check.md",
    "-memory-watch.md",
    "-log-rollup.md",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevelSyncResult {
    pub task_enabled: bool,
    pub detail: String,
    pub task_path: PathBuf,
    pub status_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DevelPaths {
    repo_root: PathBuf,
    dev_note_dir: PathBuf,
    roadmap_path: PathBuf,
    analysis_path: PathBuf,
    analysis_ko_path: PathBuf,
    dashboard_path: PathBuf,
    prompt_path: PathBuf,
    status_path: PathBuf,
}

impl DevelPaths {
    fn new(repo_root: &Path) -> Self {
        let dev_note_dir = repo_root.join(".dev_note");
        Self {
            repo_root: repo_root.to_path_buf(),
            roadmap_path: dev_note_dir.join("ROADMAP.md"),
            analysis_path: dev_note_dir.join("ANALYSIS.md"),
            analysis_ko_path: dev_note_dir.join("ANALYSIS_KO.md"),
            dashboard_path: dev_note_dir.join("DASHBOARD.md"),
            prompt_path: dev_note_dir.join("PROMPT.md"),
            status_path: dev_note_dir.join(".status"),
            dev_note_dir,
        }
    }
}

pub fn detect_repo_root(start: &Path) -> Option<PathBuf> {
    for candidate in start.ancestors() {
        if candidate.join(".dev_note").is_dir() {
            return Some(candidate.to_path_buf());
        }
    }
    None
}

pub fn sync_devel_task(task_dir: &Path, repo_root: &Path) -> Result<DevelSyncResult, String> {
    let paths = DevelPaths::new(repo_root);
    if !paths.dev_note_dir.is_dir() {
        return Err(format!(
            "Devel mode requires '{}'",
            paths.dev_note_dir.display()
        ));
    }

    fs::create_dir_all(task_dir).map_err(|err| {
        format!(
            "Failed to create task directory '{}': {}",
            task_dir.display(),
            err
        )
    })?;
    remove_legacy_default_tasks(task_dir)?;

    let task_path = task_dir.join(DEVEL_TASK_FILE_NAME);
    if status_is_done(&paths.status_path) {
        remove_task_if_present(&task_path)?;
        return Ok(DevelSyncResult {
            task_enabled: false,
            detail: format!(
                "devel status is '{}' at {}; scheduler disabled",
                DEVEL_STATUS_DONE,
                paths.status_path.display()
            ),
            task_path,
            status_path: paths.status_path,
        });
    }

    let prompt = render_devel_prompt(&paths);
    let content = render_devel_task_file(&paths.repo_root, &prompt);
    write_if_changed(&task_path, &content)?;

    Ok(DevelSyncResult {
        task_enabled: true,
        detail: format!(
            "registered {} every 30m for {}",
            task_path.display(),
            paths.repo_root.display()
        ),
        task_path,
        status_path: paths.status_path,
    })
}

pub fn spawn_devel_task_sync(task_dir: PathBuf, repo_root: PathBuf) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            if let Err(err) = sync_devel_task(&task_dir, &repo_root) {
                log::warn!("Devel mode sync failed: {}", err);
            }
        }
    })
}

fn status_is_done(status_path: &Path) -> bool {
    fs::read_to_string(status_path)
        .ok()
        .map(|content| content.trim().eq_ignore_ascii_case(DEVEL_STATUS_DONE))
        .unwrap_or(false)
}

fn remove_task_if_present(task_path: &Path) -> Result<(), String> {
    match fs::remove_file(task_path) {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!(
            "Failed to remove devel task '{}': {}",
            task_path.display(),
            err
        )),
    }
}

fn remove_legacy_default_tasks(task_dir: &Path) -> Result<(), String> {
    let entries = match fs::read_dir(task_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(format!(
                "Failed to read task directory '{}': {}",
                task_dir.display(),
                err
            ));
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if LEGACY_DEFAULT_TASK_SUFFIXES
            .iter()
            .any(|suffix| file_name.ends_with(suffix))
        {
            remove_task_if_present(&path)?;
        }
    }

    Ok(())
}

fn write_if_changed(path: &Path, content: &str) -> Result<(), String> {
    if fs::read_to_string(path)
        .ok()
        .as_deref()
        .map(|existing| existing == content)
        .unwrap_or(false)
    {
        return Ok(());
    }

    fs::write(path, content)
        .map_err(|err| format!("Failed to write '{}': {}", path.display(), err))
}

fn render_devel_task_file(repo_root: &Path, prompt: &str) -> String {
    format!(
        "\
---
name: Devel roadmap cycle
schedule: interval 30m
interval_secs: 1800
one_shot: false
enabled: true
session_id: scheduler_devel
project_dir: {}
coding_backend: codex
coding_model: gpt-5.4
execution_mode: plan
auto_approve: true
---
{}
",
        repo_root.display(),
        prompt
    )
}

fn render_devel_prompt(paths: &DevelPaths) -> String {
    format!(
        "\
Run the TizenClaw self-development cycle for this repository.

Repository:
- root: {repo_root}

Authoritative files:
- roadmap: {roadmap}
- analysis_ko: {analysis_ko}
- analysis_en: {analysis_en}
- dashboard: {dashboard}
- prompt: {prompt}
- status: {status}
- rust rule: .agent/rules/rust.md

Execution policy:
- Work inside this repository and use Codex CLI through the normal TizenClaw coding-agent path.
- Follow AGENTS.md exactly and record every stage in .dev_note/DASHBOARD.md.
- Use the host-default cycle with ./deploy_host.sh and ./deploy_host.sh --test unless the user explicitly requests a Tizen cycle.
- When daemon-visible behavior changes, add or update a tizenclaw-tests scenario first and use it as the system-test contract.
- During code review, inspect .agent/rules/rust.md, the roadmap goal, runtime safety, and unit/system tests in detail.
- Complete the cycle through commit and push when the implemented roadmap slice is verified.

Required workflow:
1. Open .dev_note/DASHBOARD.md first. If earlier devel work looks incomplete or suspicious, verify it before starting new implementation.
2. If .dev_note/PROMPT.md exists, treat that as the first devel step:
   - analyze PROMPT.md
   - generate or refresh ROADMAP.md and ANALYSIS.md in English
   - refresh any matching analysis context needed for the implementation cycle
   - remove PROMPT.md after the roadmap/analysis artifacts are updated
3. After the prompt step, analyze DASHBOARD.md and ROADMAP.md and pick the next unfinished roadmap item.
4. Implement the selected roadmap work, record the 6 AGENTS stages in DASHBOARD.md, and validate with tizenclaw-tests plus ./deploy_host.sh --test.
5. If development needs the daemon restarted, it may re-run ./deploy_host.sh --devel.
6. When every roadmap item is complete:
   - write done into .dev_note/.status
   - stop further devel work
   - do not continue the timer-driven cycle after completion

Behavior goals:
- ROADMAP.md is the source of truth for remaining development work.
- ANALYSIS_KO.md contains the Korean development context for the ongoing work.
- DASHBOARD.md must stay concise but should reflect plan, design, development, build/deploy, test/review, supervisor gates, and commit/push.
",
        repo_root = paths.repo_root.display(),
        roadmap = paths.roadmap_path.display(),
        analysis_ko = paths.analysis_ko_path.display(),
        analysis_en = paths.analysis_path.display(),
        dashboard = paths.dashboard_path.display(),
        prompt = paths.prompt_path.display(),
        status = paths.status_path.display(),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        DEVEL_TASK_FILE_NAME, DevelSyncResult, detect_repo_root, render_devel_prompt,
        status_is_done, sync_devel_task,
    };

    fn sample_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".dev_note")).unwrap();
        dir
    }

    #[test]
    fn detect_repo_root_finds_ancestor_with_dev_note() {
        let repo = sample_repo();
        let nested = repo.path().join("src/tizenclaw/core");
        std::fs::create_dir_all(&nested).unwrap();

        let detected = detect_repo_root(&nested).unwrap();
        assert_eq!(detected, repo.path());
    }

    #[test]
    fn sync_devel_task_creates_expected_task_file() {
        let repo = sample_repo();
        let tasks = repo.path().join("runtime/tasks");
        let result = sync_devel_task(&tasks, repo.path()).unwrap();

        assert_eq!(
            result,
            DevelSyncResult {
                task_enabled: true,
                detail: format!(
                    "registered {} every 30m for {}",
                    tasks.join(DEVEL_TASK_FILE_NAME).display(),
                    repo.path().display()
                ),
                task_path: tasks.join(DEVEL_TASK_FILE_NAME),
                status_path: repo.path().join(".dev_note/.status"),
            }
        );

        let content = std::fs::read_to_string(tasks.join(DEVEL_TASK_FILE_NAME)).unwrap();
        assert!(content.contains("schedule: interval 30m"));
        assert!(content.contains("coding_backend: codex"));
        assert!(content.contains(".dev_note/ROADMAP.md"));
        assert!(content.contains("./deploy_host.sh --devel"));
    }

    #[test]
    fn sync_devel_task_removes_file_when_status_is_done() {
        let repo = sample_repo();
        let tasks = repo.path().join("runtime/tasks");
        std::fs::create_dir_all(&tasks).unwrap();
        std::fs::write(tasks.join(DEVEL_TASK_FILE_NAME), "placeholder\n").unwrap();
        std::fs::write(repo.path().join(".dev_note/.status"), "done\n").unwrap();

        let result = sync_devel_task(&tasks, repo.path()).unwrap();

        assert!(!result.task_enabled);
        assert!(!tasks.join(DEVEL_TASK_FILE_NAME).exists());
        assert!(status_is_done(&repo.path().join(".dev_note/.status")));
    }

    #[test]
    fn rendered_prompt_references_prompt_stage() {
        let repo = sample_repo();
        let prompt = render_devel_prompt(&super::DevelPaths::new(repo.path()));

        assert!(prompt.contains(".dev_note/PROMPT.md"));
        assert!(prompt.contains("generate or refresh ROADMAP.md and ANALYSIS.md in English"));
        assert!(prompt.contains("write done into .dev_note/.status"));
    }
}
