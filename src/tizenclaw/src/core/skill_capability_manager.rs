use crate::core::registration_store::RegisteredPaths;
use crate::core::skill_support::normalize_skill_name;
use crate::core::textual_skill_scanner::{scan_textual_skills_from_roots, TextualSkill};
use crate::core::tool_dispatcher::ToolDispatcher;
use libtizenclaw_core::framework::paths::PlatformPaths;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

const SKILL_CAPABILITIES_CONFIG: &str = "skill_capabilities.json";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SkillCapabilityConfig {
    #[serde(default)]
    pub disabled_skills: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillRoot {
    pub path: String,
    pub kind: String,
    pub external: bool,
}

#[derive(Clone, Debug)]
pub struct SkillCapabilityEntry {
    pub skill: TextualSkill,
    pub source_root: String,
    pub root_kind: String,
    pub enabled: bool,
    pub dependency_ready: bool,
    pub missing_requires: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct SkillCapabilitySnapshot {
    pub config_path: String,
    pub disabled_skills: Vec<String>,
    pub roots: Vec<SkillRoot>,
    pub skills: Vec<SkillCapabilityEntry>,
}

impl SkillCapabilitySnapshot {
    pub fn enabled_skills(&self) -> Vec<TextualSkill> {
        self.skills
            .iter()
            .filter(|entry| entry.enabled)
            .map(|entry| entry.skill.clone())
            .collect()
    }

    pub fn is_disabled(&self, name: &str) -> bool {
        let normalized = normalize_skill_name(name);
        self.disabled_skills.iter().any(|entry| entry == &normalized)
    }

    pub fn find_skill(&self, name: &str) -> Option<&SkillCapabilityEntry> {
        let normalized = normalize_skill_name(name);
        self.skills
            .iter()
            .find(|entry| {
                entry.skill.file_name == name
                    || normalize_skill_name(&entry.skill.file_name) == normalized
            })
    }

    pub fn summary_json(&self) -> Value {
        let total_count = self.skills.len();
        let enabled_count = self.skills.iter().filter(|entry| entry.enabled).count();
        let disabled_count = total_count.saturating_sub(enabled_count);
        let dependency_blocked_count = self
            .skills
            .iter()
            .filter(|entry| !entry.dependency_ready)
            .count();
        let external_roots = self
            .roots
            .iter()
            .filter(|root| root.external)
            .map(|root| root.path.clone())
            .collect::<Vec<_>>();
        let managed_roots = self.root_paths_for_kind("user");
        let system_roots = self.root_paths_for_kind("system");
        let external_roots_by_kind = self.root_paths_for_kind("external");

        json!({
            "config_path": self.config_path,
            "disabled_skills": self.disabled_skills,
            "total_count": total_count,
            "enabled_count": enabled_count,
            "disabled_count": disabled_count,
            "dependency_blocked_count": dependency_blocked_count,
            "external_root_count": external_roots.len(),
            "external_roots": external_roots,
            "roots": {
                "managed": managed_roots.clone(),
                "user": managed_roots,
                "system": system_roots,
                "external": external_roots_by_kind,
            },
            "skills": self.skills.iter().map(|entry| json!({
                "name": entry.skill.file_name,
                "path": entry.skill.absolute_path,
                "description": entry.skill.description,
                "enabled": entry.enabled,
                "dependency_ready": entry.dependency_ready,
                "missing_requires": entry.missing_requires,
                "install_hints": entry.skill.openclaw_install,
                "requires": entry.skill.openclaw_requires,
                "prelude_excerpt": entry.skill.prelude_excerpt,
                "code_fence_languages": entry.skill.code_fence_languages,
                "shell_prelude": entry.skill.shell_prelude,
                "root_kind": entry.root_kind,
                "source_root": entry.source_root,
            })).collect::<Vec<_>>(),
        })
    }

    fn root_paths_for_kind(&self, kind: &str) -> Vec<String> {
        self.roots
            .iter()
            .filter(|root| root.kind == kind)
            .map(|root| root.path.clone())
            .collect()
    }
}

// ── Snapshot cache ────────────────────────────────────────────────────────────

/// Reason that triggers a cache invalidation.
#[derive(Clone, Debug)]
pub enum SkillSnapshotInvalidationReason {
    RegistrationChanged,
    CapabilityConfigChanged,
    SkillRootChanged,
    ClawHubInstall,
    ClawHubUpdate,
    ManualReload,
}

/// Lightweight signature for one skill root directory.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SkillRootSignature {
    path: String,
    /// Directory-entry count (number of immediate children).
    entry_count: usize,
    /// Latest modification time (nanoseconds since Unix epoch) of any
    /// immediate child.  Nanosecond precision avoids false cache hits when
    /// multiple edits occur within the same second.
    latest_modified_unix_nanos: u128,
}

impl SkillRootSignature {
    fn from_path(path: &str) -> Self {
        let dir = Path::new(path);
        let mut entry_count = 0usize;
        let mut latest_nanos = 0u128;

        fn mtime_nanos(path: &Path) -> u128 {
            std::fs::metadata(path)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        }

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                entry_count += 1;
                let child_path = entry.path();
                let nanos = mtime_nanos(&child_path);
                if nanos > latest_nanos {
                    latest_nanos = nanos;
                }
                // On Linux, editing a file inside a subdirectory does not update
                // the subdirectory's mtime.  Skills live at root/name/SKILL.md, so
                // also track the SKILL.md mtime one level deeper so that in-place
                // edits are detected without an explicit cache invalidation call.
                if child_path.is_dir() {
                    let skill_md = child_path.join("SKILL.md");
                    let nested_nanos = mtime_nanos(&skill_md);
                    if nested_nanos > latest_nanos {
                        latest_nanos = nested_nanos;
                    }
                }
            }
        }
        Self {
            path: path.to_string(),
            entry_count,
            latest_modified_unix_nanos: latest_nanos,
        }
    }
}

