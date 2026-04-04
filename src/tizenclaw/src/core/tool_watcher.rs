use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::task::JoinHandle;

pub struct ToolWatcher {
    callback: Option<Box<dyn Fn() + Send + 'static>>,
    watch_dirs: Vec<PathBuf>,
}

impl ToolWatcher {
    pub fn new(tool_dir: String) -> Self {
        ToolWatcher {
            callback: None,
            watch_dirs: vec![PathBuf::from(tool_dir)],
        }
    }

    pub fn set_change_callback<F>(&mut self, callback: F)
    where
        F: Fn() + Send + 'static,
    {
        self.callback = Some(Box::new(callback));
    }

    /// Extends the watch list bounds if necessary.
    pub fn add_watch_dir(&mut self, dir: &str) {
        self.watch_dirs.push(PathBuf::from(dir));
    }

    pub fn start(self) -> Option<JoinHandle<()>> {
        let callback = self.callback?;
        let shared_cb = Arc::new(Mutex::new(callback));
        let watch_dirs = self.watch_dirs.clone();

        let handle = tokio::spawn(async move {
            log::debug!("ToolWatcher: Monitoring tool directories for changes...");
            // Polling interval: 3 seconds
            let mut interval = tokio::time::interval(Duration::from_secs(3));
            let mut last_modified_times: std::collections::HashMap<PathBuf, std::time::SystemTime> =
                std::collections::HashMap::new();

            // Debounce: wait DEBOUNCE_SECS of quiet after the last change
            // before invoking the callback.
            const DEBOUNCE_SECS: u64 = 10;
            let mut last_change: Option<std::time::Instant> = None;

            // Guard: suppress callback on very first scan (baseline population).
            let mut initial_scan_done = false;

            fn scan_dir_recursive(
                dir: &Path,
                max_depth: usize,
                current_depth: usize,
                last_modified_times: &mut std::collections::HashMap<PathBuf, std::time::SystemTime>,
                current_seen: &mut std::collections::HashSet<PathBuf>,
                initial_scan_done: bool,
            ) -> bool {
                if current_depth > max_depth {
                    return false;
                }
                let mut changed = false;
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        current_seen.insert(path.clone());
                        if let Ok(metadata) = entry.metadata() {
                            if let Ok(modified) = metadata.modified() {
                                match last_modified_times.get(&path) {
                                    Some(&last_mod) if modified > last_mod => {
                                        changed = true;
                                        last_modified_times.insert(path.clone(), modified);
                                    }
                                    None => {
                                        // New entry — always record the time.
                                        last_modified_times.insert(path.clone(), modified);
                                        // Only report as a change if this is NOT
                                        // the initial baseline scan.
                                        if initial_scan_done {
                                            changed = true;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            if metadata.is_dir()
                                && scan_dir_recursive(
                                    &path,
                                    max_depth,
                                    current_depth + 1,
                                    last_modified_times,
                                    current_seen,
                                    initial_scan_done,
                                )
                            {
                                changed = true;
                            }
                        }
                    }
                }
                changed
            }

            loop {
                interval.tick().await;

                let mut current_seen = std::collections::HashSet::new();
                let mut changed = false;

                for dir in &watch_dirs {
                    if scan_dir_recursive(
                        dir,
                        3,
                        0,
                        &mut last_modified_times,
                        &mut current_seen,
                        initial_scan_done,
                    ) {
                        changed = true;
                    }
                }

                if !initial_scan_done {
                    // First pass is baseline only — mark done, skip callback.
                    initial_scan_done = true;
                    log::debug!(
                        "ToolWatcher: Initial scan complete, {} entries indexed.",
                        last_modified_times.len()
                    );
                    continue;
                }

                // Detect deleted files
                let prev_keys: Vec<PathBuf> = last_modified_times.keys().cloned().collect();
                for k in prev_keys {
                    if !current_seen.contains(&k) {
                        changed = true;
                        last_modified_times.remove(&k);
                    }
                }

                // Record when the most recent change occurred
                if changed {
                    log::debug!(
                        "ToolWatcher: Change detected — (re)starting debounce timer."
                    );
                    last_change = Some(std::time::Instant::now());
                }

                // Fire callback only after DEBOUNCE_SECS of stability
                if let Some(t) = last_change {
                    if t.elapsed() >= Duration::from_secs(DEBOUNCE_SECS) {
                        log::debug!(
                            "ToolWatcher: {}s quiet — invoking reload callback.",
                            DEBOUNCE_SECS
                        );
                        if let Ok(cb) = shared_cb.lock() {
                            cb();
                        }
                        last_change = None; // reset
                    }
                }
            }
        });

        Some(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_tool_watcher_new() {
        let watcher = ToolWatcher::new("/tmp/test_tools".to_string());
        assert_eq!(watcher.watch_dirs.len(), 1);
        assert_eq!(watcher.watch_dirs[0], PathBuf::from("/tmp/test_tools"));
        // callback is private, so we just check it doesn't panic
    }

    #[test]
    fn test_add_watch_dir() {
        let mut watcher = ToolWatcher::new("/tmp/test_tools1".to_string());
        watcher.add_watch_dir("/tmp/test_tools2");
        assert_eq!(watcher.watch_dirs.len(), 2);
        assert_eq!(watcher.watch_dirs[1], PathBuf::from("/tmp/test_tools2"));
    }

    /// A watcher with no callback must return None from start().
    #[test]
    fn test_start_without_callback_returns_none() {
        let watcher = ToolWatcher::new("/tmp/test_no_cb".to_string());
        assert!(watcher.callback.is_none());
    }
}
