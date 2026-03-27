//! Logging module — wraps Tizen dlog with Rust log facade.
//!
//! Usage (via standard `log` crate macros):
//!   log::info!("message {}", value);
//!   log::error!("something failed: {}", err);

use std::ffi::CString;
use std::sync::Once;

const TAG: &str = "TIZENCLAW";

static INIT: Once = Once::new();

/// Initialize the dlog-based logger as the global `log` backend.
pub fn init() {
    INIT.call_once(|| {
        log::set_logger(&DlogLogger).unwrap();
        log::set_max_level(log::LevelFilter::Debug);
    });
}

struct DlogLogger;

impl log::Log for DlogLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let prio = match record.level() {
            log::Level::Error => tizen_sys::dlog::DLOG_ERROR,
            log::Level::Warn => tizen_sys::dlog::DLOG_WARN,
            log::Level::Info => tizen_sys::dlog::DLOG_INFO,
            log::Level::Debug | log::Level::Trace => tizen_sys::dlog::DLOG_DEBUG,
        };

        let msg = format!(
            "{:>30}:{} : {}",
            record.file().unwrap_or("?"),
            record.line().unwrap_or(0),
            record.args()
        );

        // Escape '%' to prevent format string attacks in dlog
        let escaped = msg.replace('%', "%%");
        if let (Ok(tag), Ok(cmsg)) = (CString::new(TAG), CString::new(escaped)) {
            unsafe {
                tizen_sys::dlog::dlog_print(prio, tag.as_ptr(), cmsg.as_ptr());
            }
        }

        // Also write to file log if configured
        FileLogBackend::write(&msg, prio);
    }

    fn flush(&self) {}
}

// ── Optional File log backend ──

use std::sync::Mutex;

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

    fn write(msg: &str, prio: std::os::raw::c_int) {
        if let Ok(guard) = FILE_LOG.lock() {
            if let Some(backend) = guard.as_ref() {
                let level = match prio {
                    x if x == tizen_sys::dlog::DLOG_ERROR => "E",
                    x if x == tizen_sys::dlog::DLOG_WARN => "W",
                    x if x == tizen_sys::dlog::DLOG_INFO => "I",
                    _ => "D",
                };
                let line = format!("[{}] {}\n", level, msg);
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
