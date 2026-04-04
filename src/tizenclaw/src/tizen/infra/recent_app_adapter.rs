//! Recent app adapter — tracks recently used applications.
//!
//! Reads the recent app history from the system to provide context for the AI agent.

use std::process::Command;

/// Info about a recently used app.
#[derive(Debug, Clone)]
pub struct RecentApp {
    pub app_id: String,
    pub label: String,
    pub last_used: u64, // Unix timestamp
}

pub struct RecentAppAdapter;

impl RecentAppAdapter {
    /// Get the list of recently used applications.
    pub fn get_recent_apps(max_count: usize) -> Vec<RecentApp> {
        // Use app_launcher or app_manager CLI to query recent apps
        let output = Command::new("app_launcher")
            .args(["-l"])
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let mut apps: Vec<RecentApp> = stdout
                    .lines()
                    .filter_map(|line| {
                        let trimmed = line.trim();
                        if trimmed.is_empty() || trimmed.starts_with('#') {
                            return None;
                        }
                        // Format: app_id 'label'
                        let parts: Vec<&str> = trimmed.splitn(2, '\'').collect();
                        parts.first().map(|app_id| RecentApp {
                                app_id: app_id.trim().to_string(),
                                label: parts.get(1).unwrap_or(&"").trim_end_matches('\'').to_string(),
                                last_used: 0,
                            })
                    })
                    .collect();
                apps.truncate(max_count);
                apps
            }
            Err(e) => {
                log::debug!("RecentAppAdapter: app_launcher failed: {}", e);
                Vec::new()
            }
        }
    }
}
