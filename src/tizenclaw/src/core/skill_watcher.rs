//! Skill watcher — watches filesystem for SKILL.md changes via polling.
//!
//! Monitors the skills directory for add/remove/modify events
//! and invokes a callback when changes are detected.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Default skill installation directory.
const SKILLS_DIR: &str = "/opt/usr/share/tizen-tools/skills";

/// Polling interval for filesystem changes.
const POLL_INTERVAL_SECS: u64 = 5;

pub struct SkillWatcher {
    running: Arc<AtomicBool>,
    watch_dirs: Vec<String>,
    on_change: Option<Box<dyn Fn() + Send + Sync>>,
}

impl SkillWatcher {
    pub fn new() -> Self {
        SkillWatcher {
            running: Arc::new(AtomicBool::new(false)),
            watch_dirs: vec![SKILLS_DIR.into()],
            on_change: None,
        }
    }

    pub fn set_change_callback(&mut self, cb: impl Fn() + Send + Sync + 'static) {
        self.on_change = Some(Box::new(cb));
    }

    pub fn start(&self) -> Option<std::thread::JoinHandle<()>> {
        if self.running.load(Ordering::SeqCst) {
            return None;
        }
        self.running.store(true, Ordering::SeqCst);

        let running = self.running.clone();
        let dirs = self.watch_dirs.clone();

        let handle = std::thread::spawn(move || {
            log::info!("SkillWatcher started, monitoring: {:?}", dirs);
            let mut mtimes = collect_mtimes(&dirs);

            while running.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS));

                let new_mtimes = collect_mtimes(&dirs);
                if new_mtimes != mtimes {
                    log::info!("SkillWatcher: change detected in skills directory");
                    mtimes = new_mtimes;
                    // Note: on_change callback invocation would be wired
                    // through AgentCore to trigger skill reload
                }
            }
            log::info!("SkillWatcher stopped");
        });

        Some(handle)
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

/// Collect modification times for all SKILL.md files in the watched directories.
fn collect_mtimes(dirs: &[String]) -> HashMap<String, u64> {
    let mut mtimes = HashMap::new();
    for dir in dirs {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            // Track SKILL.md mtime specifically
            let skill_md = path.join("SKILL.md");
            if let Ok(meta) = skill_md.metadata() {
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                mtimes.insert(skill_md.to_string_lossy().to_string(), mtime);
            }
        }
    }
    mtimes
}
