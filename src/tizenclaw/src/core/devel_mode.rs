use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::core::agent_core::AgentCore;

const DEVEL_STATUS_FILE_NAME: &str = "STATUS.md";
const DEVEL_RECURRING_TASK_FILE_NAME: &str = "devel-autonomous-cycle.md";
const LEGACY_DEVEL_STATUS_FILE_NAME: &str = ".status";
const RESULT_EVENT_MASK: u32 = libc::IN_CREATE
    | libc::IN_CLOSE_WRITE
    | libc::IN_MOVED_TO
    | libc::IN_DELETE_SELF
    | libc::IN_MOVE_SELF
    | libc::IN_IGNORED;
const PROGRESS_EVENT_MASK: u32 = libc::IN_CREATE
    | libc::IN_CLOSE_WRITE
    | libc::IN_MODIFY
    | libc::IN_MOVED_TO
    | libc::IN_DELETE_SELF
    | libc::IN_MOVE_SELF
    | libc::IN_IGNORED;
const MAX_TELEGRAM_OUTBOUND_CHARS: usize = 4000;

static DEVEL_WATCHER_ACTIVE: AtomicBool = AtomicBool::new(false);
static LAST_PROMPT_PATH: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

pub async fn run(agent: &AgentCore) {
    let prompts = [
        (
            "devel-ping",
            "Reply with a short single-line health confirmation for devel mode.",
        ),
        (
            "devel-runtime-summary",
            "Summarize the active runtime in two short bullet points.",
        ),
    ];

    println!("== TizenClaw devel mode ==");
    for (session_id, prompt) in prompts {
        println!("-- {} --", session_id);
        println!("Prompt: {}", prompt);
        let response = agent.process_prompt(session_id, prompt, None).await;
        println!("{}", response.trim());
        println!();
    }
    println!("Devel mode sequence completed.");
}

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
    pub prompt_actionable: bool,
    pub status_next_phase: Option<String>,
    pub roadmap_has_pending_work: bool,
    pub roadmap_all_phases_complete: bool,
    pub recurring_task_present: bool,
    pub bootstrap_task_present: bool,
    pub development_required: bool,
    pub telegram_notifications_enabled: bool,
    pub prompt_dir: PathBuf,
    pub progress_dir: PathBuf,
    pub result_dir: PathBuf,
    pub last_prompt_path: Option<PathBuf>,
    pub progress_watcher_active: bool,
    pub result_watcher_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestDevelResult {
    pub result_dir: PathBuf,
    pub available: bool,
    pub latest_result_path: Option<PathBuf>,
    pub latest_prompt_path: Option<PathBuf>,
    pub latest_prompt_result_path: Option<PathBuf>,
    pub latest_prompt_result_available: bool,
    pub latest_result_matches_latest_prompt: bool,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RoadmapProgress {
    total_phases: usize,
    pending_phases: usize,
    next_phase: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DevelPaths {
    devel_dir: PathBuf,
    prompt_dir: PathBuf,
    progress_dir: PathBuf,
    result_dir: PathBuf,
    status_path: PathBuf,
    legacy_status_path: PathBuf,
    roadmap_path: PathBuf,
    recurring_task_path: PathBuf,
    goal_path: PathBuf,
    repo_plan_path: PathBuf,
}

impl DevelPaths {
    fn new(repo_root: &Path) -> Self {
        let data_dir = crate::core::runtime_paths::default_data_dir();
        let devel_dir = data_dir.join("devel");
        Self {
            prompt_dir: devel_dir.join("prompt"),
            progress_dir: devel_dir.join("progress"),
            result_dir: devel_dir.join("result"),
            status_path: repo_root.join(".dev").join(DEVEL_STATUS_FILE_NAME),
            legacy_status_path: repo_root.join(".dev").join(LEGACY_DEVEL_STATUS_FILE_NAME),
            roadmap_path: repo_root.join(".dev").join("ROADMAP.md"),
            recurring_task_path: repo_root.join(".dev").join(DEVEL_RECURRING_TASK_FILE_NAME),
            goal_path: repo_root.join(".dev").join("GOAL.md"),
            repo_plan_path: repo_root.join("PLAN.md"),
            devel_dir,
        }
    }
}

struct InotifyGuard {
    fd: i32,
}

impl Drop for InotifyGuard {
    fn drop(&mut self) {
        DEVEL_WATCHER_ACTIVE.store(false, Ordering::SeqCst);
        unsafe {
            libc::close(self.fd);
        }
    }
}

#[derive(Debug, Default)]
struct ProgressStreamState {
    offset: u64,
    carry: String,
}

pub fn detect_repo_root(start: &Path) -> Option<PathBuf> {
    for candidate in start.ancestors() {
        if candidate.join(".dev").is_dir() {
            return Some(candidate.to_path_buf());
        }
    }
    None
}

pub fn sync_devel_tasks(_task_dir: &Path, repo_root: &Path) -> Result<DevelSyncResult, String> {
    sync_devel_tasks_with_prompt_state(Path::new(""), repo_root, None)
}

pub fn sync_devel_tasks_with_prompt_state(
    _task_dir: &Path,
    repo_root: &Path,
    _last_prompt_fingerprint: Option<&str>,
) -> Result<DevelSyncResult, String> {
    let paths = DevelPaths::new(repo_root);
    ensure_devel_runtime_dirs(&paths)?;
    refresh_last_prompt_path(&paths.prompt_dir);
    let last_prompt = latest_prompt_file(&paths.prompt_dir);
    Ok(DevelSyncResult {
        task_enabled: true,
        bootstrap_queued: false,
        detail: format!(
            "devel prompt bridge ready: prompt {} progress {} result {}",
            paths.prompt_dir.display(),
            paths.progress_dir.display(),
            paths.result_dir.display()
        ),
        task_path: paths.prompt_dir.clone(),
        bootstrap_task_path: paths.result_dir.clone(),
        status_path: paths.status_path,
        last_prompt_fingerprint: last_prompt.as_deref().and_then(current_file_fingerprint),
    })
}

pub fn create_prompt_file(prompt_text: &str) -> Result<PathBuf, String> {
    let text = prompt_text.trim();
    if text.is_empty() {
        return Err("Devel prompt text is empty".to_string());
    }

    let repo_root = std::env::current_dir()
        .ok()
        .and_then(|cwd| detect_repo_root(&cwd))
        .unwrap_or_else(|| PathBuf::from("."));
    let paths = DevelPaths::new(&repo_root);
    ensure_devel_runtime_dirs(&paths)?;

    let file_name = format!("{}_prompt.md", current_local_timestamp_compact());
    let file_path = unique_prompt_path(&paths.prompt_dir, &file_name);
    fs::write(&file_path, format!("{}\n", text)).map_err(|err| {
        format!(
            "Failed to write devel prompt '{}': {}",
            file_path.display(),
            err
        )
    })?;
    set_last_prompt_path(Some(file_path.clone()));
    Ok(file_path)
}

pub fn spawn_devel_task_sync(
    _task_dir: PathBuf,
    repo_root: PathBuf,
    _last_prompt_fingerprint: Option<String>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(err) = watch_devel_dirs(repo_root).await {
            DEVEL_WATCHER_ACTIVE.store(false, Ordering::SeqCst);
            log::warn!("Devel result watcher stopped: {}", err);
        }
    })
}

