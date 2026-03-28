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

            // Populate initial state
            for dir in &watch_dirs {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        if let Ok(metadata) = entry.metadata() {
                            if let Ok(modified) = metadata.modified() {
                                last_modified_times.insert(entry.path(), modified);
                            }
                        }
                    }
                }
            }

            loop {
                interval.tick().await;

                let mut changed = false;

                // Simple polling check: Any change in modified time or new files?
                for dir in &watch_dirs {
                    if let Ok(entries) = std::fs::read_dir(dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if let Ok(metadata) = entry.metadata() {
                                if let Ok(modified) = metadata.modified() {
                                    match last_modified_times.get(&path) {
                                        Some(&last_mod) if modified > last_mod => {
                                            changed = true;
                                            last_modified_times.insert(path, modified);
                                        }
                                        None => {
                                            // New file detected
                                            changed = true;
                                            last_modified_times.insert(path, modified);
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
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
