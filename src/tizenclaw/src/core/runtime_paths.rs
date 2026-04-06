use std::path::PathBuf;

pub fn is_tizen_runtime() -> bool {
    std::path::Path::new("/etc/tizen-release").exists()
        || std::path::Path::new("/opt/usr/share/tizenclaw").exists()
}

pub fn default_data_dir() -> PathBuf {
    if let Ok(path) = std::env::var("TIZENCLAW_DATA_DIR") {
        return PathBuf::from(path);
    }
    if is_tizen_runtime() {
        return PathBuf::from("/opt/usr/share/tizenclaw");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".tizenclaw")
}

pub fn default_dashboard_port() -> u16 {
    if is_tizen_runtime() {
        9090
    } else {
        8080
    }
}

pub fn default_tools_dir() -> PathBuf {
    if let Ok(path) = std::env::var("TIZENCLAW_TOOLS_DIR") {
        return PathBuf::from(path);
    }
    default_data_dir().join("tools")
}
