//! Common utility functions: timestamps, file I/O, atomic writes.

use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Get current timestamp as ISO-8601 string (YYYY-MM-DDTHH:MM:SS).
pub fn timestamp_now() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    // Convert epoch to UTC broken-down time manually
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Days since 1970-01-01 to Y-M-D (simplified Gregorian)
    let (year, month, day) = epoch_days_to_ymd(days as i64);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        year, month, day, hours, minutes, seconds
    )
}

/// Get current date as YYYY-MM-DD.
pub fn date_today() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let days = dur.as_secs() / 86400;
    let (y, m, d) = epoch_days_to_ymd(days as i64);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// Get current month as YYYY-MM.
pub fn month_now() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let days = dur.as_secs() / 86400;
    let (y, m, _) = epoch_days_to_ymd(days as i64);
    format!("{:04}-{:02}", y, m)
}

/// Convert epoch days to (year, month, day).
fn epoch_days_to_ymd(days: i64) -> (i32, u32, u32) {
    // Civil calendar from epoch days
    // Algorithm from Howard Hinnant (http://howardhinnant.github.io/date_algorithms.html)
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64 + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Get current epoch time in milliseconds.
pub fn epoch_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Ensure a directory exists (equivalent to `mkdir -p`).
pub fn ensure_dir(path: &str) {
    let _ = fs::create_dir_all(path);
}

/// Atomic file write: write to .tmp then rename.
pub fn atomic_write(path: &str, content: &str) -> std::io::Result<()> {
    let tmp = format!("{}.tmp", path);
    let mut file = fs::File::create(&tmp)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Read entire file to string, returning empty string on error.
pub fn read_file(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

/// Read file, returning None if not found.
pub fn read_file_opt(path: &str) -> Option<String> {
    fs::read_to_string(path).ok()
}

/// Check if a file exists.
pub fn file_exists(path: &str) -> bool {
    Path::new(path).exists()
}

/// List files in a directory matching a suffix.
pub fn list_files(dir: &str, suffix: &str) -> Vec<String> {
    let mut result = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(suffix) {
                    if let Some(path) = entry.path().to_str() {
                        result.push(path.to_string());
                    }
                }
            }
        }
    }
    result.sort();
    result
}

/// List subdirectories in a directory.
pub fn list_dirs(dir: &str) -> Vec<String> {
    let mut result = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if let Some(name) = entry.file_name().to_str() {
                    result.push(name.to_string());
                }
            }
        }
    }
    result.sort();
    result
}

/// Simple string split (to avoid pulling in regex).
pub fn split_first(s: &str, delim: char) -> (&str, &str) {
    match s.find(delim) {
        Some(pos) => (&s[..pos], &s[pos + 1..]),
        None => (s, ""),
    }
}