/// Deterministic invalidation key for the snapshot cache.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SkillSnapshotFingerprint {
    root_signatures: Vec<SkillRootSignature>,
    /// Content hash of the registered skill path list.
    registration_signature: u64,
    /// Content hash of `skill_capabilities.json`.
    config_signature: u64,
    /// Signatures for tool directories (tools_dir, embedded_tools_dir, and
    /// registered tool_paths).  dependency_ready is derived from the live tool
    /// inventory under all three sources, so file additions, removals, or
    /// modifications inside those directories must also invalidate the cache.
    tool_dir_signatures: Vec<SkillRootSignature>,
}

fn hash_str_u64(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

impl SkillSnapshotFingerprint {
    fn compute(paths: &PlatformPaths, registrations: &RegisteredPaths) -> Self {
        let roots = collect_skill_roots(paths, registrations);
        let root_signatures = roots
            .iter()
            .map(|root| SkillRootSignature::from_path(&root.path))
            .collect();

        // Include both skill_paths and tool_paths so that a tool-registration
        // change also invalidates the cache (dependency_ready depends on the
        // registered tool set, not just skill roots).
        let registration_input = format!(
            "{}||{}",
            registrations.skill_paths.join("|"),
            registrations.tool_paths.join("|"),
        );
        let registration_signature = hash_str_u64(&registration_input);

        let config_path = paths.config_dir.join(SKILL_CAPABILITIES_CONFIG);
        let config_content = std::fs::read_to_string(&config_path).unwrap_or_default();
        let config_signature = hash_str_u64(&config_content);

        // Build directory-level signatures for every source that registered_tool_names()
        // scans so that file additions, removals, or modifications within those directories
        // also invalidate the snapshot.  registration_signature only covers the path-list
        // itself (detecting new/removed registrations) — it does not detect file changes
        // inside already-registered directories.
        let mut tool_dir_paths: Vec<String> = vec![
            paths.tools_dir.to_string_lossy().to_string(),
            paths.embedded_tools_dir.to_string_lossy().to_string(),
        ];
        tool_dir_paths.extend(registrations.tool_paths.iter().cloned());
        let tool_dir_signatures: Vec<SkillRootSignature> = tool_dir_paths
            .iter()
            .map(|p| SkillRootSignature::from_path(p))
            .collect();

        Self {
            root_signatures,
            registration_signature,
            config_signature,
            tool_dir_signatures,
        }
    }
}

struct SkillSnapshotCache {
    fingerprint: SkillSnapshotFingerprint,
    snapshot: SkillCapabilitySnapshot,
}

/// Process-global snapshot cache.  Protected by a `Mutex` so concurrent
/// callers share one cached value without a separate reference-counted wrapper.
static SNAPSHOT_CACHE: LazyLock<Mutex<Option<SkillSnapshotCache>>> =
    LazyLock::new(|| Mutex::new(None));

/// Explicitly drop the cached snapshot so the next `load_snapshot` call
/// rebuilds it from the filesystem.
pub fn invalidate_snapshot_cache(reason: SkillSnapshotInvalidationReason) {
    log::debug!(
        "[SkillCache] Cache invalidated: {:?}",
        reason
    );
    if let Ok(mut guard) = SNAPSHOT_CACHE.lock() {
        *guard = None;
    }
}

pub fn build_skill_snapshot(
    paths: &PlatformPaths,
    registrations: &RegisteredPaths,
) -> SkillCapabilitySnapshot {
    let config_path = paths.config_dir.join(SKILL_CAPABILITIES_CONFIG);
    let config = load_config(&config_path);
    let disabled = normalized_disabled_skills(&config.disabled_skills);
    let roots = collect_skill_roots(paths, registrations);
    let root_paths = roots
        .iter()
        .map(|root| root.path.as_str())
        .collect::<Vec<_>>();
    let root_kinds = roots
        .iter()
        .map(|root| (root.path.clone(), root.kind.clone()))
        .collect::<BTreeMap<_, _>>();
    let registered_tools = registered_tool_names(paths, registrations);
    let textual_skills = scan_textual_skills_from_roots(&root_paths);
    let mut skills = textual_skills
        .into_iter()
        .map(|skill| {
            let source_root = skill_source_root(&skill).unwrap_or_default();
            let root_kind = root_kinds
                .get(&source_root)
                .cloned()
                .unwrap_or_else(|| "user".to_string());
            let missing_requires =
                missing_dependencies(&skill.openclaw_requires, &registered_tools);
            let dependency_ready = missing_requires.is_empty();
            let enabled = dependency_ready && !disabled.contains(&normalize_skill_name(&skill.file_name));
            SkillCapabilityEntry {
                skill,
                source_root,
                root_kind,
                enabled,
                dependency_ready,
                missing_requires,
            }
        })
        .collect::<Vec<_>>();
    skills.sort_by(|left, right| left.skill.file_name.cmp(&right.skill.file_name));

    SkillCapabilitySnapshot {
        config_path: config_path.to_string_lossy().to_string(),
        disabled_skills: disabled.into_iter().collect(),
        roots,
        skills,
    }
}

pub fn load_snapshot(
    paths: &PlatformPaths,
    registrations: &RegisteredPaths,
) -> SkillCapabilitySnapshot {
    let fingerprint = SkillSnapshotFingerprint::compute(paths, registrations);

    if let Ok(guard) = SNAPSHOT_CACHE.lock() {
        if let Some(cache) = guard.as_ref() {
            if cache.fingerprint == fingerprint {
                log::debug!("[SkillCache] Cache hit — returning cached snapshot");
                return cache.snapshot.clone();
            }
        }
    }

    log::debug!("[SkillCache] Cache miss — rebuilding snapshot from filesystem");
    let snapshot = build_skill_snapshot(paths, registrations);

    if let Ok(mut guard) = SNAPSHOT_CACHE.lock() {
        *guard = Some(SkillSnapshotCache {
            fingerprint,
            snapshot: snapshot.clone(),
        });
    }

    snapshot
}

pub fn top_skills_for_prompt<'a>(
    snapshot: &'a SkillCapabilitySnapshot,
    prompt: &str,
    max_skills: usize,
) -> Vec<&'a TextualSkill> {
    if max_skills == 0 {
        return Vec::new();
    }

    let prompt_lower = prompt.trim().to_ascii_lowercase();
    if prompt_lower.is_empty() {
        return Vec::new();
    }

    let mut ranked = snapshot
        .skills
        .iter()
        .filter(|entry| entry.enabled && entry.dependency_ready)
        .filter_map(|entry| {
            let mut score = 0usize;

            for candidate in entry
                .skill
                .triggers
                .iter()
                .chain(entry.skill.tags.iter())
                .map(|value| value.trim().to_ascii_lowercase())
                .filter(|value| !value.is_empty())
            {
                if prompt_lower.contains(&candidate) {
                    score += 1;
                }
            }

            if score == 0 && !entry.skill.searchable_text.is_empty() {
                for token in prompt_lower.split_whitespace().filter(|token| token.len() >= 3) {
                    if entry.skill.searchable_text.contains(token) {
                        score += 1;
                    }
                }
            }

            (score > 0).then_some((score, entry.skill.file_name.as_str(), &entry.skill))
        })
        .collect::<Vec<_>>();

    ranked.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(right.1)));
    ranked.truncate(max_skills);
    ranked.into_iter().map(|(_, _, skill)| skill).collect()
}

