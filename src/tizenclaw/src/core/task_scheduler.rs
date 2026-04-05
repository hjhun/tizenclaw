//! Task scheduler — manages recurring and one-shot scheduled agent tasks.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A scheduled task definition.
#[derive(Clone, Debug)]
pub struct ScheduledTask {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub session_id: String,
    pub interval_secs: u64,
    pub schedule_expr: Option<String>,
    pub one_shot: bool,
    pub enabled: bool,
}

pub struct TaskScheduler {
    running: Arc<AtomicBool>,
    tasks: Arc<std::sync::Mutex<Vec<ScheduledTask>>>,
    task_dir: Arc<std::sync::Mutex<Option<std::path::PathBuf>>>,
}

impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskScheduler {
    pub fn new() -> Self {
        TaskScheduler {
            running: Arc::new(AtomicBool::new(false)),
            tasks: Arc::new(std::sync::Mutex::new(vec![])),
            task_dir: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    pub fn add_task(&self, task: ScheduledTask) {
        if let Ok(mut tasks) = self.tasks.lock() {
            log::debug!(
                "Scheduler: added task '{}' (interval={}s)",
                task.name,
                task.interval_secs
            );
            tasks.push(task);
        }
    }

    pub fn remove_task(&self, task_id: &str) {
        if let Ok(mut tasks) = self.tasks.lock() {
            tasks.retain(|t| t.id != task_id);
        }
    }

    pub fn load_config(&self, dir_path: &str) {
        if let Ok(mut task_dir) = self.task_dir.lock() {
            *task_dir = Some(Path::new(dir_path).to_path_buf());
        }
        self.refresh_from_disk();
    }

    pub fn refresh_from_disk(&self) {
        let dir_path = self
            .task_dir
            .lock()
            .ok()
            .and_then(|dir| dir.clone());
        let Some(dir_path) = dir_path else {
            return;
        };

        let mut loaded = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&dir_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(task) = Self::parse_task_file(&path) {
                    loaded.push(task);
                }
            }
        }

        loaded.sort_by(|left, right| left.id.cmp(&right.id));
        if let Ok(mut tasks) = self.tasks.lock() {
            *tasks = loaded;
        }
    }

    pub fn list_tasks_from_dir(dir_path: &Path) -> Result<Vec<ScheduledTask>, String> {
        let mut tasks = Vec::new();
        if !dir_path.exists() {
            return Ok(tasks);
        }
        let entries = std::fs::read_dir(dir_path)
            .map_err(|e| format!("Failed to read task directory '{}': {}", dir_path.display(), e))?;

        for entry in entries.flatten() {
            if let Some(task) = Self::parse_task_file(&entry.path()) {
                tasks.push(task);
            }
        }

        tasks.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(tasks)
    }

    pub fn create_task_file(
        dir_path: &Path,
        schedule: &str,
        prompt: &str,
    ) -> Result<ScheduledTask, String> {
        let schedule = schedule.trim();
        let prompt = prompt.trim();
        if schedule.is_empty() {
            return Err("Schedule must not be empty".into());
        }
        if prompt.is_empty() {
            return Err("Prompt must not be empty".into());
        }

        std::fs::create_dir_all(dir_path).map_err(|e| {
            format!(
                "Failed to create task directory '{}': {}",
                dir_path.display(),
                e
            )
        })?;

        let (interval_secs, one_shot) = Self::parse_schedule_expr(schedule)?;
        let slug = slugify_task_name(prompt);
        let timestamp = current_timestamp_compact();
        let id = format!("{}-{}", timestamp, slug);
        let file_path = dir_path.join(format!("{}.md", id));
        let name = first_prompt_line(prompt);
        let session_id = format!("scheduler_{}", slug);
        let content = format!(
            "---\nname: {}\nschedule: {}\ninterval_secs: {}\none_shot: {}\nenabled: true\nsession_id: {}\n---\n{}\n",
            name,
            schedule,
            interval_secs,
            if one_shot { "true" } else { "false" },
            session_id,
            prompt
        );

        std::fs::write(&file_path, content).map_err(|e| {
            format!(
                "Failed to write task file '{}': {}",
                file_path.display(),
                e
            )
        })?;

        Ok(ScheduledTask {
            id,
            name,
            prompt: prompt.to_string(),
            session_id,
            interval_secs,
            schedule_expr: Some(schedule.to_string()),
            one_shot,
            enabled: true,
        })
    }

    pub fn delete_task_file(dir_path: &Path, task_id: &str) -> Result<bool, String> {
        let task_id = task_id.trim();
        if task_id.is_empty() || task_id.contains('/') || task_id.contains("..") {
            return Err("Invalid task id".into());
        }

        let file_path = dir_path.join(format!("{}.md", task_id));
        match std::fs::remove_file(&file_path) {
            Ok(_) => Ok(true),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(err) => Err(format!(
                "Failed to delete task file '{}': {}",
                file_path.display(),
                err
            )),
        }
    }

    pub fn seed_default_tasks_if_empty(&self, dir_path: &str) -> usize {
        let dir = Path::new(dir_path);
        let _ = std::fs::create_dir_all(dir);

        let has_existing_tasks = std::fs::read_dir(dir)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(|entry| entry.ok()))
            .any(|entry| {
                entry
                    .path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext == "md")
                    .unwrap_or(false)
            });

        if has_existing_tasks {
            return 0;
        }

        let today = local_date_string();
        let defaults = [
            (
                format!("{}-daily-health-check.md", today),
                format!(
                    "---\nname: Daily health check\ninterval_secs: 3600\none_shot: false\nenabled: true\nsession_id: scheduler_health\n---\nCollect a short health summary for CPU, memory, and service status.\n"
                ),
            ),
            (
                format!("{}-memory-watch.md", today),
                format!(
                    "---\nname: Memory watch\ninterval_secs: 1800\none_shot: false\nenabled: true\nsession_id: scheduler_memory\n---\nCheck memory pressure and report if the daemon footprint grows unusually.\n"
                ),
            ),
            (
                format!("{}-log-rollup.md", today),
                format!(
                    "---\nname: Log rollup\ninterval_secs: 7200\none_shot: false\nenabled: true\nsession_id: scheduler_logs\n---\nReview recent runtime logs and prepare a concise operator summary.\n"
                ),
            ),
        ];

        let mut created = 0usize;
        for (file_name, content) in defaults {
            let path = dir.join(file_name);
            if std::fs::write(&path, content).is_ok() {
                created += 1;
            }
        }
        created
    }

    pub fn start(&self) -> Option<tokio::task::JoinHandle<()>> {
        if self.running.load(Ordering::SeqCst) {
            return None;
        }
        self.running.store(true, Ordering::SeqCst);

        let running = self.running.clone();
        let tasks = self.tasks.clone();
        let task_dir = self.task_dir.clone();

        let handle = tokio::spawn(async move {
            log::info!("TaskScheduler started");
            let mut next_run_at: std::collections::HashMap<String, std::time::SystemTime> =
                std::collections::HashMap::new();

            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));

            while running.load(Ordering::SeqCst) {
                interval.tick().await;

                let task_dir_path = task_dir.lock().ok().and_then(|dir| dir.clone());
                if let Some(dir_path) = task_dir_path {
                    let mut loaded = Vec::new();
                    if let Ok(entries) = std::fs::read_dir(&dir_path) {
                        for entry in entries.flatten() {
                            if let Some(task) = TaskScheduler::parse_task_file(&entry.path()) {
                                loaded.push(task);
                            }
                        }
                    }
                    loaded.sort_by(|left, right| left.id.cmp(&right.id));
                    if let Ok(mut shared_tasks) = tasks.lock() {
                        *shared_tasks = loaded;
                    }
                }

                let task_list = match tasks.lock() {
                    Ok(t) => t.clone(),
                    Err(_) => continue,
                };
                let active_ids: std::collections::HashSet<String> =
                    task_list.iter().map(|task| task.id.clone()).collect();
                next_run_at.retain(|task_id, _| active_ids.contains(task_id));

                let now = std::time::SystemTime::now();
                for task in &task_list {
                    if !task.enabled {
                        continue;
                    }

                    let due_at = next_run_at
                        .entry(task.id.clone())
                        .or_insert_with(|| task.initial_due_time());
                    let should_run = now.duration_since(*due_at).is_ok();

                    if should_run {
                        log::debug!("Scheduler: executing task '{}'", task.name);

                        if task.one_shot {
                            if let Some(dir_path) = task_dir.lock().ok().and_then(|dir| dir.clone())
                            {
                                let _ = TaskScheduler::delete_task_file(&dir_path, &task.id);
                            }
                            if let Ok(mut ts) = tasks.lock() {
                                ts.retain(|t| t.id != task.id);
                            }
                            next_run_at.remove(&task.id);
                        } else {
                            *due_at = task.next_due_time_from(now);
                        }
                    }
                }
            }
            log::info!("TaskScheduler stopped");
        });

        Some(handle)
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

