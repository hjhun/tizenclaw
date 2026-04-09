use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEVEL_TASK_FILE_NAME: &str = "devel-autonomous-cycle.md";
const DEVEL_BOOTSTRAP_TASK_FILE_NAME: &str = "devel-autonomous-bootstrap.md";
const DEVEL_STATUS_DONE: &str = "done";
const LEGACY_DEFAULT_TASK_SUFFIXES: &[&str] = &[
    "-daily-health-check.md",
    "-memory-watch.md",
    "-log-rollup.md",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevelSyncResult {
    pub task_enabled: bool,
    pub bootstrap_queued: bool,
    pub detail: String,
    pub task_path: PathBuf,
    pub bootstrap_task_path: PathBuf,
    pub status_path: PathBuf,
    pub last_prompt_fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevelStatus {
    pub repo_root: PathBuf,
    pub task_path: PathBuf,
    pub bootstrap_task_path: PathBuf,
    pub status_path: PathBuf,
    pub status_done: bool,
    pub prompt_exists: bool,
    pub roadmap_has_pending_work: bool,
    pub recurring_task_present: bool,
    pub bootstrap_task_present: bool,
    pub telegram_notifications_enabled: bool,
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

pub fn sync_devel_tasks(task_dir: &Path, repo_root: &Path) -> Result<DevelSyncResult, String> {
    sync_devel_tasks_with_prompt_state(task_dir, repo_root, None)
}

pub fn sync_devel_tasks_with_prompt_state(
    task_dir: &Path,
    repo_root: &Path,
    last_prompt_fingerprint: Option<&str>,
) -> Result<DevelSyncResult, String> {
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
    let bootstrap_task_path = task_dir.join(DEVEL_BOOTSTRAP_TASK_FILE_NAME);
    if status_is_done(&paths.status_path) {
        remove_task_if_present(&task_path)?;
        remove_task_if_present(&bootstrap_task_path)?;
        return Ok(DevelSyncResult {
            task_enabled: false,
            bootstrap_queued: false,
            detail: format!(
                "devel status is '{}' at {}; scheduler disabled",
                DEVEL_STATUS_DONE,
                paths.status_path.display()
            ),
            task_path,
            bootstrap_task_path,
            status_path: paths.status_path,
            last_prompt_fingerprint: current_prompt_fingerprint(&paths.prompt_path),
        });
    }

    let prompt = render_devel_prompt(&paths);
    let recurring_content = render_devel_task_file(&paths.repo_root, &prompt);
    write_if_changed(&task_path, &recurring_content)?;

    let current_prompt_fingerprint = current_prompt_fingerprint(&paths.prompt_path);
    let roadmap_has_pending_work = roadmap_has_pending_work(&paths.roadmap_path);
    let current_work_fingerprint =
        current_work_fingerprint(&paths, current_prompt_fingerprint.clone(), roadmap_has_pending_work);
    let prompt_changed = current_work_fingerprint
        .as_deref()
        .zip(last_prompt_fingerprint)
        .map(|(current, previous)| current != previous)
        .unwrap_or(current_work_fingerprint.is_some());
    let should_bootstrap = !bootstrap_task_path.exists() && prompt_changed;

    if should_bootstrap {
        let bootstrap_content = render_bootstrap_task_file(
            &paths.repo_root,
            &prompt,
            &bootstrap_session_id(current_work_fingerprint.as_deref()),
        );
        write_if_changed(&bootstrap_task_path, &bootstrap_content)?;
    }

    let detail = if should_bootstrap {
        format!(
            "queued immediate devel bootstrap at {} and kept 30m scheduler {}",
            bootstrap_task_path.display(),
            task_path.display()
        )
    } else {
        format!(
            "registered {} every 30m for {}",
            task_path.display(),
            paths.repo_root.display()
        )
    };

    Ok(DevelSyncResult {
        task_enabled: true,
        bootstrap_queued: should_bootstrap,
        detail,
        task_path,
        bootstrap_task_path,
        status_path: paths.status_path,
        last_prompt_fingerprint: current_work_fingerprint,
    })
}

pub fn spawn_devel_task_sync(
    task_dir: PathBuf,
    repo_root: PathBuf,
    last_prompt_fingerprint: Option<String>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut prompt_fingerprint = last_prompt_fingerprint;

        loop {
            interval.tick().await;
            match sync_devel_tasks_with_prompt_state(
                &task_dir,
                &repo_root,
                prompt_fingerprint.as_deref(),
            ) {
                Ok(result) => {
                    prompt_fingerprint = result.last_prompt_fingerprint;
                }
                Err(err) => {
                    log::warn!("Devel mode sync failed: {}", err);
                }
            }
        }
    })
}

pub fn devel_status(task_dir: &Path, repo_root: &Path) -> DevelStatus {
    let paths = DevelPaths::new(repo_root);
    let task_path = task_dir.join(DEVEL_TASK_FILE_NAME);
    let bootstrap_task_path = task_dir.join(DEVEL_BOOTSTRAP_TASK_FILE_NAME);
    let telegram_notifications_enabled = telegram_notifications_enabled();
    DevelStatus {
        repo_root: repo_root.to_path_buf(),
        task_path: task_path.clone(),
        bootstrap_task_path: bootstrap_task_path.clone(),
        status_path: paths.status_path.clone(),
        status_done: status_is_done(&paths.status_path),
        prompt_exists: paths.prompt_path.is_file(),
        roadmap_has_pending_work: roadmap_has_pending_work(&paths.roadmap_path),
        recurring_task_present: task_path.is_file(),
        bootstrap_task_present: bootstrap_task_path.is_file(),
        telegram_notifications_enabled,
    }
}

pub fn devel_status_json(task_dir: &Path, repo_root: &Path) -> Value {
    let status = devel_status(task_dir, repo_root);
    json!({
        "status": "success",
        "repo_root": status.repo_root.display().to_string(),
        "task_path": status.task_path.display().to_string(),
        "bootstrap_task_path": status.bootstrap_task_path.display().to_string(),
        "status_path": status.status_path.display().to_string(),
        "status_done": status.status_done,
        "prompt_exists": status.prompt_exists,
        "roadmap_has_pending_work": status.roadmap_has_pending_work,
        "recurring_task_present": status.recurring_task_present,
        "bootstrap_task_present": status.bootstrap_task_present,
        "telegram_notifications_enabled": status.telegram_notifications_enabled,
    })
}

fn telegram_notifications_enabled() -> bool {
    let config_path = crate::core::runtime_paths::default_data_dir()
        .join("config")
        .join("telegram_config.json");
    telegram_notifications_enabled_for_config(&config_path)
}

fn telegram_notifications_enabled_for_config(config_path: &Path) -> bool {
    let content = match fs::read_to_string(config_path) {
        Ok(content) => content,
        Err(_) => return false,
    };
    let config: Value = match serde_json::from_str(&content) {
        Ok(config) => config,
        Err(_) => return false,
    };
    let bot_token = config
        .get("bot_token")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if bot_token.is_empty() || bot_token == "YOUR_TELEGRAM_BOT_TOKEN_HERE" {
        return false;
    }

    config
        .get("allowed_chat_ids")
        .and_then(Value::as_array)
        .map(|items| items.iter().any(|item| item.as_i64().is_some()))
        .unwrap_or(false)
}

fn current_prompt_fingerprint(prompt_path: &Path) -> Option<String> {
    let metadata = prompt_path.metadata().ok()?;
    let modified = metadata.modified().ok()?;
    let elapsed = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(format!("{}-{}", metadata.len(), elapsed.as_nanos()))
}

fn current_work_fingerprint(
    paths: &DevelPaths,
    prompt_fingerprint: Option<String>,
    roadmap_has_pending_work: bool,
) -> Option<String> {
    prompt_fingerprint.or_else(|| {
        roadmap_has_pending_work
            .then(|| current_file_fingerprint(&paths.roadmap_path))
            .flatten()
            .map(|fingerprint| format!("roadmap-{}", fingerprint))
    })
}

fn current_file_fingerprint(path: &Path) -> Option<String> {
    let metadata = path.metadata().ok()?;
    let modified = metadata.modified().ok()?;
    let elapsed = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(format!("{}-{}", metadata.len(), elapsed.as_nanos()))
}

fn roadmap_has_pending_work(roadmap_path: &Path) -> bool {
    fs::read_to_string(roadmap_path)
        .ok()
        .map(|content| {
            content
                .lines()
                .any(|line| line.trim_start().starts_with("[ ] "))
        })
        .unwrap_or(false)
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

fn render_bootstrap_task_file(repo_root: &Path, prompt: &str, session_id: &str) -> String {
    format!(
        "\
---
name: Devel bootstrap cycle
schedule: once {}
interval_secs: 1
one_shot: true
enabled: true
session_id: {}
project_dir: {}
coding_backend: codex
coding_model: gpt-5.4
execution_mode: plan
auto_approve: true
---
Before waiting for the 30 minute recurring timer, inspect whether immediate development work is pending and process it now.

Immediate triggers:
- `.dev_note/PROMPT.md` was newly created or changed
- `ROADMAP.md` still contains unfinished `[ ]` items

{}
",
        current_local_timestamp_minute(),
        session_id,
        repo_root.display(),
        prompt
    )
}

fn bootstrap_session_id(prompt_fingerprint: Option<&str>) -> String {
    let token = prompt_fingerprint
        .map(sanitize_session_token)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(current_local_timestamp_compact);
    format!("scheduler_devel_bootstrap_{}", token)
}

fn current_local_timestamp_minute() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as libc::time_t;
    let mut tm_buf: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        libc::localtime_r(&now, &mut tm_buf);
    }

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        tm_buf.tm_year + 1900,
        tm_buf.tm_mon + 1,
        tm_buf.tm_mday,
        tm_buf.tm_hour,
        tm_buf.tm_min
    )
}