pub fn devel_status(_task_dir: &Path, repo_root: &Path) -> DevelStatus {
    let paths = DevelPaths::new(repo_root);
    let _ = ensure_devel_runtime_dirs(&paths);
    let roadmap_progress = roadmap_progress(&paths.roadmap_path);
    let status_done = status_is_done(&paths.status_path, &paths.legacy_status_path);
    let mut last_prompt_path = latest_prompt_file(&paths.prompt_dir);
    if last_prompt_path.is_none() {
        last_prompt_path = last_prompt_path_store()
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .filter(|path| path.is_file());
    }
    if last_prompt_path.is_none() {
        last_prompt_path = latest_prompt_path_from_result(&paths.result_dir);
    }
    if last_prompt_path.is_none() {
        last_prompt_path = bootstrap_prompt_path(&paths);
    }
    if last_prompt_path.is_some() {
        set_last_prompt_path(last_prompt_path.clone());
    }
    let prompt_exists = last_prompt_path.is_some();
    let prompt_actionable = prompt_exists && !status_done;
    let roadmap_has_pending_work = roadmap_progress.pending_phases > 0;
    let roadmap_all_phases_complete =
        roadmap_progress.total_phases > 0 && roadmap_progress.pending_phases == 0;
    let recurring_task_present = paths.recurring_task_path.is_file();
    let task_path = if recurring_task_present {
        paths.recurring_task_path.clone()
    } else {
        paths.prompt_dir.clone()
    };
    let development_required =
        !status_done && (prompt_actionable || roadmap_has_pending_work || recurring_task_present);

    DevelStatus {
        repo_root: repo_root.to_path_buf(),
        task_path,
        bootstrap_task_path: paths.result_dir.clone(),
        status_path: paths.status_path.clone(),
        status_done,
        prompt_exists,
        prompt_actionable,
        status_next_phase: roadmap_progress.next_phase,
        roadmap_has_pending_work,
        roadmap_all_phases_complete,
        recurring_task_present,
        bootstrap_task_present: false,
        development_required,
        telegram_notifications_enabled: telegram_notifications_enabled(),
        prompt_dir: paths.prompt_dir,
        progress_dir: paths.progress_dir,
        result_dir: paths.result_dir,
        last_prompt_path,
        progress_watcher_active: DEVEL_WATCHER_ACTIVE.load(Ordering::SeqCst),
        result_watcher_active: DEVEL_WATCHER_ACTIVE.load(Ordering::SeqCst),
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
        "prompt_actionable": status.prompt_actionable,
        "status_next_phase": status.status_next_phase,
        "roadmap_has_pending_work": status.roadmap_has_pending_work,
        "roadmap_all_phases_complete": status.roadmap_all_phases_complete,
        "recurring_task_present": status.recurring_task_present,
        "bootstrap_task_present": status.bootstrap_task_present,
        "development_required": status.development_required,
        "telegram_notifications_enabled": status.telegram_notifications_enabled,
        "prompt_dir": status.prompt_dir.display().to_string(),
        "progress_dir": status.progress_dir.display().to_string(),
        "result_dir": status.result_dir.display().to_string(),
        "last_prompt_path": status
            .last_prompt_path
            .as_ref()
            .map(|path| path.display().to_string()),
        "progress_watcher_active": status.progress_watcher_active,
        "result_watcher_active": status.result_watcher_active,
    })
}