fn local_date_string() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as libc::time_t;
    let mut tm_buf: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        libc::localtime_r(&now, &mut tm_buf);
    }

    format!(
        "{:04}-{:02}-{:02}",
        tm_buf.tm_year + 1900,
        tm_buf.tm_mon + 1,
        tm_buf.tm_mday
    )
}

impl ScheduledTask {
    fn initial_due_time(&self) -> std::time::SystemTime {
        if let Some(schedule) = self.schedule_expr.as_deref() {
            if let Some(system_time) = first_due_time_for_schedule(schedule) {
                return system_time;
            }
        }
        std::time::SystemTime::now()
    }

    fn next_due_time_from(&self, after: std::time::SystemTime) -> std::time::SystemTime {
        if let Some(schedule) = self.schedule_expr.as_deref() {
            if let Some(system_time) = next_due_time_for_schedule(schedule, after) {
                return system_time;
            }
        }
        after + std::time::Duration::from_secs(self.interval_secs.max(1))
    }
}

impl TaskScheduler {
    fn parse_task_file(path: &Path) -> Option<ScheduledTask> {
        if !path.is_file() {
            return None;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "md" {
            return None;
        }

        let content = std::fs::read_to_string(path).ok()?;
        let id = path.file_stem()?.to_string_lossy().to_string();
        let mut name = id.clone();
        let mut interval_secs = 3600;
        let mut one_shot = false;
        let mut enabled = true;
        let mut session_id = "scheduler".to_string();
        let mut schedule_expr = None;
        let mut prompt = String::new();
        let mut in_frontmatter = false;

        for line in content.lines() {
            let text = line.trim();
            if text == "---" {
                in_frontmatter = !in_frontmatter;
                continue;
            }
            if in_frontmatter {
                if let Some((k, v)) = text.split_once(':') {
                    let val = v.trim().trim_matches(|c| c == '\'' || c == '"');
                    match k.trim() {
                        "interval" | "interval_secs" => {
                            interval_secs = val.parse().unwrap_or(3600)
                        }
                        "schedule" => schedule_expr = Some(val.to_string()),
                        "one_shot" => one_shot = val == "true",
                        "enabled" => enabled = val != "false",
                        "name" => name = val.to_string(),
                        "session_id" => session_id = val.to_string(),
                        _ => {}
                    }
                }
            } else if !text.is_empty() {
                prompt.push_str(text);
                prompt.push('\n');
            }
        }

        Some(ScheduledTask {
            id,
            name,
            prompt: prompt.trim().to_string(),
            session_id,
            interval_secs,
            schedule_expr,
            one_shot,
            enabled,
        })
    }

    fn parse_schedule_expr(schedule: &str) -> Result<(u64, bool), String> {
        let trimmed = schedule.trim();
        let lower = trimmed.to_ascii_lowercase();

        if let Some(rest) = lower.strip_prefix("interval ") {
            let secs = parse_duration_secs(rest)
                .ok_or_else(|| format!("Unsupported interval schedule '{}'", schedule))?;
            return Ok((secs.max(1), false));
        }
        if let Some(rest) = lower.strip_prefix("daily ") {
            if parse_hhmm(rest).is_some() {
                return Ok((24 * 60 * 60, false));
            }
            return Err(format!("Unsupported daily schedule '{}'", schedule));
        }
        if let Some(rest) = lower.strip_prefix("weekly ") {
            let mut parts = rest.split_whitespace();
            let weekday = parts.next().unwrap_or("");
            let hhmm = parts.next().unwrap_or("");
            if weekday_index(weekday).is_some() && parse_hhmm(hhmm).is_some() {
                return Ok((7 * 24 * 60 * 60, false));
            }
            return Err(format!("Unsupported weekly schedule '{}'", schedule));
        }
        if let Some(rest) = trimmed.strip_prefix("once ") {
            if parse_once_timestamp(rest).is_some() {
                return Ok((1, true));
            }
            return Err(format!("Unsupported one-shot schedule '{}'", schedule));
        }

        Err(format!("Unsupported schedule '{}'", schedule))
    }
}

fn first_prompt_line(prompt: &str) -> String {
    prompt
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| truncate_chars(line.trim(), 48))
        .filter(|line| !line.is_empty())
        .unwrap_or_else(|| "Scheduled task".to_string())
}

