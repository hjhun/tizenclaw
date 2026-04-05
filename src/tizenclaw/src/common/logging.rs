//! Logging module — platform-agnostic log backend.
//!
//! On Tizen: dlog via the tizen platform plugin (libtizenclaw_plugin.so)
//! On generic Linux: stderr + optional file-based logging
//!
//! Usage (via standard `log` crate macros):
//!   log::debug!("message {}", value);
//!   log::error!("something failed: {}", err);

use std::path::{Path, PathBuf};
use std::sync::{Mutex, Once};

const TAG: &str = "TIZENCLAW";

static INIT: Once = Once::new();

/// Initialize with the TizenClaw global logger.
pub fn init_with_logger() {
    INIT.call_once(|| {
        log::set_logger(&PlatformLogBridge).unwrap();
        // Allow all levels to the bridge, we will filter in `enabled` based on target.
        log::set_max_level(log::LevelFilter::Trace);
    });
}

struct PlatformLogBridge;

impl log::Log for PlatformLogBridge {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        // Enforce strict filtering: ONLY tizenclaw internal modules get Debug/Trace.
        // Vendor crates (mdns_sd, hyper, rustls, etc.) are restricted to Warn/Error.
        if metadata.target().starts_with("tizenclaw")
            || metadata.target().starts_with("libtizenclaw")
        {
            metadata.level() <= log::Level::Debug
        } else {
            metadata.level() <= log::Level::Warn
        }
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let level = match record.level() {
            log::Level::Error => libtizenclaw_core::framework::LogLevel::Error,
            log::Level::Warn => libtizenclaw_core::framework::LogLevel::Warn,
            log::Level::Info => libtizenclaw_core::framework::LogLevel::Info,
            log::Level::Debug | log::Level::Trace => libtizenclaw_core::framework::LogLevel::Debug,
        };

        let filepath = record.file().unwrap_or("?");
        let filename = filepath
            .rsplit('/')
            .next()
            .unwrap_or(filepath)
            .rsplit('\\')
            .next()
            .unwrap_or(filepath);

        let msg = format!(
            "{}:{} {}",
            filename,
            record.line().unwrap_or(0),
            record.args()
        );

        let is_tizen = std::fs::read_to_string("/etc/os-release")
            .map(|s| s.to_lowercase().contains("tizen"))
            .unwrap_or(false);

        if is_tizen {
            let prio = match record.level() {
                log::Level::Error => libtizenclaw_core::tizen_sys::dlog::DLOG_ERROR,
                log::Level::Warn => libtizenclaw_core::tizen_sys::dlog::DLOG_WARN,
                log::Level::Info => libtizenclaw_core::tizen_sys::dlog::DLOG_INFO,
                log::Level::Debug | log::Level::Trace => {
                    libtizenclaw_core::tizen_sys::dlog::DLOG_DEBUG
                }
            };
            if let (Ok(tag_c), Ok(msg_c)) = (
                std::ffi::CString::new(TAG),
                std::ffi::CString::new(msg.replace('%', "%%")),
            ) {
                unsafe {
                    libtizenclaw_core::tizen_sys::dlog::dlog_print(
                        prio,
                        tag_c.as_ptr(),
                        msg_c.as_ptr(),
                    );
                }
            }
            FileLogBackend::write(&msg, level);
            return;
        }

        // Fallback: stderr
        let prefix = match level {
            libtizenclaw_core::framework::LogLevel::Error => "E",
            libtizenclaw_core::framework::LogLevel::Warn => "W",
            libtizenclaw_core::framework::LogLevel::Info => "I",
            libtizenclaw_core::framework::LogLevel::Debug => "D",
        };
        eprintln!("[{}] [{}] {}", prefix, TAG, msg);
        FileLogBackend::write(&msg, level);
    }

    fn flush(&self) {}
}

// ── Optional File log backend ──

static FILE_LOG: Mutex<Option<FileLogBackend>> = Mutex::new(None);

