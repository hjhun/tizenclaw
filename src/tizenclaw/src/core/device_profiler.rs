//! Device profiler — collects device hardware/software profile information.

use serde_json::{json, Value};

pub struct DeviceProfiler;

impl DeviceProfiler {
    pub fn new() -> Self { DeviceProfiler }

    pub fn get_profile(&self) -> Value {
        let mut profile = json!({});

        // CPU info
        if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
            let cores = cpuinfo.matches("processor").count();
            profile["cpu_cores"] = json!(cores);
            for line in cpuinfo.lines() {
                if line.starts_with("model name") {
                    if let Some(name) = line.split(':').nth(1) {
                        profile["cpu_model"] = json!(name.trim());
                        break;
                    }
                }
            }
        }

        // Memory
        if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            for line in meminfo.lines() {
                if line.starts_with("MemTotal:") {
                    let kb: u64 = line.split_whitespace().nth(1)
                        .and_then(|s| s.parse().ok()).unwrap_or(0);
                    profile["memory_mb"] = json!(kb / 1024);
                    break;
                }
            }
        }

        // OS version
        if let Ok(release) = std::fs::read_to_string("/etc/tizen-release") {
            profile["os_version"] = json!(release.trim());
        }

        // Display
        if let Ok(fb) = std::fs::read_to_string("/sys/class/graphics/fb0/virtual_size") {
            profile["display_resolution"] = json!(fb.trim());
        }

        profile
    }
}