fn slugify_task_name(prompt: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in first_prompt_line(prompt).chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            prev_dash = false;
            ch.to_ascii_lowercase()
        } else {
            if prev_dash {
                continue;
            }
            prev_dash = true;
            '-'
        };
        out.push(mapped);
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "task".to_string()
    } else {
        trimmed
    }
}

fn current_timestamp_compact() -> String {
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

fn parse_duration_secs(raw: &str) -> Option<u64> {
    let value = raw.trim();
    if value.len() < 2 {
        return None;
    }
    let unit = value.chars().last()?;
    let amount = value[..value.len() - 1].trim().parse::<u64>().ok()?;
    match unit {
        's' => Some(amount),
        'm' => Some(amount * 60),
        'h' => Some(amount * 60 * 60),
        'd' => Some(amount * 24 * 60 * 60),
        _ => None,
    }
}

fn parse_hhmm(raw: &str) -> Option<(i32, i32)> {
    let mut parts = raw.trim().split(':');
    let hour = parts.next()?.parse::<i32>().ok()?;
    let minute = parts.next()?.parse::<i32>().ok()?;
    if parts.next().is_some() || !(0..=23).contains(&hour) || !(0..=59).contains(&minute) {
        return None;
    }
    Some((hour, minute))
}

fn weekday_index(raw: &str) -> Option<i32> {
    match raw.trim() {
        "sun" => Some(0),
        "mon" => Some(1),
        "tue" => Some(2),
        "wed" => Some(3),
        "thu" => Some(4),
        "fri" => Some(5),
        "sat" => Some(6),
        _ => None,
    }
}

fn parse_once_timestamp(raw: &str) -> Option<std::time::SystemTime> {
    let mut parts = raw.split_whitespace();
    let date = parts.next()?;
    let time = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    let mut date_parts = date.split('-');
    let year = date_parts.next()?.parse::<i32>().ok()?;
    let month = date_parts.next()?.parse::<i32>().ok()?;
    let day = date_parts.next()?.parse::<i32>().ok()?;
    if date_parts.next().is_some() {
        return None;
    }
    let (hour, minute) = parse_hhmm(time)?;

    system_time_from_local_parts(year, month, day, hour, minute)
}

fn first_due_time_for_schedule(schedule: &str) -> Option<std::time::SystemTime> {
    let now = std::time::SystemTime::now();
    next_due_time_for_schedule(schedule, now)
}

fn next_due_time_for_schedule(
    schedule: &str,
    after: std::time::SystemTime,
) -> Option<std::time::SystemTime> {
    let trimmed = schedule.trim();
    let lower = trimmed.to_ascii_lowercase();

    if let Some(rest) = lower.strip_prefix("interval ") {
        let secs = parse_duration_secs(rest)?.max(1);
        return Some(after + std::time::Duration::from_secs(secs));
    }
    if let Some(rest) = lower.strip_prefix("daily ") {
        let (hour, minute) = parse_hhmm(rest)?;
        return next_daily_due(after, hour, minute);
    }
    if let Some(rest) = lower.strip_prefix("weekly ") {
        let mut parts = rest.split_whitespace();
        let weekday = weekday_index(parts.next()?)?;
        let (hour, minute) = parse_hhmm(parts.next()?)?;
        return next_weekly_due(after, weekday, hour, minute);
    }
    if let Some(rest) = trimmed.strip_prefix("once ") {
        let target = parse_once_timestamp(rest)?;
        return Some(if target.duration_since(after).is_ok() {
            target
        } else {
            after
        });
    }

    None
}

fn next_daily_due(
    after: std::time::SystemTime,
    hour: i32,
    minute: i32,
) -> Option<std::time::SystemTime> {
    let after_epoch = after
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as libc::time_t;
    let mut tm_buf: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        libc::localtime_r(&after_epoch, &mut tm_buf);
    }
    tm_buf.tm_hour = hour;
    tm_buf.tm_min = minute;
    tm_buf.tm_sec = 0;
    let candidate = system_time_from_tm(tm_buf)?;
    if candidate.duration_since(after).is_ok() {
        Some(candidate)
    } else {
        tm_buf.tm_mday += 1;
        system_time_from_tm(tm_buf)
    }
}

