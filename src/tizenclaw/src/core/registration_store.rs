use crate::core::runtime_paths::RuntimeTopology;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegistrationKind {
    Tool,
    Skill,
}

impl RegistrationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            RegistrationKind::Tool => "tool",
            RegistrationKind::Skill => "skill",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RegisteredPaths {
    #[serde(default)]
    pub tool_paths: Vec<String>,
    #[serde(default)]
    pub skill_paths: Vec<String>,
    #[serde(default)]
    pub entries: Vec<RegisteredPathEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegisteredPathEntry {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub source: String,
    pub active: bool,
    pub created_at_ms: u64,
}

impl RegisteredPaths {
    pub fn load(config_dir: &Path) -> Self {
        let compatibility_path = compatibility_path(config_dir);
        let content = match std::fs::read_to_string(&compatibility_path) {
            Ok(content) => content,
            Err(_) => return Self::default(),
        };
        let mut registrations = serde_json::from_str::<Self>(&content).unwrap_or_default();
        let snapshot_path = snapshot_path(config_dir);
        if let Ok(snapshot) = std::fs::read_to_string(&snapshot_path) {
            if let Ok(snapshot_doc) = serde_json::from_str::<Self>(&snapshot) {
                if !snapshot_doc.entries.is_empty() {
                    registrations.entries = snapshot_doc.entries;
                }
            }
        }
        registrations.ensure_entries();
        log::debug!(
            "RegistrationStore: loaded compatibility='{}' snapshot='{}' entries={}",
            compatibility_path.display(),
            snapshot_path.display(),
            registrations.entries.len()
        );
        registrations
    }

    pub fn save(&self, config_dir: &Path) -> Result<PathBuf, String> {
        let mut normalized = self.clone();
        normalized.ensure_entries();

        std::fs::create_dir_all(config_dir).map_err(|err| {
            format!(
                "Failed to create config dir '{}': {}",
                config_dir.display(),
                err
            )
        })?;
        let compatibility = compatibility_path(config_dir);
        let content = serde_json::to_string_pretty(&normalized)
            .map_err(|err| format!("Failed to serialize registered paths: {}", err))?;
        std::fs::write(&compatibility, content)
            .map_err(|err| format!("Failed to write '{}': {}", compatibility.display(), err))?;

        let snapshot = snapshot_path(config_dir);
        if let Some(parent) = snapshot.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "Failed to create registry dir '{}': {}",
                    parent.display(),
                    err
                )
            })?;
        }
        let snapshot_content = serde_json::to_string_pretty(&normalized)
            .map_err(|err| format!("Failed to serialize registry snapshot: {}", err))?;
        std::fs::write(&snapshot, snapshot_content)
            .map_err(|err| format!("Failed to write '{}': {}", snapshot.display(), err))?;
        log::info!(
            "RegistrationStore: saved compatibility='{}' snapshot='{}' entries={}",
            compatibility.display(),
            snapshot.display(),
            normalized.entries.len()
        );
        Ok(compatibility)
    }

    pub fn list_for_kind(&self, kind: RegistrationKind) -> &[String] {
        match kind {
            RegistrationKind::Tool => &self.tool_paths,
            RegistrationKind::Skill => &self.skill_paths,
        }
    }

    fn list_for_kind_mut(&mut self, kind: RegistrationKind) -> &mut Vec<String> {
        match kind {
            RegistrationKind::Tool => &mut self.tool_paths,
            RegistrationKind::Skill => &mut self.skill_paths,
        }
    }

    fn ensure_entries(&mut self) {
        if self.entries.is_empty() {
            for path in self.tool_paths.clone() {
                self.entries.push(RegisteredPathEntry::new(
                    RegistrationKind::Tool,
                    path,
                    "external",
                ));
            }
            for path in self.skill_paths.clone() {
                self.entries.push(RegisteredPathEntry::new(
                    RegistrationKind::Skill,
                    path,
                    "external",
                ));
            }
        }

        self.entries.sort_by(|left, right| {
            left.kind
                .cmp(&right.kind)
                .then_with(|| left.path.cmp(&right.path))
        });
        self.entries
            .dedup_by(|left, right| left.kind == right.kind && left.path == right.path);

        self.tool_paths = collect_paths_for_kind(&self.entries, RegistrationKind::Tool);
        self.skill_paths = collect_paths_for_kind(&self.entries, RegistrationKind::Skill);
    }
}

impl RegisteredPathEntry {
    pub fn new(kind: RegistrationKind, path: String, source: &str) -> Self {
        let created_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self {
            id: build_registration_id(kind, &path),
            kind: kind.as_str().to_string(),
            path,
            source: source.to_string(),
            active: true,
            created_at_ms,
        }
    }
}

fn compatibility_path(config_dir: &Path) -> PathBuf {
    config_dir.join("registered_paths.json")
}

fn snapshot_path(config_dir: &Path) -> PathBuf {
    RuntimeTopology::from_config_dir(config_dir).registry_snapshot_path()
}

fn build_registration_id(kind: RegistrationKind, path: &str) -> String {
    let normalized = path
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | ' ' => '-',
            _ => ch,
        })
        .collect::<String>();
    format!("{}-{}", kind.as_str(), normalized)
}

fn collect_paths_for_kind(entries: &[RegisteredPathEntry], kind: RegistrationKind) -> Vec<String> {
    let mut paths = entries
        .iter()
        .filter(|entry| entry.kind == kind.as_str() && entry.active)
        .map(|entry| entry.path.clone())
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
}