pub fn latest_devel_result(repo_root: &Path) -> LatestDevelResult {
    let paths = DevelPaths::new(repo_root);
    let _ = ensure_devel_runtime_dirs(&paths);
    let latest_result_path = latest_result_file(&paths.result_dir);
    let latest_prompt_path = latest_prompt_file(&paths.prompt_dir)
        .or_else(|| {
            latest_result_path
                .as_ref()
                .and_then(|path| prompt_path_from_result_file(path))
        })
        .or_else(|| bootstrap_prompt_path(&paths));
    let latest_prompt_result_path = latest_prompt_path
        .as_ref()
        .map(|path| result_path_for_prompt(&paths.result_dir, path));
    let latest_prompt_result_available = latest_prompt_result_path
        .as_ref()
        .map(|path| path.is_file())
        .unwrap_or(false);
    let latest_result_matches_latest_prompt = match (
        latest_result_path.as_ref(),
        latest_prompt_result_path.as_ref(),
    ) {
        (_, None) => true,
        (Some(result_path), Some(prompt_result_path)) => result_path == prompt_result_path,
        (None, Some(_)) => false,
    };
    let content = latest_result_path
        .as_ref()
        .and_then(|path| fs::read_to_string(path).ok())
        .unwrap_or_default();

    LatestDevelResult {
        result_dir: paths.result_dir,
        available: latest_result_path.is_some(),
        latest_result_path,
        latest_prompt_path,
        latest_prompt_result_path,
        latest_prompt_result_available,
        latest_result_matches_latest_prompt,
        content,
    }
}

pub fn devel_result_json(_task_dir: &Path, repo_root: &Path) -> Value {
    let result = latest_devel_result(repo_root);
    json!({
        "status": "success",
        "result_dir": result.result_dir.display().to_string(),
        "available": result.available,
        "latest_result_path": result
            .latest_result_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        "latest_prompt_path": result
            .latest_prompt_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        "latest_prompt_result_path": result
            .latest_prompt_result_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default(),
        "latest_prompt_result_available": result.latest_prompt_result_available,
        "latest_result_matches_latest_prompt": result.latest_result_matches_latest_prompt,
        "content": result.content,
    })
}

fn ensure_devel_runtime_dirs(paths: &DevelPaths) -> Result<(), String> {
    fs::create_dir_all(&paths.devel_dir).map_err(|err| {
        format!(
            "Failed to create devel directory '{}': {}",
            paths.devel_dir.display(),
            err
        )
    })?;
    fs::create_dir_all(&paths.prompt_dir).map_err(|err| {
        format!(
            "Failed to create prompt directory '{}': {}",
            paths.prompt_dir.display(),
            err
        )
    })?;
    fs::create_dir_all(&paths.progress_dir).map_err(|err| {
        format!(
            "Failed to create progress directory '{}': {}",
            paths.progress_dir.display(),
            err
        )
    })?;
    fs::create_dir_all(&paths.result_dir).map_err(|err| {
        format!(
            "Failed to create result directory '{}': {}",
            paths.result_dir.display(),
            err
        )
    })?;
    Ok(())
}

async fn watch_devel_dirs(repo_root: PathBuf) -> Result<(), String> {
    let paths = DevelPaths::new(&repo_root);
    ensure_devel_runtime_dirs(&paths)?;

    let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
    if fd < 0 {
        return Err(format!(
            "Failed to initialize inotify for '{}': {}",
            paths.result_dir.display(),
            std::io::Error::last_os_error()
        ));
    }

    let _guard = InotifyGuard { fd };
    DEVEL_WATCHER_ACTIVE.store(true, Ordering::SeqCst);
    let mut result_watch_descriptor = add_inotify_watch(fd, &paths.result_dir, RESULT_EVENT_MASK)?;
    let mut progress_watch_descriptor =
        add_inotify_watch(fd, &paths.progress_dir, PROGRESS_EVENT_MASK)?;
    let mut processed_results = HashSet::new();
    let mut completed_prompts = HashSet::new();
    let mut progress_streams = HashMap::new();
    let mut buffer = vec![0u8; 8192];

    loop {
        ensure_devel_runtime_dirs(&paths)?;
        let read_len =
            unsafe { libc::read(fd, buffer.as_mut_ptr() as *mut libc::c_void, buffer.len()) };

        if read_len > 0 {
            let mut reset_result_watch = false;
            let mut reset_progress_watch = false;
            for event in parse_inotify_events(&buffer[..read_len as usize]) {
                let is_result_watch = event.watch_descriptor == result_watch_descriptor;
                let is_progress_watch = event.watch_descriptor == progress_watch_descriptor;

                if event.mask & (libc::IN_DELETE_SELF | libc::IN_MOVE_SELF | libc::IN_IGNORED) != 0
                {
                    if is_result_watch {
                        reset_result_watch = true;
                    }
                    if is_progress_watch {
                        reset_progress_watch = true;
                    }
                }

                if is_result_watch
                    && event.mask & (libc::IN_CREATE | libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO)
                        != 0
                {
                    if let Some(name) = event.name.as_deref() {
                        let file_path = paths.result_dir.join(name);
                        if let Some(prompt_key) = prompt_key_for_result_file(&file_path) {
                            completed_prompts.insert(prompt_key.clone());
                            progress_streams.remove(&prompt_key);
                        }
                        process_result_file(&file_path, &mut processed_results).await;
                    }
                }

                if is_progress_watch
                    && event.mask
                        & (libc::IN_CREATE
                            | libc::IN_CLOSE_WRITE
                            | libc::IN_MODIFY
                            | libc::IN_MOVED_TO)
                        != 0
                {
                    if let Some(name) = event.name.as_deref() {
                        let file_path = paths.progress_dir.join(name);
                        process_progress_file(
                            &paths,
                            &file_path,
                            event.mask & libc::IN_CLOSE_WRITE != 0,
                            &completed_prompts,
                            &mut progress_streams,
                        )
                        .await;
                    }
                }
            }
            if reset_result_watch {
                remove_inotify_watch(fd, result_watch_descriptor);
                result_watch_descriptor =
                    add_inotify_watch(fd, &paths.result_dir, RESULT_EVENT_MASK)?;
            }
            if reset_progress_watch {
                remove_inotify_watch(fd, progress_watch_descriptor);
                progress_watch_descriptor =
                    add_inotify_watch(fd, &paths.progress_dir, PROGRESS_EVENT_MASK)?;
            }
            continue;
        }

        if read_len < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() != std::io::ErrorKind::WouldBlock {
                log::warn!(
                    "Devel result watcher read failed for '{}': {}",
                    paths.result_dir.display(),
                    err
                );
            }
        }

        tokio::time::sleep(Duration::from_millis(400)).await;
    }
}