fn next_weekly_due(
    after: std::time::SystemTime,
    target_wday: i32,
    hour: i32,
    minute: i32,
) -> Option<std::time::SystemTime> {
    let after_epoch = after
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs() as libc::time_t;
    let mut tm_buf: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        libc::localtime_r(&after_epoch, &mut tm_buf);
    }
    let days_ahead = (target_wday - tm_buf.tm_wday + 7) % 7;
    tm_buf.tm_mday += days_ahead;
    tm_buf.tm_hour = hour;
    tm_buf.tm_min = minute;
    tm_buf.tm_sec = 0;
    let candidate = system_time_from_tm(tm_buf)?;
    if candidate.duration_since(after).is_ok() && days_ahead > 0 {
        Some(candidate)
    } else if candidate.duration_since(after).is_ok() {
        Some(candidate)
    } else {
        tm_buf.tm_mday += 7;
        system_time_from_tm(tm_buf)
    }
}

fn system_time_from_local_parts(
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    minute: i32,
) -> Option<std::time::SystemTime> {
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_sec = 0;
    tm.tm_min = minute;
    tm.tm_hour = hour;
    tm.tm_mday = day;
    tm.tm_mon = month - 1;
    tm.tm_year = year - 1900;
    tm.tm_isdst = -1;
    system_time_from_tm(tm)
}