fn load_config(path: &Path) -> SkillCapabilityConfig {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<SkillCapabilityConfig>(&content).ok())
        .unwrap_or_default()
}

fn normalized_disabled_skills(raw: &[String]) -> Vec<String> {
    let mut normalized = BTreeSet::new();
    for value in raw {
        let normalized_name = normalize_skill_name(value);
        if !normalized_name.is_empty() {
            normalized.insert(normalized_name);
        }
    }
    normalized.into_iter().collect()
}

fn collect_skill_roots(paths: &PlatformPaths, registrations: &RegisteredPaths) -> Vec<SkillRoot> {
    let mut roots = Vec::new();
    for root in paths.skill_root_dirs() {
        roots.push(SkillRoot {
            path: root.to_string_lossy().to_string(),
            kind: "user".to_string(),
            external: false,
        });
    }
    for root in paths.skill_hub_root_dirs() {
        roots.push(SkillRoot {
            path: root.to_string_lossy().to_string(),
            kind: "system".to_string(),
            external: false,
        });
    }
    for root in paths.discover_skill_hub_roots() {
        roots.push(SkillRoot {
            path: root.to_string_lossy().to_string(),
            kind: "system".to_string(),
            external: false,
        });
    }
    for root in &registrations.skill_paths {
        roots.push(SkillRoot {
            path: root.clone(),
            kind: "external".to_string(),
            external: true,
        });
    }

    let mut seen = HashSet::new();
    roots.retain(|root| seen.insert(root.path.clone()));
    roots
}