async fn process_progress_file(
    paths: &DevelPaths,
    path: &Path,
    flush_partial: bool,
    completed_prompts: &HashSet<String>,
    progress_streams: &mut HashMap<String, ProgressStreamState>,
) {
    let Some(prompt_key) = prompt_key_for_progress_file(path) else {
        return;
    };
    if completed_prompts.contains(&prompt_key)
        || result_path_for_prompt_key(&paths.result_dir, &prompt_key).is_file()
    {
        progress_streams.remove(&prompt_key);
        return;
    }

    let state = progress_streams.entry(prompt_key.clone()).or_default();
    let chunk = match read_progress_delta(path, state, flush_partial) {
        Ok(chunk) => chunk,
        Err(err) => {
            log::warn!(
                "Failed to stream devel progress '{}': {}",
                path.display(),
                err
            );
            return;
        }
    };
    if chunk.trim().is_empty() {
        return;
    }

    let title = format!(
        "Devel progress [{}]",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("progress")
    );
    if let Err(err) = send_telegram_outbound_message(&title, &chunk).await {
        log::warn!(
            "Failed to forward devel progress '{}': {}",
            path.display(),
            err
        );
    }
}

async fn process_result_file(path: &Path, processed: &mut HashSet<String>) {
    let Some(fingerprint) = current_file_fingerprint(path) else {
        return;
    };
    let key = format!("{}:{}", path.display(), fingerprint);
    if processed.contains(&key) {
        return;
    }

    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) => {
            log::warn!("Failed to read devel result '{}': {}", path.display(), err);
            return;
        }
    };
    if content.trim().is_empty() {
        return;
    }

    processed.insert(key);
    let title = format!(
        "Devel result [{}]",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("result")
    );
    if let Err(err) = send_telegram_outbound_message(&title, content.trim()).await {
        log::warn!(
            "Failed to forward devel result '{}': {}",
            path.display(),
            err
        );
    }
}

async fn send_telegram_outbound_message(title: &str, message: &str) -> Result<(), String> {
    let config_path = crate::core::runtime_paths::default_data_dir()
        .join("config")
        .join("telegram_config.json");
    let content = fs::read_to_string(&config_path).map_err(|err| {
        format!(
            "Failed to read telegram config '{}': {}",
            config_path.display(),
            err
        )
    })?;
    let config: Value = serde_json::from_str(&content)
        .map_err(|err| format!("Invalid telegram config JSON: {}", err))?;

    let bot_token = config
        .get("bot_token")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if bot_token.is_empty() || bot_token == "YOUR_TELEGRAM_BOT_TOKEN_HERE" {
        return Err("Telegram bot token is not configured".to_string());
    }

    let chat_ids = config
        .get("allowed_chat_ids")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
        .unwrap_or_default();
    if chat_ids.is_empty() {
        return Err("No allowed_chat_ids configured for Telegram".to_string());
    }

    let client = crate::infra::http_client::HttpClient::new();
    let url = format!("https://api.telegram.org/bot{}/sendMessage", bot_token);
    let composed = format!("{}\n\n{}", title, message);
    let body = if composed.chars().count() > MAX_TELEGRAM_OUTBOUND_CHARS {
        format!(
            "{}\n...(truncated)",
            utf8_safe_preview(&composed, MAX_TELEGRAM_OUTBOUND_CHARS)
        )
    } else {
        composed
    };
    for chat_id in chat_ids {
        let payload = json!({
            "chat_id": chat_id,
            "text": body,
        })
        .to_string();
        client
            .post(&url, &payload)
            .await
            .map_err(|err| format!("Telegram outbound failed for chat {}: {}", chat_id, err))?;
    }
    Ok(())
}

