//! Boot status logger — tracks subsystem initialization timing.

use std::collections::HashMap;
use std::time::Instant;

pub struct BootStatusLogger {
    start_time: Instant,
    entries: Vec<(String, u64, bool)>,  // (name, duration_ms, success)
}

impl BootStatusLogger {
    pub fn new() -> Self {
        BootStatusLogger {
            start_time: Instant::now(),
            entries: vec![],
        }
    }

    pub fn track(&mut self, subsystem: &str) -> BootTracker {
        BootTracker {
            name: subsystem.to_string(),
            start: Instant::now(),
            failed: false,
        }
    }

    pub fn record(&mut self, name: String, duration_ms: u64, success: bool) {
        self.entries.push((name, duration_ms, success));
    }

    pub fn total_boot_time_ms(&self) -> u64 {
        self.start_time.elapsed().as_millis() as u64
    }

    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = self.entries.iter()
            .map(|(name, dur, ok)| format!("  {} {}ms {}", name, dur, if *ok { "OK" } else { "FAIL" }))
            .collect();
        parts.insert(0, format!("Boot completed in {}ms:", self.total_boot_time_ms()));
        parts.join("\n")
    }
}

pub struct BootTracker {
    name: String,
    start: Instant,
    failed: bool,
}

impl BootTracker {
    pub fn set_failed(&mut self, _reason: &str) {
        self.failed = true;
    }
}

impl Drop for BootTracker {
    fn drop(&mut self) {
        let duration = self.start.elapsed().as_millis() as u64;
        log::info!("Boot [{}]: {}ms {}", self.name, duration, if self.failed { "FAIL" } else { "OK" });
    }
}
