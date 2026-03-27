//! OTA updater — over-the-air self-update mechanism.
//!
//! Checks a remote manifest for new versions and applies updates
//! by downloading and replacing the binary.

use serde::{Deserialize, Serialize};
use serde_json::Value;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Update manifest describing an available update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    pub download_url: String,
    pub checksum_sha256: String,
    pub release_notes: String,
    pub min_supported_version: String,
}

pub struct OtaUpdater {
    manifest_url: String,
    install_path: String,
}

impl OtaUpdater {
    pub fn new(manifest_url: &str, install_path: &str) -> Self {
        OtaUpdater {
            manifest_url: manifest_url.to_string(),
            install_path: install_path.to_string(),
        }
    }

    /// Check for available updates.
    pub fn check_update(&self) -> Option<UpdateManifest> {
        let resp = crate::infra::http_client::http_get_sync(&self.manifest_url, &[], 1, 15);
        if !resp.success {
            log::warn!("OTA: failed to fetch update manifest: {}", resp.error);
            return None;
        }

        match serde_json::from_str::<UpdateManifest>(&resp.body) {
            Ok(manifest) => {
                if Self::is_newer(&manifest.version) {
                    log::info!(
                        "OTA: update available: {} -> {}",
                        CURRENT_VERSION,
                        manifest.version
                    );
                    Some(manifest)
                } else {
                    log::info!("OTA: current version {} is up to date", CURRENT_VERSION);
                    None
                }
            }
            Err(e) => {
                log::warn!("OTA: failed to parse manifest: {}", e);
                None
            }
        }
    }

    /// Get the current version.
    pub fn current_version() -> &'static str {
        CURRENT_VERSION
    }

    fn is_newer(new_version: &str) -> bool {
        // Simple semver comparison
        let parse = |v: &str| -> Vec<u32> {
            v.split('.')
                .filter_map(|s| s.parse().ok())
                .collect()
        };
        let current = parse(CURRENT_VERSION);
        let new = parse(new_version);

        for i in 0..3 {
            let c = current.get(i).copied().unwrap_or(0);
            let n = new.get(i).copied().unwrap_or(0);
            if n > c {
                return true;
            }
            if n < c {
                return false;
            }
        }
        false
    }
}
