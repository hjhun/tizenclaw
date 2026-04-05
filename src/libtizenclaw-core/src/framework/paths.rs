//! Platform-resolved paths.
//!
//! Determines the correct directories for data, config, tools, skills,
//! plugins, embedded tool descriptors, and web assets based on
//! environment variables or defaults.
//!
//! Priority:
//! 1. Environment variables (TIZENCLAW_DATA_DIR, TIZENCLAW_TOOLS_DIR, etc.)
//! 2. Tizen standard paths (if /opt/usr/share/tizenclaw exists)
//! 3. Host Linux paths (~/.tizenclaw)

use std::path::PathBuf;

/// All resolved platform paths.
#[derive(Debug, Clone)]
pub struct PlatformPaths {
    /// Main data directory (configs, sessions, etc.)
    pub data_dir: PathBuf,
    /// Configuration files directory
    pub config_dir: PathBuf,
    /// Tool scripts directory
    pub tools_dir: PathBuf,
    /// Textual skills directory
    pub skills_dir: PathBuf,
    /// TizenClaw-owned embedded tool descriptor directory
    pub embedded_tools_dir: PathBuf,
    /// Plugin .so files directory
    pub plugins_dir: PathBuf,
    /// Packaged reference docs directory
    pub docs_dir: PathBuf,
    /// Web dashboard static files
    pub web_root: PathBuf,
    /// Workflows directory
    pub workflows_dir: PathBuf,
    /// Generated and reusable code directory
    pub codes_dir: PathBuf,
    /// Log directory
    pub logs_dir: PathBuf,
    /// Actions directory
    pub actions_dir: PathBuf,
    /// Pipelines directory
    pub pipelines_dir: PathBuf,
    /// LLM backend plugins directory
    pub llm_plugins_dir: PathBuf,
    /// CLI plugins metadata directory
    pub cli_plugins_dir: PathBuf,
}

/// Tizen standard base path.
const TIZEN_DATA_DIR: &str = "/opt/usr/share/tizenclaw";
const TIZEN_TOOLS_DIR: &str = "/opt/usr/share/tizenclaw/tools";

impl PlatformPaths {
    /// Auto-detect paths based on environment and OS.
    pub fn detect() -> Self {
        // Check environment overrides first
        if let Ok(data_dir) = std::env::var("TIZENCLAW_DATA_DIR") {
            return Self::from_base(PathBuf::from(data_dir));
        }

        // Check if Tizen paths exist
        if PathBuf::from(TIZEN_DATA_DIR).exists() || is_tizen_environment() {
            return Self::tizen_defaults();
        }

        // Fallback: XDG-compliant Linux paths
        Self::linux_defaults()
    }

    /// Build paths from a custom base directory.
    pub fn from_base(base: PathBuf) -> Self {
        let tools_dir = std::env::var("TIZENCLAW_TOOLS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| base.join("tools"));

        let skills_dir = base.join("workspace/skills");
        let embedded_tools_dir = std::env::var("TIZENCLAW_EMBEDDED_TOOLS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| base.join("embedded"));

        PlatformPaths {
            config_dir: base.join("config"),
            tools_dir,
            skills_dir,
            embedded_tools_dir,
            plugins_dir: base.join("plugins"),
            docs_dir: base.join("docs"),
            web_root: base.join("web"),
            workflows_dir: base.join("workflows"),
            codes_dir: base.join("codes"),
            logs_dir: base.join("logs"),
            actions_dir: base.join("actions"),
            pipelines_dir: base.join("pipelines"),
            llm_plugins_dir: base.join("plugins/llm"),
            cli_plugins_dir: base.join("plugins/cli"),
            data_dir: base,
        }
    }

    /// Standard Tizen paths.
    fn tizen_defaults() -> Self {
        let tools_dir = std::env::var("TIZENCLAW_TOOLS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(TIZEN_TOOLS_DIR));

        let skills_dir = PathBuf::from(TIZEN_DATA_DIR).join("workspace/skills");
        let embedded_tools_dir = std::env::var("TIZENCLAW_EMBEDDED_TOOLS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(TIZEN_DATA_DIR).join("embedded"));

        PlatformPaths {
            data_dir: PathBuf::from(TIZEN_DATA_DIR),
            config_dir: PathBuf::from(TIZEN_DATA_DIR).join("config"),
            tools_dir,
            skills_dir,
            embedded_tools_dir,
            plugins_dir: PathBuf::from(TIZEN_DATA_DIR).join("plugins"),
            docs_dir: PathBuf::from(TIZEN_DATA_DIR).join("docs"),
            web_root: PathBuf::from(TIZEN_DATA_DIR).join("web"),
            workflows_dir: PathBuf::from(TIZEN_DATA_DIR).join("workflows"),
            codes_dir: PathBuf::from(TIZEN_DATA_DIR).join("codes"),
            logs_dir: PathBuf::from(TIZEN_DATA_DIR).join("logs"),
            actions_dir: PathBuf::from(TIZEN_DATA_DIR).join("actions"),
            pipelines_dir: PathBuf::from(TIZEN_DATA_DIR).join("pipelines"),
            llm_plugins_dir: PathBuf::from(TIZEN_DATA_DIR).join("plugins/llm"),
            cli_plugins_dir: PathBuf::from(TIZEN_DATA_DIR).join("plugins/cli"),
        }
    }

    /// Host Linux paths.
    fn linux_defaults() -> Self {
        let base = dirs_or_home().join(".tizenclaw");
        Self::from_base(base)
    }

    /// Ensure all directories exist (create if missing).
    pub fn ensure_dirs(&self) {
        let dirs = [
            &self.data_dir,
            &self.config_dir,
            &self.tools_dir,
            &self.skills_dir,
            &self.embedded_tools_dir,
            &self.plugins_dir,
            &self.docs_dir,
            &self.web_root,
            &self.workflows_dir,
            &self.codes_dir,
            &self.logs_dir,
        ];
        for dir in &dirs {
            if !dir.exists() {
                if let Err(e) = std::fs::create_dir_all(dir) {
                    log::error!("Warning: failed to create dir {:?}: {}", dir, e);
                }
            }
        }
    }

    /// Get the session database path.
    pub fn sessions_db_path(&self) -> PathBuf {
        self.data_dir.join("sessions/sessions.db")
    }

    /// Get the app data directory for file-based storage.
    /// The project now keeps both Tizen and host runtime state under the
    /// resolved data root directly.
    pub fn app_data_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }
}

/// Check if we're running in a Tizen environment.
fn is_tizen_environment() -> bool {
    PathBuf::from("/etc/tizen-release").exists()
}

/// Get the user's home directory.
fn dirs_or_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_base_places_embedded_tools_under_base() {
        let base = PathBuf::from("/tmp/tizenclaw-paths");
        let paths = PlatformPaths::from_base(base.clone());

        assert_eq!(paths.tools_dir, base.join("tools"));
        assert_eq!(paths.skills_dir, base.join("workspace/skills"));
        assert_eq!(paths.embedded_tools_dir, base.join("embedded"));
        assert_eq!(paths.codes_dir, base.join("codes"));
    }

    #[test]
    fn ensure_dirs_creates_embedded_directory() {
        let unique = format!(
            "tizenclaw-paths-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let base = std::env::temp_dir().join(unique);
        let paths = PlatformPaths::from_base(base.clone());

        paths.ensure_dirs();

        assert!(paths.embedded_tools_dir.exists());
        assert!(paths.codes_dir.exists());

        let _ = std::fs::remove_dir_all(base);
    }
}