fn skill_source_root(skill: &TextualSkill) -> Option<String> {
    let path = PathBuf::from(&skill.absolute_path);
    let skill_dir = path.parent()?;
    let root = skill_dir.parent()?;
    Some(root.to_string_lossy().to_string())
}

fn missing_dependencies(requires: &[String], registered_tools: &BTreeSet<String>) -> Vec<String> {
    let mut missing = Vec::new();
    for requirement in requires {
        let normalized = normalize_skill_name(requirement);
        if normalized.is_empty() {
            continue;
        }
        if !registered_tools.contains(&normalized) {
            missing.push(requirement.trim().to_string());
        }
    }
    missing.sort();
    missing.dedup();
    missing
}

fn registered_tool_names(paths: &PlatformPaths, registrations: &RegisteredPaths) -> BTreeSet<String> {
    let mut dispatcher = ToolDispatcher::new();
    let mut roots = vec![
        paths.tools_dir.to_string_lossy().to_string(),
        paths.embedded_tools_dir.to_string_lossy().to_string(),
    ];
    roots.extend(registrations.tool_paths.iter().cloned());
    dispatcher.load_tools_from_paths(roots.iter().map(String::as_str));

    dispatcher
        .list()
        .into_iter()
        .map(|tool| normalize_skill_name(&tool.name))
        .filter(|name| !name.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        build_skill_snapshot, invalidate_snapshot_cache, load_snapshot, top_skills_for_prompt,
        SkillCapabilityConfig, SkillSnapshotInvalidationReason, SKILL_CAPABILITIES_CONFIG,
        SNAPSHOT_CACHE,
    };
    use crate::core::registration_store::RegisteredPaths;
    use libtizenclaw_core::framework::paths::PlatformPaths;
    use serde_json::json;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn snapshot_marks_disabled_and_dependency_blocked_skills() {
        let temp = tempdir().unwrap();
        let paths = PlatformPaths::from_base(temp.path().join("runtime"));
        paths.ensure_dirs();

        let managed_skill = paths.skills_dir.join("managed_skill");
        fs::create_dir_all(&managed_skill).unwrap();
        fs::write(
            managed_skill.join("SKILL.md"),
            "---\ndescription: Managed\nmetadata:\n  openclaw:\n    requires:\n      - missing-cmd-for-test\n    install:\n      - sudo apt install missing-cmd-for-test\n---\n# Managed\n",
        )
        .unwrap();

        let external_root = temp.path().join("external-skills");
        fs::create_dir_all(external_root.join("external_skill")).unwrap();
        fs::write(
            external_root.join("external_skill").join("SKILL.md"),
            "---\ndescription: External\n---\n# External\n",
        )
        .unwrap();

        let config = SkillCapabilityConfig {
            disabled_skills: vec!["external skill".to_string()],
        };
        fs::write(
            paths.config_dir.join(SKILL_CAPABILITIES_CONFIG),
            serde_json::to_string_pretty(&config).unwrap(),
        )
        .unwrap();

        let mut registrations = RegisteredPaths::default();
        registrations
            .skill_paths
            .push(external_root.to_string_lossy().to_string());

        let snapshot = load_snapshot(&paths, &registrations);
        assert_eq!(snapshot.disabled_skills, vec!["external_skill"]);
        assert_eq!(snapshot.skills.len(), 2);

        let managed = snapshot.find_skill("managed_skill").unwrap();
        assert!(!managed.dependency_ready);
        assert!(!managed.enabled);
        assert_eq!(managed.root_kind, "user");

        let external = snapshot.find_skill("external skill").unwrap();
        assert!(external.dependency_ready);
        assert!(!external.enabled);
        assert_eq!(external.root_kind, "external");

        let summary = snapshot.summary_json();
        assert_eq!(summary["disabled_count"], 2);
        assert_eq!(summary["external_root_count"], 1);
    }

    #[test]
    fn snapshot_reports_hub_roots_separately() {
        let temp = tempdir().unwrap();
        let paths = PlatformPaths::from_base(temp.path().join("runtime"));
        paths.ensure_dirs();

        let hub_root = paths.skill_hubs_dir.join("openclaw");
        fs::create_dir_all(hub_root.join("battery-helper")).unwrap();
        fs::write(
            hub_root.join("battery-helper").join("SKILL.md"),
            "---\ndescription: Battery helper\n---\n# Skill\n",
        )
        .unwrap();

        let snapshot = load_snapshot(&paths, &RegisteredPaths::default());
        let entry = snapshot.find_skill("battery-helper").unwrap();
        assert_eq!(entry.root_kind, "system");

        let summary = snapshot.summary_json();
        assert_eq!(
            summary["roots"]["managed"][0],
            json!(paths.skills_dir.to_string_lossy().to_string())
        );
        assert_eq!(
            summary["roots"]["user"][0],
            json!(paths.skills_dir.to_string_lossy().to_string())
        );
        assert_eq!(
            summary["roots"]["system"][0],
            json!(paths.skill_hubs_dir.to_string_lossy().to_string())
        );
        assert_eq!(
            summary["roots"]["system"][1],
            json!(hub_root.to_string_lossy().to_string())
        );
    }

    #[test]
    fn snapshot_scans_direct_skills_under_skill_hubs_dir() {
        let temp = tempdir().unwrap();
        let paths = PlatformPaths::from_base(temp.path().join("runtime"));
        paths.ensure_dirs();

        let direct_skill = paths.skill_hubs_dir.join("direct_helper");
        fs::create_dir_all(&direct_skill).unwrap();
        fs::write(
            direct_skill.join("SKILL.md"),
            "---\ndescription: Direct hub helper\nrequires: hub_tool\n---\n# Skill\n",
        )
        .unwrap();
        fs::write(
            paths.tools_dir.join("hub_tool.json"),
            r#"{
  "name": "hub_tool",
  "description": "Hub tool",
  "binary_path": "/bin/echo"
}"#,
        )
        .unwrap();

        let snapshot = build_skill_snapshot(&paths, &RegisteredPaths::default());
        let entry = snapshot.find_skill("direct_helper").unwrap();
        assert_eq!(entry.root_kind, "system");
        assert_eq!(entry.source_root, paths.skill_hubs_dir.to_string_lossy());
        assert!(entry.dependency_ready);
    }

    #[test]
    fn snapshot_marks_dependencies_ready_when_tool_is_registered() {
        let temp = tempdir().unwrap();
        let paths = PlatformPaths::from_base(temp.path().join("runtime"));
        paths.ensure_dirs();

        fs::write(
            paths.tools_dir.join("battery_tool.json"),
            r#"{
  "name": "battery_tool",
  "description": "Battery tool",
  "binary_path": "/bin/echo"
}"#,
        )
        .unwrap();

        let skill_dir = paths.skills_dir.join("get_battery");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: Battery helper\ntriggers:\n  - battery level\nmetadata:\n  openclaw:\n    requires:\n      - battery_tool\n---\n# Battery\n",
        )
        .unwrap();

        let snapshot = build_skill_snapshot(&paths, &RegisteredPaths::default());
        let entry = snapshot.find_skill("get_battery").unwrap();
        assert!(entry.dependency_ready);
        assert!(entry.enabled);
        assert!(entry.missing_requires.is_empty());
    }

    #[test]
    fn snapshot_cache_returns_cached_value_on_second_call() {
        // Clear shared process cache before this test.
        *SNAPSHOT_CACHE.lock().unwrap() = None;

        let temp = tempdir().unwrap();
        let paths = PlatformPaths::from_base(temp.path().join("runtime"));
        paths.ensure_dirs();

        let skill_dir = paths.skills_dir.join("cached_skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: Cache test skill\n---\n# Cache\n",
        )
        .unwrap();

        let registrations = RegisteredPaths::default();

        // First call — populates cache.
        let snap1 = load_snapshot(&paths, &registrations);
        assert_eq!(snap1.skills.len(), 1);

        // Second call with same inputs — must return cached value (same address
        // is not testable, but entry count and name must match).
        let snap2 = load_snapshot(&paths, &registrations);
        assert_eq!(snap2.skills.len(), 1);
        assert_eq!(snap1.skills[0].skill.file_name, snap2.skills[0].skill.file_name);
    }

    #[test]
    fn snapshot_cache_invalidate_forces_rebuild() {
        *SNAPSHOT_CACHE.lock().unwrap() = None;

        let temp = tempdir().unwrap();
        let paths = PlatformPaths::from_base(temp.path().join("runtime"));
        paths.ensure_dirs();

        let skill_dir = paths.skills_dir.join("inv_skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: Invalidation test\n---\n# Inv\n",
        )
        .unwrap();

        let registrations = RegisteredPaths::default();

        // Populate cache.
        let _ = load_snapshot(&paths, &registrations);
        assert!(SNAPSHOT_CACHE.lock().unwrap().is_some());

        // Invalidate.
        invalidate_snapshot_cache(SkillSnapshotInvalidationReason::ManualReload);
        assert!(SNAPSHOT_CACHE.lock().unwrap().is_none());

        // Next load rebuilds.
        let snap = load_snapshot(&paths, &registrations);
        assert_eq!(snap.skills.len(), 1);
        assert!(SNAPSHOT_CACHE.lock().unwrap().is_some());
    }

    #[test]
    fn snapshot_cache_detects_skill_md_edit_without_explicit_invalidation() {
        // Editing SKILL.md inside root/skill_name/SKILL.md must not require an
        // explicit invalidate_snapshot_cache call.  The fingerprint must change
        // automatically because SkillRootSignature tracks SKILL.md mtimes at
        // depth 1 inside each immediate child directory.
        *SNAPSHOT_CACHE.lock().unwrap() = None;

        let temp = tempdir().unwrap();
        let paths = PlatformPaths::from_base(temp.path().join("runtime"));
        paths.ensure_dirs();

        let skill_dir = paths.skills_dir.join("edit_skill");
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_md = skill_dir.join("SKILL.md");
        fs::write(&skill_md, "---\ndescription: Original\n---\n# Edit\n").unwrap();

        let registrations = RegisteredPaths::default();

        // Populate the cache.
        let snap1 = load_snapshot(&paths, &registrations);
        assert_eq!(snap1.skills.len(), 1);
        assert_eq!(
            snap1.skills[0].skill.description,
            "Original",
        );

        // Overwrite SKILL.md — this updates the file's mtime but NOT the
        // parent directory's mtime on Linux.
        // A short sleep ensures the mtime is strictly greater than the cached
        // value; nanosecond fingerprinting makes even a few ms sufficient.
        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::write(&skill_md, "---\ndescription: Updated\n---\n# Edit\n").unwrap();

        // load_snapshot must detect the change via fingerprint comparison.
        let snap2 = load_snapshot(&paths, &registrations);
        assert_eq!(
            snap2.skills[0].skill.description,
            "Updated",
            "load_snapshot must detect SKILL.md edits without an explicit invalidation call"
        );
    }

    #[test]
    fn top_skills_for_prompt_returns_best_matching_skill() {
        let temp = tempdir().unwrap();
        let paths = PlatformPaths::from_base(temp.path().join("runtime"));
        paths.ensure_dirs();

        fs::write(
            paths.tools_dir.join("battery_tool.json"),
            r#"{
  "name": "battery_tool",
  "description": "Battery tool",
  "binary_path": "/bin/echo"
}"#,
        )
        .unwrap();

        let battery_dir = paths.skills_dir.join("get_battery");
        fs::create_dir_all(&battery_dir).unwrap();
        fs::write(
            battery_dir.join("SKILL.md"),
            "---\ndescription: Battery helper\ntags:\n  - battery\ntriggers:\n  - battery level\nmetadata:\n  openclaw:\n    requires:\n      - battery_tool\n---\n# Battery\n",
        )
        .unwrap();

        let wifi_dir = paths.skills_dir.join("get_wifi");
        fs::create_dir_all(&wifi_dir).unwrap();
        fs::write(
            wifi_dir.join("SKILL.md"),
            "---\ndescription: Wifi helper\ntags:\n  - wifi\ntriggers:\n  - wifi status\n---\n# Wifi\n",
        )
        .unwrap();

        let snapshot = build_skill_snapshot(&paths, &RegisteredPaths::default());
        let ranked = top_skills_for_prompt(&snapshot, "What is the battery level?", 2);

        assert_eq!(ranked.len(), 1);
        assert_eq!(ranked[0].file_name, "get_battery");
    }
}
