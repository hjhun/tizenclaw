//! Health monitor — tracks system resource usage.

use std::fs;

pub struct HealthStatus {
    pub memory_used_kb: u64,
    pub memory_total_kb: u64,
    pub cpu_load_percent: f64,
    pub uptime_secs: u64,
}

pub struct HealthMonitor {
    start_time: std::time::Instant,
}

impl Default for HealthMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthMonitor {
    pub fn new() -> Self {
        HealthMonitor {
            start_time: std::time::Instant::now(),
        }
    }

    /// Get current system health snapshot.
    pub fn get_status(&self) -> HealthStatus {
        let (mem_used, mem_total) = read_meminfo();
        let cpu_load = read_loadavg();
        HealthStatus {
            memory_used_kb: mem_total.saturating_sub(mem_used),
            memory_total_kb: mem_total,
            cpu_load_percent: cpu_load,
            uptime_secs: self.start_time.elapsed().as_secs(),
        }
    }
}

fn read_meminfo() -> (u64, u64) {
    let content = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut total = 0u64;
    let mut available = 0u64;
    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            total = parse_kb_value(line);
        } else if line.starts_with("MemAvailable:") {
            available = parse_kb_value(line);
        }
    }
    (available, total)
}

fn parse_kb_value(line: &str) -> u64 {
    line.split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

fn read_loadavg() -> f64 {
    let content = fs::read_to_string("/proc/loadavg").unwrap_or_default();
    content
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0)
}
