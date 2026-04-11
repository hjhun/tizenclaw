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

use std::path::{Path, PathBuf};

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
    /// Skill hub mount directory containing external OpenClaw-style roots
    pub skill_hubs_dir: PathBuf,
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
const HOST_DATA_DIR_NAME: &str = ".tizenclaw";

impl PlatformPaths {
    /// Resolve platform paths from environment and runtime markers.
    pub fn resolve() -> Self {
        let is_tizen = has_tizen_markers();
        let data_dir = std::env::var_os("TIZENCLAW_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| default_data_dir(is_tizen));

        Self {
            data_dir: data_dir.clone(),
            config_dir: resolve_path("TIZENCLAW_CONFIG_DIR", data_dir.join("config")),
            tools_dir: resolve_path("TIZENCLAW_TOOLS_DIR", data_dir.join("tools")),
            skills_dir: resolve_path("TIZENCLAW_SKILLS_DIR", data_dir.join("workspace/skills")),
            skill_hubs_dir: resolve_path(
                "TIZENCLAW_SKILL_HUBS_DIR",
                data_dir.join("workspace/skill-hubs"),
            ),
            embedded_tools_dir: resolve_path(
                "TIZENCLAW_EMBEDDED_TOOLS_DIR",
                data_dir.join("embedded"),
            ),
            plugins_dir: resolve_path("TIZENCLAW_PLUGINS_DIR", data_dir.join("plugins")),
            llm_plugins_dir: resolve_path(
                "TIZENCLAW_LLM_PLUGINS_DIR",
                data_dir.join("plugins/llm"),
            ),
            cli_plugins_dir: resolve_path(
                "TIZENCLAW_CLI_PLUGINS_DIR",
                data_dir.join("plugins/cli"),
            ),
            docs_dir: resolve_path("TIZENCLAW_DOCS_DIR", data_dir.join("docs")),
            web_root: resolve_path("TIZENCLAW_WEB_ROOT", data_dir.join("web")),
            workflows_dir: resolve_path("TIZENCLAW_WORKFLOWS_DIR", data_dir.join("workflows")),
            codes_dir: resolve_path("TIZENCLAW_CODES_DIR", data_dir.join("codes")),
            logs_dir: resolve_path("TIZENCLAW_LOGS_DIR", data_dir.join("logs")),
            actions_dir: resolve_path("TIZENCLAW_ACTIONS_DIR", data_dir.join("actions")),
            pipelines_dir: resolve_path("TIZENCLAW_PIPELINES_DIR", data_dir.join("pipelines")),
        }
    }

    /// Backward-compatible alias for older callers.
    pub fn detect() -> Self {
        Self::resolve()
    }

    /// Build paths from a custom base directory.
    pub fn from_base(base: PathBuf) -> Self {
        PlatformPaths {
            data_dir: base.clone(),
            config_dir: base.join("config"),
            tools_dir: base.join("tools"),
            skills_dir: base.join("workspace/skills"),
            skill_hubs_dir: base.join("workspace/skill-hubs"),
            embedded_tools_dir: base.join("embedded"),
            plugins_dir: base.join("plugins"),
            llm_plugins_dir: base.join("plugins/llm"),
            cli_plugins_dir: base.join("plugins/cli"),
            docs_dir: base.join("docs"),
            web_root: base.join("web"),
            workflows_dir: base.join("workflows"),
            codes_dir: base.join("codes"),
            logs_dir: base.join("logs"),
            actions_dir: base.join("actions"),
            pipelines_dir: base.join("pipelines"),
        }
    }

    /// Ensure all directories exist (create if missing).
    pub fn ensure_dirs(&self) {
        let dirs = [
            &self.data_dir,
            &self.config_dir,
            &self.tools_dir,
            &self.skills_dir,
            &self.skill_hubs_dir,
            &self.embedded_tools_dir,
            &self.plugins_dir,
            &self.docs_dir,
            &self.web_root,
            &self.workflows_dir,
            &self.codes_dir,
            &self.logs_dir,
            &self.actions_dir,
            &self.pipelines_dir,
            &self.llm_plugins_dir,
            &self.cli_plugins_dir,
        ];
        for dir in dirs {
            if let Err(err) = std::fs::create_dir_all(dir) {
                log::warn!("Failed to create dir {:?}: {}", dir, err);
            }
        }
    }

    pub fn is_tizen(&self) -> bool {
        has_tizen_markers()
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

    /// Discover external skill roots mounted under `workspace/skill-hubs`.
    pub fn discover_skill_hub_roots(&self) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.skill_hubs_dir) else {
            return roots;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                roots.push(path);
            }
        }

        roots.sort();
        roots
    }
}