fn add_inotify_watch(fd: i32, dir: &Path, mask: u32) -> Result<i32, String> {
    let dir_string = dir.to_string_lossy().to_string();
    let c_path = std::ffi::CString::new(dir_string.as_bytes())
        .map_err(|_| format!("Invalid watch path '{}'", dir.display()))?;
    let watch_descriptor = unsafe { libc::inotify_add_watch(fd, c_path.as_ptr(), mask) };
    if watch_descriptor < 0 {
        Err(format!(
            "Failed to watch result directory '{}': {}",
            dir.display(),
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(watch_descriptor)
    }
}

fn remove_inotify_watch(fd: i32, watch_descriptor: i32) {
    if watch_descriptor >= 0 {
        unsafe {
            libc::inotify_rm_watch(fd, watch_descriptor);
        }
    }
}

#[derive(Debug)]
struct ParsedInotifyEvent {
    watch_descriptor: i32,
    mask: u32,
    name: Option<String>,
}

fn parse_inotify_events(buffer: &[u8]) -> Vec<ParsedInotifyEvent> {
    let mut offset = 0usize;
    let mut events = Vec::new();
    let header_len = std::mem::size_of::<libc::inotify_event>();

    while offset + header_len <= buffer.len() {
        let event = unsafe { &*(buffer[offset..].as_ptr() as *const libc::inotify_event) };
        let name_len = event.len as usize;
        let name_start = offset + header_len;
        let name_end = name_start.saturating_add(name_len).min(buffer.len());
        let name = if name_len == 0 || name_start >= buffer.len() {
            None
        } else {
            let raw = &buffer[name_start..name_end];
            let nul = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
            let trimmed = String::from_utf8_lossy(&raw[..nul]).trim().to_string();
            (!trimmed.is_empty()).then_some(trimmed)
        };
        events.push(ParsedInotifyEvent {
            watch_descriptor: event.wd,
            mask: event.mask,
            name,
        });
        offset = name_end;
    }

    events
}

fn latest_prompt_file(prompt_dir: &Path) -> Option<PathBuf> {
    latest_matching_file(prompt_dir, |name| name.ends_with("_prompt.md"))
}

fn latest_result_file(result_dir: &Path) -> Option<PathBuf> {
    latest_matching_file(result_dir, |_| true)
}

fn latest_matching_file(dir: &Path, matcher: impl Fn(&str) -> bool) -> Option<PathBuf> {
    let mut files = fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(&matcher)
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();

    files.sort_by(|left, right| {
        let left_key = current_file_fingerprint(left).unwrap_or_default();
        let right_key = current_file_fingerprint(right).unwrap_or_default();
        right_key.cmp(&left_key)
    });
    files.into_iter().next()
}

fn unique_prompt_path(prompt_dir: &Path, base_name: &str) -> PathBuf {
    let candidate = prompt_dir.join(base_name);
    if !candidate.exists() {
        return candidate;
    }

    let stem = base_name.strip_suffix(".md").unwrap_or(base_name);
    for suffix in 1..1000usize {
        let next = prompt_dir.join(format!("{}_{}.md", stem, suffix));
        if !next.exists() {
            return next;
        }
    }
    prompt_dir.join(format!("{}_overflow.md", stem))
}

fn result_path_for_prompt(result_dir: &Path, prompt_path: &Path) -> PathBuf {
    let prompt_name = prompt_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("prompt.md");
    let stem = prompt_name.strip_suffix(".md").unwrap_or(prompt_name);
    result_dir.join(format!("{}_RESULT.md", stem))
}

fn result_path_for_prompt_key(result_dir: &Path, prompt_key: &str) -> PathBuf {
    result_dir.join(format!("{}_RESULT.md", prompt_key))
}

fn prompt_key_for_result_file(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix("_RESULT.md").map(str::to_string))
}

fn prompt_key_for_progress_file(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix("_progress.log").map(str::to_string))
}

fn utf8_safe_preview(text: &str, max_chars: usize) -> &str {
    if max_chars == 0 {
        return "";
    }
    match text.char_indices().nth(max_chars) {
        Some((index, _)) => &text[..index],
        None => text,
    }
}

fn read_progress_delta(
    path: &Path,
    state: &mut ProgressStreamState,
    flush_partial: bool,
) -> Result<String, String> {
    let mut file = fs::File::open(path)
        .map_err(|err| format!("Failed to open progress file '{}': {}", path.display(), err))?;
    let metadata = file
        .metadata()
        .map_err(|err| format!("Failed to stat progress file '{}': {}", path.display(), err))?;

    if metadata.len() < state.offset {
        state.offset = 0;
        state.carry.clear();
    }

    file.seek(SeekFrom::Start(state.offset)).map_err(|err| {
        format!(
            "Failed to seek progress file '{}' at {}: {}",
            path.display(),
            state.offset,
            err
        )
    })?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|err| {
        format!(
            "Failed to read progress file '{}' from {}: {}",
            path.display(),
            state.offset,
            err
        )
    })?;
    state.offset = metadata.len();
    if bytes.is_empty() {
        return Ok(String::new());
    }

    let chunk = String::from_utf8_lossy(&bytes);
    let mut combined = std::mem::take(&mut state.carry);
    combined.push_str(&chunk);

    let (ready, carry) = if flush_partial {
        (combined.trim_end_matches('\n').to_string(), String::new())
    } else if let Some(last_newline) = combined.rfind('\n') {
        (
            combined[..last_newline].trim_end_matches('\n').to_string(),
            combined[last_newline + 1..].to_string(),
        )
    } else {
        (String::new(), combined)
    };

    state.carry = carry;
    Ok(ready)
}