fn normalize_registration_path(raw: &str) -> Result<PathBuf, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Registration path cannot be empty".to_string());
    }

    let expanded = if trimmed == "~" || trimmed.starts_with("~/") {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        if trimmed == "~" {
            home
        } else {
            format!("{}/{}", home, &trimmed[2..])
        }
    } else {
        trimmed.to_string()
    };

    let candidate = PathBuf::from(&expanded);
    let absolute = if candidate.is_absolute() {
        candidate
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/tmp"))
            .join(candidate)
    };

    Ok(absolute)
}

pub fn canonicalize_registration_path(raw: &str) -> Result<String, String> {
    let absolute = normalize_registration_path(raw)?;
    let canonical = std::fs::canonicalize(&absolute)
        .map_err(|err| format!("Failed to resolve '{}': {}", absolute.display(), err))?;
    if !canonical.exists() {
        return Err(format!("Path '{}' does not exist", canonical.display()));
    }
    if !canonical.is_dir() {
        return Err(format!("Path '{}' is not a directory", canonical.display()));
    }

    Ok(canonical.to_string_lossy().to_string())
}

pub fn best_effort_registration_path(raw: &str) -> Result<String, String> {
    let absolute = normalize_registration_path(raw)?;
    match std::fs::canonicalize(&absolute) {
        Ok(canonical) => Ok(canonical.to_string_lossy().to_string()),
        Err(_) => Ok(absolute.to_string_lossy().to_string()),
    }
}

pub fn register_path(
    config_dir: &Path,
    kind: RegistrationKind,
    raw_path: &str,
) -> Result<(RegisteredPaths, String), String> {
    let canonical = canonicalize_registration_path(raw_path)?;
    let mut registrations = RegisteredPaths::load(config_dir);
    let entries = registrations.list_for_kind_mut(kind);
    if !entries.iter().any(|existing| existing == &canonical) {
        entries.push(canonical.clone());
        entries.sort();
        entries.dedup();
    }
    if !registrations
        .entries
        .iter()
        .any(|entry| entry.kind == kind.as_str() && entry.path == canonical)
    {
        registrations.entries.push(RegisteredPathEntry::new(
            kind,
            canonical.clone(),
            "external",
        ));
    }
    let saved_path = registrations.save(config_dir)?;
    log::info!(
        "RegistrationStore: registered kind='{}' path='{}'",
        kind.as_str(),
        canonical
    );
    Ok((registrations, saved_path.to_string_lossy().to_string()))
}

pub fn unregister_path(
    config_dir: &Path,
    kind: RegistrationKind,
    raw_path: &str,
) -> Result<(RegisteredPaths, bool, String), String> {
    let canonical = best_effort_registration_path(raw_path)?;
    let mut registrations = RegisteredPaths::load(config_dir);
    let removed = {
        let entries = registrations.list_for_kind_mut(kind);
        let before = entries.len();
        entries.retain(|entry| entry != &canonical);
        before != entries.len()
    };
    registrations
        .entries
        .retain(|entry| !(entry.kind == kind.as_str() && entry.path == canonical));
    let saved_path = registrations.save(config_dir)?;
    log::info!(
        "RegistrationStore: unregistered kind='{}' path='{}' removed={}",
        kind.as_str(),
        canonical,
        removed
    );
    Ok((
        registrations,
        removed,
        saved_path.to_string_lossy().to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_path_deduplicates_entries() {
        let dir = tempfile::tempdir().unwrap();
        let skills = dir.path().join("skills");
        std::fs::create_dir_all(&skills).unwrap();

        let (first, _) = register_path(
            dir.path(),
            RegistrationKind::Skill,
            skills.to_str().unwrap(),
        )
        .unwrap();
        let (second, _) = register_path(
            dir.path(),
            RegistrationKind::Skill,
            skills.to_str().unwrap(),
        )
        .unwrap();

        assert_eq!(first.skill_paths.len(), 1);
        assert_eq!(second.skill_paths.len(), 1);
        assert_eq!(second.entries.len(), 1);
        assert_eq!(second.entries[0].kind, "skill");
    }

    #[test]
    fn save_writes_registry_snapshot_under_state_dir() {
        let dir = tempfile::tempdir().unwrap();
        let mut registrations = RegisteredPaths::default();
        registrations.entries.push(RegisteredPathEntry::new(
            RegistrationKind::Tool,
            "/tmp/tool-root".to_string(),
            "external",
        ));

        let compatibility = registrations.save(&dir.path().join("config")).unwrap();
        let snapshot = dir.path().join("state/registry/registered_paths.v2.json");

        assert_eq!(
            compatibility,
            dir.path().join("config/registered_paths.json")
        );
        assert!(snapshot.exists());
    }

    #[test]
    fn load_synthesizes_entries_from_legacy_document() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(
            config_dir.join("registered_paths.json"),
            r#"{
  "tool_paths": ["/tmp/tools"],
  "skill_paths": ["/tmp/skills"]
}"#,
        )
        .unwrap();

        let registrations = RegisteredPaths::load(&config_dir);

        assert_eq!(registrations.entries.len(), 2);
        assert!(registrations
            .entries
            .iter()
            .any(|entry| entry.kind == "tool"));
        assert!(registrations
            .entries
            .iter()
            .any(|entry| entry.kind == "skill"));
    }
}
