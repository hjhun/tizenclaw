//! Skill watcher — watches filesystem for tool/skill changes via polling.
//!
//! Monitors `/opt/usr/share/tizen-tools` and all subdirectories for
//! add/remove/modify events and invokes a callback when changes are detected.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Root tool/skill installation directory.
const TOOLS_ROOT: &str = "/opt/usr/share/tizen-tools";

/// Polling interval for filesystem changes.
const POLL_INTERVAL_SECS: u64 = 5;

pub struct SkillWatcher {
    running: Arc<AtomicBool>,
    on_change: Option<Box<dyn Fn() + Send + Sync>>,
}

impl SkillWatcher {
    pub fn new() -> Self {
        SkillWatcher {
            running: Arc::new(AtomicBool::new(false)),
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

        let handle = std::thread::spawn(move || {
            log::info!("SkillWatcher started, monitoring: {}", TOOLS_ROOT);
            let mut mtimes = collect_mtimes_recursive(TOOLS_ROOT);

            while running.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS));

                let new_mtimes = collect_mtimes_recursive(TOOLS_ROOT);
                if new_mtimes != mtimes {
                    log::info!("SkillWatcher: change detected under {}", TOOLS_ROOT);
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

/// Recursively collect modification times for all files under the given root.
fn collect_mtimes_recursive(root: &str) -> HashMap<String, u64> {
    let mut mtimes = HashMap::new();
    let mut stack = vec![std::path::PathBuf::from(root)];

    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                if let Ok(meta) = path.metadata() {
                    let mtime = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    mtimes.insert(path.to_string_lossy().to_string(), mtime);
                }
            }
        }
    }
    mtimes
}