pub struct FileLogBackend {
    base_dir: PathBuf,
    max_size: usize,
}

impl FileLogBackend {
    /// Initialize file-based log backend.
    pub fn init(base_dir: &Path, max_size: usize) {
        if let Ok(mut guard) = FILE_LOG.lock() {
            *guard = Some(FileLogBackend {
                base_dir: base_dir.to_path_buf(),
                max_size,
            });
        }
    }

    fn write(msg: &str, level: libtizenclaw_core::framework::LogLevel) {
        if let Ok(guard) = FILE_LOG.lock() {
            if let Some(backend) = guard.as_ref() {
                let level_str = match level {
                    libtizenclaw_core::framework::LogLevel::Error => "E",
                    libtizenclaw_core::framework::LogLevel::Warn => "W",
                    libtizenclaw_core::framework::LogLevel::Info => "I",
                    libtizenclaw_core::framework::LogLevel::Debug => "D",
                };
                let pid = std::process::id();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                let secs = now.as_secs() as libc::time_t;
                let ms = now.subsec_millis();
                let mut tm_buf: libc::tm = unsafe { std::mem::zeroed() };
                unsafe { libc::gmtime_r(&secs, &mut tm_buf) };

                let ts = format!(
                    "{:04}{:02}{:02}.{:02}{:02}{:02}.{:03}UTC",
                    tm_buf.tm_year + 1900,
                    tm_buf.tm_mon + 1,
                    tm_buf.tm_mday,
                    tm_buf.tm_hour,
                    tm_buf.tm_min,
                    tm_buf.tm_sec,
                    ms
                );

                let line = format!("{}|{}|[{}] {}\n", ts, pid, level_str, msg);
                let path = backend.runtime_log_path();

                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }

                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .and_then(|mut f| {
                        use std::io::Write;
                        f.write_all(line.as_bytes())
                    });

                if let Ok(meta) = std::fs::metadata(&path) {
                    if meta.len() as usize > backend.max_size {
                        let rotated = path.with_extension("log.old");
                        let _ = std::fs::rename(&path, &rotated);
                    }
                }
            }
        }
    }

    fn runtime_log_path(&self) -> PathBuf {
        let now = local_time_parts();
        self.base_dir
            .join(format!("{:04}", now.year))
            .join(format!("{:02}", now.month))
            .join(format!("{:02}", now.day))
            .join(format!("{}.log", now.weekday_short))
    }
}

struct LocalTimeParts {
    year: i32,
    month: u32,
    day: u32,
    weekday_short: &'static str,
}

fn local_time_parts() -> LocalTimeParts {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as libc::time_t;
    let mut tm_buf: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        libc::localtime_r(&now, &mut tm_buf);
    }

    let weekday_short = match tm_buf.tm_wday {
        0 => "Sun",
        1 => "Mon",
        2 => "Tue",
        3 => "Wed",
        4 => "Thu",
        5 => "Fri",
        _ => "Sat",
    };

    LocalTimeParts {
        year: tm_buf.tm_year + 1900,
        month: (tm_buf.tm_mon + 1) as u32,
        day: tm_buf.tm_mday as u32,
        weekday_short,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn test_file_log_format_contains_pid() {
        let dir = std::env::temp_dir().join(format!(
            "tizenclaw-log-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);

        FileLogBackend::init(&dir, 1024);
        FileLogBackend::write("test payload", libtizenclaw_core::framework::LogLevel::Info);

        let path = FileLogBackend {
            base_dir: dir.clone(),
            max_size: 1024,
        }
        .runtime_log_path();
        let content = fs::read_to_string(&path).unwrap_or_default();
        let pid = std::process::id();
        assert!(
            content.contains(&format!("|{}|", pid)),
            "Log does not contain PID"
        );
        assert!(
            content.contains("UTC|"),
            "Log does not contain UTC timestamp tag"
        );
        assert!(
            content.contains("test payload"),
            "Log does not contain payload"
        );

        let _ = fs::remove_dir_all(dir);
    }
}
