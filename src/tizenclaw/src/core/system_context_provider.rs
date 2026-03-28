//! System context provider — gathers device/system state for LLM context.

use serde_json::{json, Value};

pub struct SystemContextProvider {
    cached_context: Option<Value>,
    last_update: std::time::Instant,
}

impl Default for SystemContextProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemContextProvider {
    pub fn new() -> Self {
        SystemContextProvider {
            cached_context: None,
            last_update: std::time::Instant::now(),
        }
    }

    pub fn start(&mut self) {
        self.refresh();
        log::info!("SystemContextProvider ready");
    }

    /// Get current system context (cached for 30s).
    pub fn get_context(&mut self) -> Value {
        if self.last_update.elapsed().as_secs() > 30 {
            self.refresh();
        }
        self.cached_context.clone().unwrap_or(json!({}))
    }

    fn refresh(&mut self) {
        let mut ctx = json!({});

        // Time
        ctx["current_time"] = json!(chrono_now());
        ctx["timezone"] = json!(get_timezone());

        // Battery
        if let Some(level) = read_sys_file("/sys/class/power_supply/battery/capacity") {
            ctx["battery_level"] = json!(level.trim());
        }

        // Network
        ctx["network_available"] = json!(std::net::TcpStream::connect("8.8.8.8:53")
            .map(|_| true).unwrap_or(false));

        // Hostname
        if let Ok(name) = std::fs::read_to_string("/etc/hostname") {
            ctx["hostname"] = json!(name.trim());
        }

        // Memory
        if let Some(meminfo) = read_sys_file("/proc/meminfo") {
            let mut total_kb = 0u64;
            let mut avail_kb = 0u64;
            for line in meminfo.lines() {
                if line.starts_with("MemTotal:") {
                    total_kb = parse_kb(line);
                } else if line.starts_with("MemAvailable:") {
                    avail_kb = parse_kb(line);
                }
            }
            if total_kb > 0 {
                ctx["memory_total_mb"] = json!(total_kb / 1024);
                ctx["memory_available_mb"] = json!(avail_kb / 1024);
            }
        }

        // Disk
        let statvfs = unsafe {
            let mut buf: libc::statvfs = std::mem::zeroed();
            let path = std::ffi::CString::new("/").unwrap();
            if libc::statvfs(path.as_ptr(), &mut buf) == 0 {
                Some(buf)
            } else {
                None
            }
        };
        if let Some(s) = statvfs {
            let total = s.f_blocks * s.f_frsize / (1024 * 1024);
            let free = s.f_bfree * s.f_frsize / (1024 * 1024);
            ctx["disk_total_mb"] = json!(total);
            ctx["disk_free_mb"] = json!(free);
        }

        self.cached_context = Some(ctx);
        self.last_update = std::time::Instant::now();
    }

    /// Format context as a string for system prompt injection.
    pub fn format_for_prompt(&mut self) -> String {
        let ctx = self.get_context();
        let mut parts = vec![];
        if let Some(t) = ctx.get("current_time").and_then(|v| v.as_str()) {
            parts.push(format!("Current time: {}", t));
        }
        if let Some(b) = ctx.get("battery_level").and_then(|v| v.as_str()) {
            parts.push(format!("Battery: {}%", b));
        }
        if let Some(n) = ctx.get("network_available").and_then(|v| v.as_bool()) {
            parts.push(format!("Network: {}", if n { "connected" } else { "offline" }));
        }
        if let Some(m) = ctx.get("memory_available_mb").and_then(|v| v.as_u64()) {
            parts.push(format!("Free memory: {}MB", m));
        }
        if parts.is_empty() { return String::new(); }
        format!("[System Context]\n{}", parts.join("\n"))
    }
}

fn chrono_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple ISO-ish format
    format!("{}", secs)
}

fn get_timezone() -> String {
    std::fs::read_to_string("/etc/timezone")
        .unwrap_or_else(|_| "UTC".into())
        .trim().to_string()
}

fn read_sys_file(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

fn parse_kb(line: &str) -> u64 {
    line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0)
}