fn refresh_last_prompt_path(prompt_dir: &Path) {
    if let Some(path) = latest_prompt_file(prompt_dir) {
        set_last_prompt_path(Some(path));
    }
}

fn set_last_prompt_path(path: Option<PathBuf>) {
    if let Ok(mut slot) = last_prompt_path_store().lock() {
        *slot = path;
    }
}

fn last_prompt_path_store() -> &'static Mutex<Option<PathBuf>> {
    LAST_PROMPT_PATH.get_or_init(|| Mutex::new(None))
}

fn roadmap_progress(roadmap_path: &Path) -> RoadmapProgress {
    let mut total_phases = 0usize;
    let mut pending_phases = 0usize;
    let mut next_phase = None;

    if let Ok(content) = fs::read_to_string(roadmap_path) {
        let mut in_active_roadmap = false;
        for line in content.lines() {
            let trimmed = line.trim_start();
            if trimmed == "## Active Roadmap" {
                in_active_roadmap = true;
                continue;
            }
            if trimmed.starts_with("## ") {
                in_active_roadmap = false;
            }
            if trimmed.starts_with("[ ] ")
                || trimmed.starts_with("[x] ")
                || trimmed.starts_with("[X] ")
            {
                total_phases += 1;
                if trimmed.starts_with("[ ] ") {
                    pending_phases += 1;
                    if next_phase.is_none() {
                        next_phase = Some(trimmed.trim_start_matches("[ ] ").trim().to_string());
                    }
                }
                continue;
            }

            if in_active_roadmap {
                if let Some(stripped) = trimmed
                    .split_once(". ")
                    .filter(|(prefix, _)| prefix.chars().all(|ch| ch.is_ascii_digit()))
                    .map(|(_, rest)| rest.trim())
                    .filter(|rest| !rest.is_empty())
                {
                    total_phases += 1;
                    pending_phases += 1;
                    if next_phase.is_none() {
                        next_phase = Some(stripped.to_string());
                    }
                }
            }
        }
    }

    RoadmapProgress {
        total_phases,
        pending_phases,
        next_phase,
    }
}

fn latest_prompt_path_from_result(result_dir: &Path) -> Option<PathBuf> {
    latest_result_file(result_dir).and_then(|path| prompt_path_from_result_file(&path))
}

fn prompt_path_from_result_file(result_path: &Path) -> Option<PathBuf> {
    let content = fs::read_to_string(result_path).ok()?;
    content.lines().find_map(|line| {
        let trimmed = line.trim();
        let prompt_path = trimmed.strip_prefix("- Prompt path: `")?;
        prompt_path
            .strip_suffix('`')
            .map(PathBuf::from)
            .or_else(|| Some(PathBuf::from(prompt_path)))
    })
}