fn resolve_path(env_key: &str, default: PathBuf) -> PathBuf {
    std::env::var_os(env_key)
        .map(PathBuf::from)
        .unwrap_or(default)
}

fn default_data_dir(is_tizen: bool) -> PathBuf {
    if is_tizen {
        PathBuf::from(TIZEN_DATA_DIR)
    } else {
        dirs_or_home().join(HOST_DATA_DIR_NAME)
    }
}

fn has_tizen_markers() -> bool {
    Path::new("/etc/tizen-release").exists() || Path::new(TIZEN_DATA_DIR).exists()
}

fn dirs_or_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn from_base_places_embedded_tools_under_base() {
        let base = PathBuf::from("/tmp/tizenclaw-paths");
        let paths = PlatformPaths::from_base(base.clone());

        assert_eq!(paths.tools_dir, base.join("tools"));
        assert_eq!(paths.skills_dir, base.join("workspace/skills"));
        assert_eq!(paths.skill_hubs_dir, base.join("workspace/skill-hubs"));
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

        assert!(paths.skill_hubs_dir.exists());
        assert!(paths.embedded_tools_dir.exists());
        assert!(paths.codes_dir.exists());

        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn resolve_prefers_environment_overrides() {
        let _guard = env_lock().lock().unwrap();
        let original_data_dir = std::env::var_os("TIZENCLAW_DATA_DIR");
        let original_tools_dir = std::env::var_os("TIZENCLAW_TOOLS_DIR");
        let base = std::env::temp_dir().join(format!(
            "tizenclaw-env-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let tools = base.join("custom-tools");

        unsafe {
            std::env::set_var("TIZENCLAW_DATA_DIR", &base);
            std::env::set_var("TIZENCLAW_TOOLS_DIR", &tools);
        }

        let paths = PlatformPaths::resolve();

        assert_eq!(paths.data_dir, base);
        assert_eq!(paths.tools_dir, tools);
        assert_eq!(paths.plugins_dir, base.join("plugins"));
        assert_eq!(paths.llm_plugins_dir, base.join("plugins/llm"));

        unsafe {
            restore_env_var("TIZENCLAW_DATA_DIR", original_data_dir);
            restore_env_var("TIZENCLAW_TOOLS_DIR", original_tools_dir);
        }
    }

    #[test]
    fn resolve_uses_home_based_host_paths_without_overrides() {
        let _guard = env_lock().lock().unwrap();
        let original_home = std::env::var_os("HOME");
        let original_data_dir = std::env::var_os("TIZENCLAW_DATA_DIR");
        let home = std::env::temp_dir().join(format!(
            "tizenclaw-home-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        unsafe {
            std::env::remove_var("TIZENCLAW_DATA_DIR");
            std::env::set_var("HOME", &home);
        }

        let paths = PlatformPaths::resolve();

        assert_eq!(paths.data_dir, home.join(".tizenclaw"));
        assert_eq!(paths.config_dir, home.join(".tizenclaw/config"));

        unsafe {
            restore_env_var("HOME", original_home);
            restore_env_var("TIZENCLAW_DATA_DIR", original_data_dir);
        }
    }

    #[test]
    fn platform_paths_resolve_host() {
        let _guard = env_lock().lock().unwrap();
        let original_home = std::env::var_os("HOME");
        let original_data_dir = std::env::var_os("TIZENCLAW_DATA_DIR");
        let home = std::env::temp_dir().join(format!(
            "tizenclaw-host-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        unsafe {
            std::env::remove_var("TIZENCLAW_DATA_DIR");
            std::env::set_var("HOME", &home);
        }

        let paths = PlatformPaths::resolve();
        assert!(!paths.is_tizen() || paths.data_dir.to_string_lossy().contains("tizenclaw"));

        unsafe {
            restore_env_var("HOME", original_home);
            restore_env_var("TIZENCLAW_DATA_DIR", original_data_dir);
        }
    }

    #[test]
    fn discover_skill_hub_roots_lists_child_directories() {
        let unique = format!(
            "tizenclaw-skill-hubs-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let base = std::env::temp_dir().join(unique);
        let paths = PlatformPaths::from_base(base.clone());
        std::fs::create_dir_all(&paths.skill_hubs_dir).unwrap();
        std::fs::create_dir_all(paths.skill_hubs_dir.join("openclaw")).unwrap();
        std::fs::write(paths.skill_hubs_dir.join("README.md"), "ignore").unwrap();

        let roots = paths.discover_skill_hub_roots();

        assert_eq!(roots, vec![paths.skill_hubs_dir.join("openclaw")]);

        let _ = std::fs::remove_dir_all(base);
    }

    unsafe fn restore_env_var(key: &str, value: Option<std::ffi::OsString>) {
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}
