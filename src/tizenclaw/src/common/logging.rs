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
static PLATFORM_LOGGER: Mutex<Option<&'static dyn libtizenclaw::PlatformLogger>> =
    Mutex::new(None);

/// Initialize the logging backend.
///
/// If a `PlatformLogger` is provided, use it (e.g., Tizen dlog).
/// Otherwise, use the built-in stderr logger.
pub fn init() {
    init_with_logger(None);
}

/// Initialize with a specific platform logger.
pub fn init_with_logger(logger: Option<&'static dyn libtizenclaw::PlatformLogger>) {
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
            log::Level::Error => libtizenclaw::LogLevel::Error,
            log::Level::Warn  => libtizenclaw::LogLevel::Warn,
            log::Level::Info  => libtizenclaw::LogLevel::Info,
            log::Level::Debug | log::Level::Trace => libtizenclaw::LogLevel::Debug,
        };

        let msg = format!(
            "{:>30}:{} : {}",
            record.file().unwrap_or("?"),
            record.line().unwrap_or(0),
            record.args()
        );

        // Use platform logger if available, otherwise stderr
        if let Ok(guard) = PLATFORM_LOGGER.lock() {
            if let Some(pl) = *guard {
                pl.log(level, TAG, &msg);
                // Also write to file log if configured
                FileLogBackend::write(&msg, level);
                return;
            }
        }

        // Fallback: stderr
        let prefix = match level {
            libtizenclaw::LogLevel::Error => "E",
            libtizenclaw::LogLevel::Warn  => "W",
            libtizenclaw::LogLevel::Info  => "I",
            libtizenclaw::LogLevel::Debug => "D",
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

    fn write(msg: &str, level: libtizenclaw::LogLevel) {
        if let Ok(guard) = FILE_LOG.lock() {
            if let Some(backend) = guard.as_ref() {
                let level_str = match level {
                    libtizenclaw::LogLevel::Error => "E",
                    libtizenclaw::LogLevel::Warn  => "W",
                    libtizenclaw::LogLevel::Info  => "I",
                    libtizenclaw::LogLevel::Debug => "D",
                };
                let line = format!("[{}] {}\n", level_str, msg);
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