fn system_time_from_tm(mut tm: libc::tm) -> Option<std::time::SystemTime> {
    let epoch = unsafe { libc::mktime(&mut tm) };
    if epoch < 0 {
        None
    } else {
        Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(epoch as u64))
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (idx, ch) in text.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_task(id: &str) -> ScheduledTask {
        ScheduledTask {
            id: id.into(),
            name: format!("Task {}", id),
            prompt: "Check status".into(),
            session_id: "sched".into(),
            interval_secs: 60,
            schedule_expr: None,
            one_shot: false,
            enabled: true,
        }
    }

    #[test]
    fn test_add_task() {
        let scheduler = TaskScheduler::new();
        scheduler.add_task(sample_task("t1"));
        let tasks = scheduler.tasks.lock().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "t1");
    }

    #[test]
    fn test_remove_task() {
        let scheduler = TaskScheduler::new();
        scheduler.add_task(sample_task("t1"));
        scheduler.add_task(sample_task("t2"));
        scheduler.remove_task("t1");
        let tasks = scheduler.tasks.lock().unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "t2");
    }

    #[test]
    fn test_remove_nonexistent_task() {
        let scheduler = TaskScheduler::new();
        scheduler.add_task(sample_task("t1"));
        scheduler.remove_task("nonexistent");
        let tasks = scheduler.tasks.lock().unwrap();
        assert_eq!(tasks.len(), 1);
    }

    #[test]
    fn test_empty_scheduler() {
        let scheduler = TaskScheduler::new();
        let tasks = scheduler.tasks.lock().unwrap();
        assert!(tasks.is_empty());
    }

    #[test]
    fn test_task_fields() {
        let t = sample_task("t1");
        assert_eq!(t.interval_secs, 60);
        assert!(!t.one_shot);
        assert!(t.enabled);
    }

    #[test]
    fn test_parse_interval_schedule_expression() {
        let parsed = TaskScheduler::parse_schedule_expr("interval 15m").unwrap();
        assert_eq!(parsed, (900, false));
    }

    #[test]
    fn test_create_and_delete_task_file_round_trip() {
        let temp_root = std::env::temp_dir().join(format!(
            "tizenclaw-task-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&temp_root);
        std::fs::create_dir_all(&temp_root).unwrap();

        let task = TaskScheduler::create_task_file(
            &temp_root,
            "interval 30m",
            "Collect a short health summary.",
        )
        .unwrap();
        let listed = TaskScheduler::list_tasks_from_dir(&temp_root).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, task.id);
        assert!(TaskScheduler::delete_task_file(&temp_root, &task.id).unwrap());

        let _ = std::fs::remove_dir_all(&temp_root);
    }
}