fn current_local_timestamp_compact() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as libc::time_t;
    let mut tm_buf: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        libc::localtime_r(&now, &mut tm_buf);
    }

    format!(
        "{:04}{:02}{:02}{:02}{:02}{:02}",
        tm_buf.tm_year + 1900,
        tm_buf.tm_mon + 1,
        tm_buf.tm_mday,
        tm_buf.tm_hour,
        tm_buf.tm_min,
        tm_buf.tm_sec
    )
}

fn sanitize_session_token(raw: &str) -> String {
    raw.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

fn render_devel_prompt(paths: &DevelPaths) -> String {
    let telegram_stage_reports = if telegram_notifications_enabled() {
        "enabled"
    } else {
        "disabled"
    };
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
- telegram stage reports: {telegram_stage_reports}

Execution policy:
- Work inside this repository and use Codex CLI through the normal TizenClaw coding-agent path.
- Follow AGENTS.md exactly and record every stage in .dev_note/DASHBOARD.md.
- Use the host-default cycle with ./deploy_host.sh and ./deploy_host.sh --test unless the user explicitly requests a Tizen cycle.
- When daemon-visible behavior changes, add or update a tizenclaw-tests scenario first and use it as the system-test contract.
- During code review, inspect .agent/rules/rust.md, the roadmap goal, runtime safety, and unit/system tests in detail.
- Complete the cycle through commit and push when the implemented roadmap slice is verified.
- When `telegram stage reports` is `enabled`, call `send_outbound_message`
  with channel `telegram` immediately after each `Supervisor Gate after ...`
  PASS entry is written into `.dev_note/DASHBOARD.md`.
- Each Telegram update should include the completed stage, current goal,
  key result, and next step. Delivery failure is a warning, not a blocker.

Required workflow:
1. Open .dev_note/DASHBOARD.md first. If earlier devel work looks incomplete or suspicious, verify it before starting new implementation.
2. Detect whether immediate development is required before waiting for the 30 minute timer.
   Immediate work exists when `.dev_note/PROMPT.md` has been created or changed, or when `ROADMAP.md` still contains unfinished `[ ]` items.
3. If `.dev_note/PROMPT.md` exists, treat that as the first devel step:
   - analyze PROMPT.md
   - generate or refresh ROADMAP.md and ANALYSIS.md in English
   - refresh any matching analysis context needed for the implementation cycle
   - remove PROMPT.md after the roadmap/analysis artifacts are updated
4. After the prompt step, analyze DASHBOARD.md and ROADMAP.md and pick the next unfinished roadmap item.
5. Implement the selected roadmap work, record the 6 AGENTS stages in DASHBOARD.md, and validate with tizenclaw-tests plus ./deploy_host.sh --test.
6. If development needs the daemon restarted, it may re-run ./deploy_host.sh --devel.
7. When every roadmap item is complete:
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
        telegram_stage_reports = telegram_stage_reports,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        bootstrap_session_id, DEVEL_BOOTSTRAP_TASK_FILE_NAME, DEVEL_TASK_FILE_NAME,
        detect_repo_root, devel_status, render_devel_prompt, status_is_done, sync_devel_tasks,
        sync_devel_tasks_with_prompt_state,
    };

    fn sample_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".dev_note")).unwrap();
        std::fs::write(
            dir.path().join(".dev_note/ROADMAP.md"),
            "[ ] Phase 1. Pending work\n",
        )
        .unwrap();
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
    fn initial_sync_creates_recurring_and_bootstrap_tasks_when_work_exists() {
        let repo = sample_repo();
        let tasks = repo.path().join("runtime/tasks");
        let result = sync_devel_tasks(&tasks, repo.path()).unwrap();

        assert!(result.task_enabled);
        assert!(result.bootstrap_queued);
        assert!(tasks.join(DEVEL_TASK_FILE_NAME).exists());
        assert!(tasks.join(DEVEL_BOOTSTRAP_TASK_FILE_NAME).exists());
    }

    #[test]
    fn prompt_change_requeues_bootstrap_task() {
        let repo = sample_repo();
        let tasks = repo.path().join("runtime/tasks");
        let first = sync_devel_tasks(&tasks, repo.path()).unwrap();
        std::fs::remove_file(tasks.join(DEVEL_BOOTSTRAP_TASK_FILE_NAME)).unwrap();
        std::fs::write(repo.path().join(".dev_note/PROMPT.md"), "new prompt\n").unwrap();

        let second = sync_devel_tasks_with_prompt_state(
            &tasks,
            repo.path(),
            first.last_prompt_fingerprint.as_deref(),
        )
        .unwrap();

        assert!(second.bootstrap_queued);
        assert!(tasks.join(DEVEL_BOOTSTRAP_TASK_FILE_NAME).exists());
    }

    #[test]
    fn pending_roadmap_alone_does_not_requeue_bootstrap_after_first_sync() {
        let repo = sample_repo();
        let tasks = repo.path().join("runtime/tasks");
        let first = sync_devel_tasks(&tasks, repo.path()).unwrap();
        assert!(first.bootstrap_queued);

        std::fs::remove_file(tasks.join(DEVEL_BOOTSTRAP_TASK_FILE_NAME)).unwrap();
        let second = sync_devel_tasks_with_prompt_state(
            &tasks,
            repo.path(),
            first.last_prompt_fingerprint.as_deref(),
        )
        .unwrap();

        assert!(!second.bootstrap_queued);
        assert!(!tasks.join(DEVEL_BOOTSTRAP_TASK_FILE_NAME).exists());
    }

    #[test]
    fn sync_devel_task_removes_files_when_status_is_done() {
        let repo = sample_repo();
        let tasks = repo.path().join("runtime/tasks");
        std::fs::create_dir_all(&tasks).unwrap();
        std::fs::write(tasks.join(DEVEL_TASK_FILE_NAME), "placeholder\n").unwrap();
        std::fs::write(tasks.join(DEVEL_BOOTSTRAP_TASK_FILE_NAME), "placeholder\n").unwrap();
        std::fs::write(repo.path().join(".dev_note/.status"), "done\n").unwrap();

        let result = sync_devel_tasks(&tasks, repo.path()).unwrap();

        assert!(!result.task_enabled);
        assert!(!tasks.join(DEVEL_TASK_FILE_NAME).exists());
        assert!(!tasks.join(DEVEL_BOOTSTRAP_TASK_FILE_NAME).exists());
        assert!(status_is_done(&repo.path().join(".dev_note/.status")));
    }

    #[test]
    fn devel_status_reports_prompt_and_pending_work() {
        let repo = sample_repo();
        let tasks = repo.path().join("runtime/tasks");
        sync_devel_tasks(&tasks, repo.path()).unwrap();
        std::fs::write(repo.path().join(".dev_note/PROMPT.md"), "draft prompt\n").unwrap();

        let status = devel_status(&tasks, repo.path());
        assert!(status.roadmap_has_pending_work);
        assert!(status.recurring_task_present);
        assert_eq!(
            status.telegram_notifications_enabled,
            super::telegram_notifications_enabled()
        );
    }

    #[test]
    fn telegram_notification_readiness_requires_token_and_chat_id() {
        let repo = sample_repo();
        let config_dir = repo.path().join(".tizenclaw/config");
        std::fs::create_dir_all(&config_dir).unwrap();
        let config_path = config_dir.join("telegram_config.json");

        std::fs::write(
            &config_path,
            r#"{"bot_token":"YOUR_TELEGRAM_BOT_TOKEN_HERE","allowed_chat_ids":[1]}"#,
        )
        .unwrap();
        assert!(!super::telegram_notifications_enabled_for_config(&config_path));

        std::fs::write(&config_path, r#"{"bot_token":"abc","allowed_chat_ids":[]}"#).unwrap();
        assert!(!super::telegram_notifications_enabled_for_config(&config_path));

        std::fs::write(&config_path, r#"{"bot_token":"abc","allowed_chat_ids":[1]}"#).unwrap();
        assert!(super::telegram_notifications_enabled_for_config(&config_path));
    }

    #[test]
    fn bootstrap_session_id_uses_prompt_fingerprint_when_present() {
        let session_id = bootstrap_session_id(Some("12-345:678"));
        assert_eq!(session_id, "scheduler_devel_bootstrap_12_345_678");
    }

    #[test]
    fn prompt_change_requeues_bootstrap_with_fresh_session_id() {
        let repo = sample_repo();
        let tasks = repo.path().join("runtime/tasks");
        let first = sync_devel_tasks(&tasks, repo.path()).unwrap();
        let first_bootstrap =
            std::fs::read_to_string(tasks.join(DEVEL_BOOTSTRAP_TASK_FILE_NAME)).unwrap();
        let first_session = first_bootstrap
            .lines()
            .find(|line| line.starts_with("session_id: "))
            .unwrap()
            .to_string();

        std::fs::remove_file(tasks.join(DEVEL_BOOTSTRAP_TASK_FILE_NAME)).unwrap();
        std::fs::write(repo.path().join(".dev_note/PROMPT.md"), "new prompt\n").unwrap();
        let second = sync_devel_tasks_with_prompt_state(
            &tasks,
            repo.path(),
            first.last_prompt_fingerprint.as_deref(),
        )
        .unwrap();
        assert!(second.bootstrap_queued);

        let second_bootstrap =
            std::fs::read_to_string(tasks.join(DEVEL_BOOTSTRAP_TASK_FILE_NAME)).unwrap();
        let second_session = second_bootstrap
            .lines()
            .find(|line| line.starts_with("session_id: "))
            .unwrap()
            .to_string();
        assert_ne!(first_session, second_session);
    }

    #[test]
    fn rendered_prompt_references_prompt_stage_and_initial_check() {
        let repo = sample_repo();
        let prompt = render_devel_prompt(&super::DevelPaths::new(repo.path()));

        assert!(prompt.contains(".dev_note/PROMPT.md"));
        assert!(prompt.contains("Detect whether immediate development is required"));
        assert!(prompt.contains("generate or refresh ROADMAP.md and ANALYSIS.md in English"));
        assert!(prompt.contains("write done into .dev_note/.status"));
        assert!(prompt.contains("telegram stage reports"));
        assert!(prompt.contains("send_outbound_message"));
    }
}
