use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const TIZEN_DASHBOARD_PORT: u16 = 9090;
const HOST_DASHBOARD_PORT: u16 = 9091;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeTopology {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
    pub registry_dir: PathBuf,
    pub loop_state_dir: PathBuf,
    pub sessions_dir: PathBuf,
    pub memory_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub outbound_dir: PathBuf,
    pub plugins_dir: PathBuf,
    pub skills_dir: PathBuf,
    pub skill_hubs_dir: PathBuf,
    pub telegram_sessions_dir: PathBuf,
    pub tools_dir: PathBuf,
}

impl RuntimeTopology {
    pub fn from_data_dir(data_dir: impl Into<PathBuf>) -> Self {
        let data_dir = data_dir.into();
        let state_dir = data_dir.join("state");
        Self {
            config_dir: data_dir.join("config"),
            registry_dir: state_dir.join("registry"),
            loop_state_dir: state_dir.join("loop"),
            sessions_dir: data_dir.join("sessions"),
            memory_dir: data_dir.join("memory"),
            logs_dir: data_dir.join("logs"),
            outbound_dir: data_dir.join("outbound"),
            plugins_dir: data_dir.join("plugins"),
            skills_dir: data_dir.join("workspace").join("skills"),
            skill_hubs_dir: data_dir.join("workspace").join("skill-hubs"),
            telegram_sessions_dir: data_dir.join("telegram_sessions"),
            tools_dir: data_dir.join("tools"),
            state_dir,
            data_dir,
        }
    }

    pub fn detect() -> Self {
        Self::from_data_dir(default_data_dir())
    }

    pub fn from_config_dir(config_dir: &Path) -> Self {
        let data_dir = config_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(default_data_dir);
        Self::from_data_dir(data_dir)
    }

    pub fn registry_snapshot_path(&self) -> PathBuf {
        self.registry_dir.join("registered_paths.v2.json")
    }

    pub fn loop_state_path(&self, session_id: &str) -> PathBuf {
        self.loop_state_dir.join(format!("{}.json", session_id))
    }

    pub fn summary_json(&self) -> Value {
        json!({
            "data_dir": self.data_dir,
            "config_dir": self.config_dir,
            "state_dir": self.state_dir,
            "registry_dir": self.registry_dir,
            "loop_state_dir": self.loop_state_dir,
            "sessions_dir": self.sessions_dir,
            "memory_dir": self.memory_dir,
            "logs_dir": self.logs_dir,
            "outbound_dir": self.outbound_dir,
            "plugins_dir": self.plugins_dir,
            "skills_dir": self.skills_dir,
            "skill_hubs_dir": self.skill_hubs_dir,
            "telegram_sessions_dir": self.telegram_sessions_dir,
            "tools_dir": self.tools_dir,
        })
    }
}

pub fn is_tizen_runtime() -> bool {
    std::path::Path::new("/etc/tizen-release").exists()
        || std::path::Path::new("/opt/usr/share/tizenclaw").exists()
}

pub fn default_data_dir() -> PathBuf {
    libtizenclaw_core::framework::paths::PlatformPaths::detect().data_dir
}

fn default_dashboard_port_for_runtime(is_tizen_runtime: bool) -> u16 {
    if is_tizen_runtime {
        TIZEN_DASHBOARD_PORT
    } else {
        HOST_DASHBOARD_PORT
    }
}

pub fn default_dashboard_port() -> u16 {
    default_dashboard_port_for_runtime(is_tizen_runtime())
}

pub fn default_dashboard_base_url() -> String {
    format!("http://localhost:{}", default_dashboard_port())
}

pub fn default_tools_dir() -> PathBuf {
    libtizenclaw_core::framework::paths::PlatformPaths::detect().tools_dir
}

#[cfg(test)]
mod tests {
    use super::{
        default_dashboard_base_url, default_dashboard_port_for_runtime, RuntimeTopology,
        HOST_DASHBOARD_PORT, TIZEN_DASHBOARD_PORT,
    };
    use std::path::Path;

    #[test]
    fn default_dashboard_port_uses_tizen_default_on_tizen_runtime() {
        assert_eq!(
            default_dashboard_port_for_runtime(true),
            TIZEN_DASHBOARD_PORT
        );
    }

    #[test]
    fn default_dashboard_port_uses_ubuntu_default_on_host_runtime() {
        assert_eq!(
            default_dashboard_port_for_runtime(false),
            HOST_DASHBOARD_PORT
        );
    }

    #[test]
    fn default_dashboard_base_url_uses_localhost_with_default_port() {
        let url = default_dashboard_base_url();

        assert!(
            url == format!("http://localhost:{}", TIZEN_DASHBOARD_PORT)
                || url == format!("http://localhost:{}", HOST_DASHBOARD_PORT)
        );
    }

    #[test]
    fn runtime_topology_exposes_expected_contract_dirs() {
        let topology = RuntimeTopology::from_data_dir("/tmp/tizenclaw-topology");

        assert_eq!(
            topology.config_dir,
            Path::new("/tmp/tizenclaw-topology/config")
        );
        assert_eq!(
            topology.state_dir,
            Path::new("/tmp/tizenclaw-topology/state")
        );
        assert_eq!(
            topology.registry_dir,
            Path::new("/tmp/tizenclaw-topology/state/registry")
        );
        assert_eq!(
            topology.loop_state_dir,
            Path::new("/tmp/tizenclaw-topology/state/loop")
        );
        assert_eq!(
            topology.telegram_sessions_dir,
            Path::new("/tmp/tizenclaw-topology/telegram_sessions")
        );
    }

    #[test]
    fn runtime_topology_summary_includes_registry_dir() {
        let topology = RuntimeTopology::from_data_dir("/tmp/tizenclaw-topology");
        let summary = topology.summary_json();

        assert_eq!(
            summary["registry_dir"],
            "/tmp/tizenclaw-topology/state/registry"
        );
        assert_eq!(
            summary["loop_state_dir"],
            "/tmp/tizenclaw-topology/state/loop"
        );
        assert_eq!(summary["tools_dir"], "/tmp/tizenclaw-topology/tools");
    }
}
