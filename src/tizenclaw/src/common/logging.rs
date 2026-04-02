//! Logging module — platform-agnostic log backend.
//!
//! On Tizen: dlog via the tizen platform plugin (libtizenclaw_plugin.so)
//! On generic Linux: stderr + optional file-based logging
//!
//! Usage (via standard `log` crate macros):
//!   log::info!("message {}", value);
//!   log::error!("something failed: {}", err);

use std::sync::{Mutex, Once};

const TAG: &str = "TIZENCLAW";

static INIT: Once = Once::new();

/// Global platform logger reference — set once during init.
static PLATFORM_LOGGER: Mutex<Option<std::sync::Arc<dyn libtizenclaw_core::framework::PlatformLogger>>> =
    Mutex::new(None);

/// Initialize with a specific platform logger.
pub fn init_with_logger(logger: Option<std::sync::Arc<dyn libtizenclaw_core::framework::PlatformLogger>>) {
    INIT.call_once(|| {
        if let Some(pl) = logger {
            if let Ok(mut guard) = PLATFORM_LOGGER.lock() {
                *guard = Some(pl);
            }
        }
        log::set_logger(&PlatformLogBridge).unwrap();
        log::set_max_level(log::LevelFilter::Debug);
    });
}

struct PlatformLogBridge;

impl log::Log for PlatformLogBridge {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let level = match record.level() {
            log::Level::Error => libtizenclaw_core::framework::LogLevel::Error,
            log::Level::Warn  => libtizenclaw_core::framework::LogLevel::Warn,
            log::Level::Info  => libtizenclaw_core::framework::LogLevel::Info,
            log::Level::Debug | log::Level::Trace => libtizenclaw_core::framework::LogLevel::Debug,
        };

        let msg = format!(
            "{:>30}:{} : {}",
            record.file().unwrap_or("?"),
            record.line().unwrap_or(0),
            record.args()
        );

        // Use platform logger if available, otherwise stderr
        if let Ok(guard) = PLATFORM_LOGGER.lock() {
            if let Some(pl) = &*guard {
                pl.log(level, TAG, &msg);
                // Also write to file log if configured
                FileLogBackend::write(&msg, level);
                return;
            }
        }

        // Fallback: stderr
        let prefix = match level {
            libtizenclaw_core::framework::LogLevel::Error => "E",
            libtizenclaw_core::framework::LogLevel::Warn  => "W",
            libtizenclaw_core::framework::LogLevel::Info  => "I",
            libtizenclaw_core::framework::LogLevel::Debug => "D",
        };
        eprintln!("[{}] [{}] {}", prefix, TAG, msg);
        FileLogBackend::write(&msg, level);
    }

    fn flush(&self) {}
}

// ── Optional File log backend ──

static FILE_LOG: Mutex<Option<FileLogBackend>> = Mutex::new(None);

struct FileLogBackend {
    path: String,
    max_size: usize,
}

impl FileLogBackend {
    /// Initialize file-based log backend.
    pub fn init(path: &str, max_size: usize) {
        if let Ok(mut guard) = FILE_LOG.lock() {
            *guard = Some(FileLogBackend {
                path: path.to_string(),
                max_size,
            });
        }
    }

    fn write(msg: &str, level: libtizenclaw_core::framework::LogLevel) {
        if let Ok(guard) = FILE_LOG.lock() {
            if let Some(backend) = guard.as_ref() {
                let level_str = match level {
                    libtizenclaw_core::framework::LogLevel::Error => "E",
                    libtizenclaw_core::framework::LogLevel::Warn  => "W",
                    libtizenclaw_core::framework::LogLevel::Info  => "I",
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

                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&backend.path)
                    .and_then(|mut f| {
                        use std::io::Write;
                        f.write_all(line.as_bytes())
                    });
                // Rotate if needed
                if let Ok(meta) = std::fs::metadata(&backend.path) {
                    if meta.len() as usize > backend.max_size {
                        let rotated = format!("{}.old", backend.path);
                        let _ = std::fs::rename(&backend.path, &rotated);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn test_file_log_format_contains_pid() {
        let path = "test_tizenclaw.log";
        let _ = fs::remove_file(path);
        
        FileLogBackend::init(path, 1024);
        FileLogBackend::write("test payload", libtizenclaw_core::framework::LogLevel::Info);
        
        let content = fs::read_to_string(path).unwrap_or_default();
        let pid = std::process::id();
        assert!(content.contains(&format!("|{}|", pid)), "Log does not contain PID");
        assert!(content.contains("UTC|"), "Log does not contain UTC timestamp tag");
        assert!(content.contains("test payload"), "Log does not contain payload");
        
        let _ = fs::remove_file(path);
    }
}
