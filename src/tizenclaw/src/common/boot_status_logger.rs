//! Boot status logger — tracks subsystem initialization timing.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

pub struct BootStatusLogger {
    log_path: PathBuf,
    start_time: Instant,
    entries: Vec<(String, u64, bool, String)>, // (name, duration_ms, success, detail)
}

impl BootStatusLogger {
    pub fn new(log_path: PathBuf) -> Self {
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        // Start each daemon boot with a fresh status file so only boot
        // checkpoints remain in `tizenclaw.log`.
        let _ = std::fs::File::create(&log_path);

        BootStatusLogger {
            log_path,
            start_time: Instant::now(),
            entries: vec![],
        }
    }

    pub fn record_status(&mut self, subsystem: &str, success: bool, detail: &str) {
        let duration_ms = self.start_time.elapsed().as_millis() as u64;
        self.entries.push((
            subsystem.to_string(),
            duration_ms,
            success,
            detail.to_string(),
        ));
        self.append_line(subsystem, success, detail, duration_ms);
    }

    pub fn total_boot_time_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = self
            .entries
            .iter()
            .map(|(name, dur, ok, detail)| {
                format!(
                    "  [{}] {} ({}ms) {}",
                    if *ok { "OK" } else { "FAIL" },
                    name,
                    dur,
                    detail
                )
            })
            .collect();
        parts.insert(
            0,
            format!("Boot completed in {}ms:", self.total_boot_time_ms()),
        );
        parts.join("\n")
    }

    fn append_line(&self, subsystem: &str, success: bool, detail: &str, duration_ms: u64) {
        if let Some(parent) = self.log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let status = if success { "[OK]" } else { "[FAIL]" };
        let line = format!("{} {} ({}ms) {}\n", status, subsystem, duration_ms, detail);
        let _ = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
            .and_then(|mut file| file.write_all(line.as_bytes()));
    }
}