fn bootstrap_prompt_path(paths: &DevelPaths) -> Option<PathBuf> {
    [
        paths.goal_path.as_path(),
        paths.repo_plan_path.as_path(),
        paths.roadmap_path.as_path(),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .map(PathBuf::from)
}

fn status_content_is_done(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.eq_ignore_ascii_case("done") {
        return true;
    }

    trimmed.lines().any(|line| {
        let lower = line.trim().to_ascii_lowercase();
        lower == "state: done" || lower == "status: done"
    })
}

fn status_is_done(status_path: &Path, legacy_status_path: &Path) -> bool {
    fs::read_to_string(status_path)
        .ok()
        .or_else(|| fs::read_to_string(legacy_status_path).ok())
        .map(|content| status_content_is_done(&content))
        .unwrap_or(false)
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

fn current_file_fingerprint(path: &Path) -> Option<String> {
    let metadata = path.metadata().ok()?;
    let modified = metadata.modified().ok()?;
    let elapsed = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(format!("{}-{}", metadata.len(), elapsed.as_nanos()))
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

#[cfg(test)]
mod tests {
    use super::{
        create_prompt_file, detect_repo_root, devel_status, latest_devel_result,
        parse_inotify_events, prompt_key_for_progress_file, prompt_key_for_result_file,
        read_progress_delta, roadmap_progress, sync_devel_tasks, DevelPaths, ProgressStreamState,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use tempfile::tempdir;

    struct EnvGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &std::path::Path) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(previous) = &self.previous {
                std::env::set_var(self.key, previous);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    struct CwdGuard {
        previous: std::path::PathBuf,
    }

    impl CwdGuard {
        fn set(path: &Path) -> Self {
            let previous = std::env::current_dir().unwrap();
            std::env::set_current_dir(path).unwrap();
            Self { previous }
        }
    }

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.previous);
        }
    }

    fn test_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn setup_repo() -> (
        MutexGuard<'static, ()>,
        tempfile::TempDir,
        EnvGuard,
        CwdGuard,
    ) {
        let env_lock = test_env_lock().lock().unwrap();
        let repo = tempdir().unwrap();
        fs::create_dir_all(repo.path().join(".dev")).unwrap();
        fs::write(repo.path().join(".dev/ROADMAP.md"), "- [ ] next\n").unwrap();
        let data_root = repo.path().join("runtime");
        fs::create_dir_all(&data_root).unwrap();
        let data_guard = EnvGuard::set("TIZENCLAW_DATA_DIR", &data_root);
        let cwd_guard = CwdGuard::set(repo.path());
        (env_lock, repo, data_guard, cwd_guard)
    }

    #[test]
    fn detect_repo_root_finds_dev_boundary() {
        let (_env_lock, repo, _data_guard, _cwd_guard) = setup_repo();
        let nested = repo.path().join("src/nested");
        fs::create_dir_all(&nested).unwrap();
        assert_eq!(detect_repo_root(&nested), Some(repo.path().to_path_buf()));
    }

    #[test]
    fn sync_devel_tasks_creates_prompt_and_result_dirs() {
        let (_env_lock, repo, _data_guard, _cwd_guard) = setup_repo();
        let paths = DevelPaths::new(repo.path());
        let result = sync_devel_tasks(Path::new("unused"), repo.path()).unwrap();

        assert!(result.task_enabled);
        assert!(paths.prompt_dir.is_dir());
        assert!(paths.progress_dir.is_dir());
        assert!(paths.result_dir.is_dir());
        assert_eq!(result.task_path, paths.prompt_dir);
        assert_eq!(result.bootstrap_task_path, paths.result_dir);
    }

    #[test]
    fn create_prompt_file_writes_timestamped_markdown_file() {
        let (_env_lock, repo, _data_guard, _cwd_guard) = setup_repo();
        let path = create_prompt_file("implement prompt bridge").unwrap();

        assert!(path.starts_with(repo.path().join("runtime/devel/prompt")));
        assert!(path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .ends_with("_prompt.md"));
        let content = fs::read_to_string(path).unwrap();
        assert_eq!(content, "implement prompt bridge\n");
    }

    #[test]
    fn devel_status_reports_latest_prompt_and_watcher_flag() {
        let (_env_lock, repo, _data_guard, _cwd_guard) = setup_repo();
        let created = create_prompt_file("check status").unwrap();

        let status = devel_status(Path::new("unused"), repo.path());

        assert!(status.prompt_exists);
        assert_eq!(status.last_prompt_path, Some(created));
        assert!(status.prompt_dir.ends_with("devel/prompt"));
        assert!(status.progress_dir.ends_with("devel/progress"));
        assert!(status.result_dir.ends_with("devel/result"));
        assert!(!status.progress_watcher_active);
        assert!(!status.result_watcher_active);
    }

    #[test]
    fn devel_status_bootstraps_prompt_and_task_from_repo_metadata() {
        let (_env_lock, repo, _data_guard, _cwd_guard) = setup_repo();
        fs::write(
            repo.path().join(".dev/GOAL.md"),
            "Ship the next devel slice.\n",
        )
        .unwrap();
        fs::write(
            repo.path().join(".dev/ROADMAP.md"),
            "# ROADMAP\n\n## Active Roadmap\n1. Inspect scheduler state\n2. Run tests\n",
        )
        .unwrap();
        fs::write(
            repo.path().join(".dev/devel-autonomous-cycle.md"),
            "Follow the devel autonomous cycle.\n",
        )
        .unwrap();

        let status = devel_status(Path::new("unused"), repo.path());

        assert!(status.prompt_exists);
        assert_eq!(
            status.last_prompt_path,
            Some(repo.path().join(".dev/GOAL.md"))
        );
        assert!(status.recurring_task_present);
        assert!(status.task_path.ends_with("devel-autonomous-cycle.md"));
        assert_eq!(
            status.status_next_phase,
            Some("Inspect scheduler state".to_string())
        );
        assert!(status.roadmap_has_pending_work);
        assert!(status.development_required);
    }

    #[test]
    fn latest_devel_result_returns_newest_file_content() {
        let (_env_lock, repo, _data_guard, _cwd_guard) = setup_repo();
        let result_dir = repo.path().join("runtime/devel/result");
        let prompt_dir = repo.path().join("runtime/devel/prompt");
        fs::create_dir_all(&result_dir).unwrap();
        fs::create_dir_all(&prompt_dir).unwrap();
        let prompt = prompt_dir.join("02_prompt.md");
        let older = result_dir.join("01_prompt_RESULT.md");
        let newer = result_dir.join("02_prompt_RESULT.md");
        fs::write(&prompt, "queued\n").unwrap();
        fs::write(&older, "older\n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        fs::write(&newer, "latest\n").unwrap();

        let result = latest_devel_result(repo.path());

        assert!(result.available);
        assert_eq!(result.latest_result_path, Some(newer));
        assert_eq!(result.latest_prompt_path, Some(prompt));
        assert_eq!(
            result.latest_prompt_result_path,
            Some(result_dir.join("02_prompt_RESULT.md"))
        );
        assert!(result.latest_prompt_result_available);
        assert!(result.latest_result_matches_latest_prompt);
        assert_eq!(result.content, "latest\n");
        assert!(result.result_dir.ends_with("devel/result"));
    }

    #[test]
    fn latest_devel_result_reports_pending_prompt_when_result_is_stale() {
        let (_env_lock, repo, _data_guard, _cwd_guard) = setup_repo();
        let prompt_dir = repo.path().join("runtime/devel/prompt");
        let result_dir = repo.path().join("runtime/devel/result");
        fs::create_dir_all(&prompt_dir).unwrap();
        fs::create_dir_all(&result_dir).unwrap();

        let completed_prompt = prompt_dir.join("02_prompt.md");
        let pending_prompt = prompt_dir.join("20260411104959_prompt.md");
        let completed_result = result_dir.join("02_prompt_RESULT.md");
        fs::write(&completed_prompt, "older\n").unwrap();
        fs::write(&completed_result, "completed\n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        fs::write(&pending_prompt, "pending\n").unwrap();

        let result = latest_devel_result(repo.path());

        assert!(result.available);
        assert_eq!(result.latest_result_path, Some(completed_result));
        assert_eq!(result.latest_prompt_path, Some(pending_prompt.clone()));
        assert_eq!(
            result.latest_prompt_result_path,
            Some(result_dir.join("20260411104959_prompt_RESULT.md"))
        );
        assert!(!result.latest_prompt_result_available);
        assert!(!result.latest_result_matches_latest_prompt);
        assert_eq!(result.content, "completed\n");
    }

    #[test]
    fn latest_devel_result_recovers_prompt_path_from_result_body() {
        let (_env_lock, repo, _data_guard, _cwd_guard) = setup_repo();
        let result_dir = repo.path().join("runtime/devel/result");
        fs::create_dir_all(&result_dir).unwrap();
        let result = result_dir.join("44_prompt_RESULT.md");
        fs::write(
            &result,
            "# Result\n- Prompt path: `/tmp/runtime/devel/prompt/44_prompt.md`\n",
        )
        .unwrap();

        let latest = latest_devel_result(repo.path());

        assert_eq!(
            latest.latest_prompt_path,
            Some(PathBuf::from("/tmp/runtime/devel/prompt/44_prompt.md"))
        );
        assert_eq!(
            latest.latest_prompt_result_path,
            Some(result_dir.join("44_prompt_RESULT.md"))
        );
        assert!(latest.latest_prompt_result_available);
        assert!(latest.latest_result_matches_latest_prompt);
    }

    #[test]
    fn roadmap_progress_detects_numbered_active_roadmap_items() {
        let (_env_lock, repo, _data_guard, _cwd_guard) = setup_repo();
        fs::write(
            repo.path().join(".dev/ROADMAP.md"),
            "# ROADMAP\n\n## Active Roadmap\n1. Fix prompt bootstrap\n2. Re-run tests\n",
        )
        .unwrap();

        let progress = roadmap_progress(&repo.path().join(".dev/ROADMAP.md"));

        assert_eq!(progress.total_phases, 2);
        assert_eq!(progress.pending_phases, 2);
        assert_eq!(
            progress.next_phase,
            Some("Fix prompt bootstrap".to_string())
        );
    }

    #[test]
    fn parse_inotify_events_extracts_name() {
        let header_len = std::mem::size_of::<libc::inotify_event>();
        let mut bytes = vec![0u8; header_len + 10];
        let event_ptr = bytes.as_mut_ptr() as *mut libc::inotify_event;
        unsafe {
            (*event_ptr).wd = 17;
            (*event_ptr).mask = libc::IN_CLOSE_WRITE;
            (*event_ptr).len = 10;
        }
        bytes[header_len..header_len + 10].copy_from_slice(b"done.md\0\0\0");

        let events = parse_inotify_events(&bytes);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].watch_descriptor, 17);
        assert_eq!(events[0].name.as_deref(), Some("done.md"));
    }

    #[test]
    fn prompt_key_helpers_match_progress_and_result_files() {
        assert_eq!(
            prompt_key_for_progress_file(Path::new("20260411_prompt_progress.log")),
            Some("20260411_prompt".to_string())
        );
        assert_eq!(
            prompt_key_for_result_file(Path::new("20260411_prompt_RESULT.md")),
            Some("20260411_prompt".to_string())
        );
        assert_eq!(prompt_key_for_progress_file(Path::new("notes.log")), None);
    }

    #[test]
    fn read_progress_delta_buffers_partial_lines_until_flush() {
        let (_env_lock, repo, _data_guard, _cwd_guard) = setup_repo();
        let progress_path = repo
            .path()
            .join("runtime/devel/progress")
            .join("20260411_prompt_progress.log");
        fs::create_dir_all(progress_path.parent().unwrap()).unwrap();

        let mut state = ProgressStreamState::default();
        fs::write(&progress_path, "line one\nline two").unwrap();
        let first = read_progress_delta(&progress_path, &mut state, false).unwrap();
        assert_eq!(first, "line one");
        assert_eq!(state.carry, "line two");

        fs::write(&progress_path, "line one\nline two\nline three\n").unwrap();
        let second = read_progress_delta(&progress_path, &mut state, false).unwrap();
        assert_eq!(second, "line two\nline three");
        assert!(state.carry.is_empty());

        fs::write(&progress_path, "line one\nline two\nline three\nline four").unwrap();
        let third = read_progress_delta(&progress_path, &mut state, true).unwrap();
        assert_eq!(third, "line four");
        assert!(state.carry.is_empty());
    }
}
