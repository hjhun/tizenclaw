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
            log::info!("ToolWatcher: Monitoring tool directories for changes...");
            // Polling interval 3 seconds
            let mut interval = tokio::time::interval(Duration::from_secs(3));
            let mut last_modified_times: std::collections::HashMap<PathBuf, std::time::SystemTime> = std::collections::HashMap::new();

            fn scan_dir_recursive(
                dir: &Path,
                max_depth: usize,
                current_depth: usize,
                last_modified_times: &mut std::collections::HashMap<PathBuf, std::time::SystemTime>,
                current_seen: &mut std::collections::HashSet<PathBuf>,
            ) -> bool {
                if current_depth > max_depth { return false; }
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
                                        changed = true;
                                        last_modified_times.insert(path.clone(), modified);
                                    }
                                    _ => {}
                                }
                            }
                            // Recurse into subdirectories
                            if metadata.is_dir() {
                                if scan_dir_recursive(&path, max_depth, current_depth + 1, last_modified_times, current_seen) {
                                    changed = true;
                                }
                            }
                        }
                    }
                }
                changed
            }

            // Populate initial state
            for dir in &watch_dirs {
                let mut current_seen = std::collections::HashSet::new();
                scan_dir_recursive(dir, 3, 0, &mut last_modified_times, &mut current_seen);
            }

            loop {
                interval.tick().await;

                let mut changed = false;
                let mut current_seen = std::collections::HashSet::new();

                // Recursive polling check: Any change in modified time or new files?
                for dir in &watch_dirs {
                    if scan_dir_recursive(dir, 3, 0, &mut last_modified_times, &mut current_seen) {
                        changed = true;
                    }
                }

                // Detect deleted files
                let prev_keys: Vec<PathBuf> = last_modified_times.keys().cloned().collect();
                for k in prev_keys {
                    if !current_seen.contains(&k) {
                        changed = true;
                        last_modified_times.remove(&k);
                    }
                }

                if changed {
                    log::info!("ToolWatcher: Change detected in tool directories, reloading tools.");
                    if let Ok(cb) = shared_cb.lock() {
                        cb();
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
}
