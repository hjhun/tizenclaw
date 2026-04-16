//! ClawHub skill hub client.
//!
//! Provides install, search, and list operations against the ClawHub public registry
//! at <https://clawhub.ai>.  Skills are extracted into the runtime
//! `workspace/skill-hubs/clawhub/<slug>/` directory and are picked up automatically
//! by the next `skill_capability_manager::load_snapshot` call.
//!
//! The lock file at `workspace/.clawhub/lock.json` tracks installed skills so a
//! future update command can re-fetch from the same registry.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_CLAWHUB_BASE_URL: &str = "https://clawhub.ai";
const REQUEST_TIMEOUT_SECS: u64 = 30;
const LOCK_SUBPATH: &str = ".clawhub/lock.json";

// ── Lock file types ──────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClawHubLockEntry {
    pub slug: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub install_path: String,
    pub installed_at_secs: u64,
    /// Source registry kind.  Always `"clawhub"` for registry-managed skills.
    /// Optional so existing lock entries without this field remain readable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<String>,
    /// Base URL of the registry that the skill was installed from.
    /// Defaults to `DEFAULT_CLAWHUB_BASE_URL` when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_base_url: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ClawHubLock {
    #[serde(default)]
    pub skills: Vec<ClawHubLockEntry>,
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Search ClawHub for skills matching `query`.
///
/// Returns the raw registry JSON response so the caller can surface it directly
/// via IPC without having to re-serialize an intermediate struct.
pub async fn clawhub_search(query: &str) -> Result<Value, String> {
    let url = format!("{}/api/v1/search", resolve_base_url());
    let client = build_client()?;
    let resp = client
        .get(&url)
        .query(&[("q", query), ("limit", "20")])
        .send()
        .await
        .map_err(|err| format!("ClawHub search request failed: {}", err))?;

    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|err| format!("ClawHub search body read failed: {}", err))?;
    if !status.is_success() {
        return Err(format!("ClawHub search failed ({}): {}", status, body));
    }
    serde_json::from_str::<Value>(&body)
        .map_err(|err| format!("ClawHub search parse failed: {}", err))
}

/// Download and install a skill from ClawHub into the runtime skill-hubs tree.
///
/// `source` may be either `clawhub:<slug>` or just `<slug>`.
/// The skill is extracted to `skill_hubs_dir/clawhub/<slug>/` and the lock file
/// is written to `skill_hubs_dir/../.clawhub/lock.json`.
pub async fn clawhub_install(skill_hubs_dir: &Path, source: &str) -> Result<Value, String> {
    let slug = parse_clawhub_slug(source)?;
    let base_url = resolve_base_url();
    let client = build_client()?;

    // Fetch skill metadata to get display name and current version.
    let detail_url = format!("{}/api/v1/skills/{}", base_url, slug);
    let detail_resp = client
        .get(&detail_url)
        .send()
        .await
        .map_err(|err| format!("ClawHub skill detail request failed: {}", err))?;
    let detail_status = detail_resp.status();
    let detail_body = detail_resp
        .text()
        .await
        .unwrap_or_else(|_| String::new());
    if !detail_status.is_success() {
        return Err(format!(
            "ClawHub skill '{}' not found ({}): {}",
            slug, detail_status, detail_body
        ));
    }
    let detail: Value = serde_json::from_str(&detail_body)
        .map_err(|err| format!("ClawHub skill detail parse failed: {}", err))?;

    let display_name = detail
        .pointer("/skill/displayName")
        .and_then(Value::as_str)
        .unwrap_or(&slug)
        .to_string();
    let version = detail
        .pointer("/latestVersion/version")
        .and_then(Value::as_str)
        .map(ToString::to_string);

    // Download the zip archive.
    let download_url = format!("{}/api/v1/download", base_url);
    let download_resp = client
        .get(&download_url)
        .query(&[("slug", slug.as_str())])
        .send()
        .await
        .map_err(|err| format!("ClawHub download request failed: {}", err))?;
    let download_status = download_resp.status();
    if !download_status.is_success() {
        let err_body = download_resp.text().await.unwrap_or_default();
        return Err(format!(
            "ClawHub download for '{}' failed ({}): {}",
            slug, download_status, err_body
        ));
    }
    let archive_bytes = download_resp
        .bytes()
        .await
        .map_err(|err| format!("ClawHub archive read failed: {}", err))?;

    // Extract into a staging directory first, then atomically replace the
    // final install path.  This prevents retries or concurrent updates from
    // leaving a partially-extracted skill in place.
    let install_dir = skill_hubs_dir.join("clawhub").join(&slug);
    let staging_dir = skill_hubs_dir
        .join("clawhub")
        .join(format!("{}.__installing__", slug));

    // Remove any leftover staging directory from a previous failed attempt.
    if staging_dir.exists() {
        std::fs::remove_dir_all(&staging_dir).map_err(|err| {
            format!(
                "Failed to remove stale staging directory '{}': {}",
                staging_dir.display(),
                err
            )
        })?;
    }
    std::fs::create_dir_all(&staging_dir).map_err(|err| {
        format!(
            "Failed to create staging directory '{}': {}",
            staging_dir.display(),
            err
        )
    })?;
    extract_zip_archive(&archive_bytes, &staging_dir, &slug)?;

    // Reject archives that do not contain a SKILL.md — the skill scanner
    // only recognises directories that have this file, so recording a
    // malformed archive in the lock file would produce an unusable install.
    validate_extracted_skill(&staging_dir, &slug).map_err(|err| {
        let _ = std::fs::remove_dir_all(&staging_dir);
        err
    })?;

    // Replace the live install directory with the staging directory using a
    // backup-and-restore pattern so a rename failure never discards the
    // previously-working install.
    let backup_dir = skill_hubs_dir
        .join("clawhub")
        .join(format!("{}.__backup__", slug));
    atomic_replace_dir(&staging_dir, &install_dir, &backup_dir)?;

    // Record the install in the lock file.
    let workspace_dir = skill_hubs_dir
        .parent()
        .unwrap_or(skill_hubs_dir);
    update_lock_file(
        workspace_dir,
        &slug,
        &display_name,
        version.as_deref(),
        &install_dir,
        None,
    )?;

    Ok(json!({
        "status": "installed",
        "slug": slug,
        "display_name": display_name,
        "version": version,
        "install_path": install_dir.to_string_lossy().as_ref(),
    }))
}

/// List skills recorded in the ClawHub lock file.
pub fn clawhub_list(skill_hubs_dir: &Path) -> Result<Value, String> {
    let workspace_dir = skill_hubs_dir
        .parent()
        .unwrap_or(skill_hubs_dir);
    let lock_path = workspace_dir.join(LOCK_SUBPATH);
    let lock = load_lock_file(&lock_path);
    Ok(json!({
        "skills": lock.skills,
        "lock_path": lock_path.to_string_lossy().as_ref(),
    }))
}

/// Re-install all skills tracked by the lock file using the recorded source.
///
/// Skills without a `source_base_url` default to `DEFAULT_CLAWHUB_BASE_URL`.
/// A failure for one skill does not abort the rest; each entry is classified as
/// `updated`, `skipped`, or `failed` in the result.
pub async fn clawhub_update(skill_hubs_dir: &Path) -> Result<Value, String> {
    let workspace_dir = skill_hubs_dir.parent().unwrap_or(skill_hubs_dir);
    let lock_path = workspace_dir.join(LOCK_SUBPATH);
    let lock = load_lock_file(&lock_path);

    if lock.skills.is_empty() {
        return Ok(json!({
            "updated": [],
            "skipped": [{"slug": "__empty__", "status": "skipped", "detail": "lock file has no tracked skills"}],
            "failed": [],
            "lock_path": lock_path.to_string_lossy().as_ref(),
        }));
    }

    let client = build_client()?;
    let mut updated = Vec::new();
    let mut skipped: Vec<Value> = Vec::new();
    let mut failed: Vec<Value> = Vec::new();

    for entry in &lock.skills {
        let slug = &entry.slug;
        let base_url = entry
            .source_base_url
            .as_deref()
            .unwrap_or(DEFAULT_CLAWHUB_BASE_URL)
            .trim_end_matches('/')
            .to_string();

        match update_one_skill(skill_hubs_dir, entry, &client, &base_url).await {
            Ok(outcome) => {
                if outcome["status"] == "updated" {
                    updated.push(outcome);
                } else {
                    skipped.push(outcome);
                }
            }
            Err(detail) => {
                log::warn!("ClawHub update failed for '{}': {}", slug, detail);
                failed.push(json!({
                    "slug": slug,
                    "display_name": entry.display_name,
                    "status": "failed",
                    "detail": detail,
                }));
            }
        }
    }

    Ok(json!({
        "updated": updated,
        "skipped": skipped,
        "failed": failed,
        "lock_path": lock_path.to_string_lossy().as_ref(),
    }))
}

/// Re-install one skill using its lock-file source identity.
async fn update_one_skill(
    skill_hubs_dir: &Path,
    entry: &ClawHubLockEntry,
    client: &reqwest::Client,
    base_url: &str,
) -> Result<Value, String> {
    let slug = &entry.slug;
    let previous_version = entry.version.clone();

    // Fetch current metadata to get the latest version.
    let detail_url = format!("{}/api/v1/skills/{}", base_url, slug);
    let detail_resp = client
        .get(&detail_url)
        .send()
        .await
        .map_err(|err| format!("ClawHub detail request failed for '{}': {}", slug, err))?;
    let detail_status = detail_resp.status();
    let detail_body = detail_resp.text().await.unwrap_or_default();
    if !detail_status.is_success() {
        return Err(format!(
            "ClawHub skill '{}' metadata fetch failed ({}): {}",
            slug, detail_status, detail_body
        ));
    }
    let detail: Value = serde_json::from_str(&detail_body)
        .map_err(|err| format!("ClawHub detail parse failed for '{}': {}", slug, err))?;

    let display_name = detail
        .pointer("/skill/displayName")
        .and_then(Value::as_str)
        .unwrap_or(slug)
        .to_string();
    let new_version = detail
        .pointer("/latestVersion/version")
        .and_then(Value::as_str)
        .map(ToString::to_string);

    // Download the archive.
    let download_url = format!("{}/api/v1/download", base_url);
    let download_resp = client
        .get(&download_url)
        .query(&[("slug", slug.as_str())])
        .send()
        .await
        .map_err(|err| format!("ClawHub download failed for '{}': {}", slug, err))?;
    let download_status = download_resp.status();
    if !download_status.is_success() {
        let err_body = download_resp.text().await.unwrap_or_default();
        return Err(format!(
            "ClawHub download for '{}' failed ({}): {}",
            slug, download_status, err_body
        ));
    }
    let archive_bytes = download_resp
        .bytes()
        .await
        .map_err(|err| format!("ClawHub archive read failed for '{}': {}", slug, err))?;

    // Extract to staging, validate, then atomically replace live install.
    let install_dir = skill_hubs_dir.join("clawhub").join(slug);
    let staging_dir = skill_hubs_dir
        .join("clawhub")
        .join(format!("{}.__installing__", slug));
    let backup_dir = skill_hubs_dir
        .join("clawhub")
        .join(format!("{}.__backup__", slug));

    if staging_dir.exists() {
        std::fs::remove_dir_all(&staging_dir).map_err(|err| {
            format!(
                "Failed to remove stale staging dir for '{}': {}",
                slug, err
            )
        })?;
    }
    std::fs::create_dir_all(&staging_dir).map_err(|err| {
        format!("Failed to create staging dir for '{}': {}", slug, err)
    })?;

    extract_zip_archive(&archive_bytes, &staging_dir, slug).map_err(|err| {
        let _ = std::fs::remove_dir_all(&staging_dir);
        err
    })?;
    validate_extracted_skill(&staging_dir, slug).map_err(|err| {
        let _ = std::fs::remove_dir_all(&staging_dir);
        err
    })?;
    atomic_replace_dir(&staging_dir, &install_dir, &backup_dir)?;

    // Update lock entry, preserving the recorded source URL so a routine
    // update does not silently migrate the skill to a different registry.
    let workspace_dir = skill_hubs_dir.parent().unwrap_or(skill_hubs_dir);
    update_lock_file(
        workspace_dir,
        slug,
        &display_name,
        new_version.as_deref(),
        &install_dir,
        Some(base_url),
    )?;

    Ok(json!({
        "slug": slug,
        "display_name": display_name,
        "status": "updated",
        "previous_version": previous_version,
        "current_version": new_version,
    }))
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Verify that a freshly-extracted staging directory contains a `SKILL.md`
/// file.  The textual skill scanner only recognises directories that have this
/// file, so we must reject malformed archives before they reach the lock file.
fn validate_extracted_skill(dir: &Path, slug: &str) -> Result<(), String> {
    let skill_md = dir.join("SKILL.md");
    if !skill_md.is_file() {
        return Err(format!(
            "ClawHub archive for '{}' does not contain a SKILL.md file; \
             the install was rejected.",
            slug
        ));
    }
    Ok(())
}

/// Replace `install` with `staging` while keeping `backup` as a safety net.
///
/// Sequence:
/// 1. Remove any stale `backup` from a previous incomplete run.
/// 2. Rename the current `install` to `backup` (if it exists).
/// 3. Rename `staging` to `install`.
/// 4. On step-3 success: remove `backup`.
/// 5. On step-3 failure: restore `backup` → `install` before returning the
///    error.  This guarantees the previously-working skill survives a failed
///    update.
fn atomic_replace_dir(staging: &Path, install: &Path, backup: &Path) -> Result<(), String> {
    // Remove stale backup from any previous incomplete run.
    if backup.exists() {
        std::fs::remove_dir_all(backup).map_err(|err| {
            format!(
                "Failed to remove stale backup directory '{}': {}",
                backup.display(),
                err
            )
        })?;
    }

    // Move the live install to the backup slot (if one exists).
    if install.exists() {
        std::fs::rename(install, backup).map_err(|err| {
            format!(
                "Failed to back up existing install directory '{}': {}",
                install.display(),
                err
            )
        })?;
    }

    // Promote staging to the live install slot.
    if let Err(err) = std::fs::rename(staging, install) {
        // Restore the backup so the caller still has a working skill.
        if backup.exists() {
            let _ = std::fs::rename(backup, install);
        }
        return Err(format!(
            "Failed to move staging directory to '{}': {}",
            install.display(),
            err
        ));
    }

    // Discard the backup now that the new version is live.
    if backup.exists() {
        let _ = std::fs::remove_dir_all(backup);
    }

    Ok(())
}

fn resolve_base_url() -> String {
    std::env::var("TIZENCLAW_CLAWHUB_URL")
        .or_else(|_| std::env::var("CLAWHUB_URL"))
        .unwrap_or_else(|_| DEFAULT_CLAWHUB_BASE_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn build_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|err| format!("Failed to build HTTP client: {}", err))
}

fn parse_clawhub_slug(source: &str) -> Result<String, String> {
    let trimmed = source.trim();
    let slug = if let Some(rest) = trimmed.strip_prefix("clawhub:") {
        rest.trim()
    } else {
        trimmed
    };
    if slug.is_empty() {
        return Err("ClawHub skill slug cannot be empty.".to_string());
    }
    // Basic sanity check: slugs are lowercase alphanumeric with hyphens.
    if slug
        .chars()
        .any(|char| !char.is_alphanumeric() && char != '-' && char != '_' && char != '.')
    {
        return Err(format!(
            "Invalid ClawHub slug '{}': only alphanumeric characters, hyphens, underscores, and dots are allowed.",
            slug
        ));
    }
    Ok(slug.to_string())
}

fn extract_zip_archive(bytes: &[u8], dest_dir: &Path, slug: &str) -> Result<(), String> {
    use std::io::Cursor;

    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|err| format!("Failed to open zip archive: {}", err))?;

    // Determine whether all entries share a common prefix that matches the slug
    // (e.g., `<slug>/SKILL.md`).  If so, strip it when writing.
    let prefix = format!("{}/", slug);
    let all_have_prefix = (0..archive.len()).all(|index| {
        archive
            .by_index(index)
            .map(|entry| {
                let name = entry.name().to_string();
                name.starts_with(&prefix) || name == slug
            })
            .unwrap_or(false)
    });

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| format!("Failed to read archive entry {}: {}", index, err))?;

        if entry.is_dir() {
            continue;
        }

        let raw_name = entry.name().to_string();
        let relative = if all_have_prefix {
            raw_name
                .strip_prefix(&prefix)
                .unwrap_or(&raw_name)
        } else {
            raw_name.as_str()
        };

        if relative.is_empty() {
            continue;
        }

        // Reject path-traversal and absolute-path entries.
        // Checking just for ".." misses absolute entries like "/etc/passwd"
        // which Path::join() would treat as a new root. We inspect every
        // component individually and also verify the final path stays inside
        // dest_dir as a defense-in-depth guard.
        {
            use std::path::Component;
            let unsafe_component = Path::new(relative).components().any(|c| {
                matches!(
                    c,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            });
            if unsafe_component {
                log::warn!(
                    "ClawHub: skipping unsafe archive entry '{}'",
                    raw_name
                );
                continue;
            }
        }

        let out_path = dest_dir.join(relative);

        // Defense-in-depth: after joining, confirm the path is still rooted
        // inside dest_dir (catches any edge case the component check missed).
        if !out_path.starts_with(dest_dir) {
            log::warn!(
                "ClawHub: skipping archive entry '{}' that would escape install dir",
                raw_name
            );
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "Failed to create directory '{}': {}",
                    parent.display(),
                    err
                )
            })?;
        }

        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|err| format!("Failed to read archive entry '{}': {}", raw_name, err))?;

        let mut out_file = std::fs::File::create(&out_path).map_err(|err| {
            format!("Failed to create '{}': {}", out_path.display(), err)
        })?;
        out_file.write_all(&buf).map_err(|err| {
            format!("Failed to write '{}': {}", out_path.display(), err)
        })?;
    }

    Ok(())
}

fn load_lock_file(path: &Path) -> ClawHubLock {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<ClawHubLock>(&content).ok())
        .unwrap_or_default()
}

/// Write or upsert a skill entry in the lock file.
///
/// `source_base_url_override` controls which registry URL is recorded:
/// - `Some(url)` preserves the caller-supplied URL verbatim (used by the
///   update path so the original install source is not silently migrated).
/// - `None` calls `resolve_base_url()` to derive the URL from the current
///   environment (used by the install path for new entries).
fn update_lock_file(
    workspace_dir: &Path,
    slug: &str,
    display_name: &str,
    version: Option<&str>,
    install_dir: &Path,
    source_base_url_override: Option<&str>,
) -> Result<(), String> {
    let lock_path = workspace_dir.join(LOCK_SUBPATH);
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            format!("Failed to create lock directory '{}': {}", parent.display(), err)
        })?;
    }

    let mut lock = load_lock_file(&lock_path);
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let base_url = source_base_url_override
        .map(ToString::to_string)
        .unwrap_or_else(resolve_base_url);
    if let Some(entry) = lock.skills.iter_mut().find(|entry| entry.slug == slug) {
        entry.display_name = display_name.to_string();
        entry.version = version.map(ToString::to_string);
        entry.install_path = install_dir.to_string_lossy().to_string();
        entry.installed_at_secs = now_secs;
        entry.source_kind = Some("clawhub".to_string());
        entry.source_base_url = Some(base_url.clone());
    } else {
        lock.skills.push(ClawHubLockEntry {
            slug: slug.to_string(),
            display_name: display_name.to_string(),
            version: version.map(ToString::to_string),
            install_path: install_dir.to_string_lossy().to_string(),
            installed_at_secs: now_secs,
            source_kind: Some("clawhub".to_string()),
            source_base_url: Some(base_url),
        });
    }

    let serialized = serde_json::to_string_pretty(&lock)
        .map_err(|err| format!("Failed to serialize lock file: {}", err))?;
    std::fs::write(&lock_path, &serialized).map_err(|err| {
        format!(
            "Failed to write lock file '{}': {}",
            lock_path.display(),
            err
        )
    })?;

    Ok(())
}

// ── Path helper ──────────────────────────────────────────────────────────────

/// Return the `workspace/skill-hubs` path from a `PlatformPaths` reference.
pub fn skill_hubs_dir_from_paths(
    paths: &libtizenclaw_core::framework::paths::PlatformPaths,
) -> PathBuf {
    paths.skill_hubs_dir.clone()
}

#[cfg(test)]
mod tests {
    use super::{
        extract_zip_archive, load_lock_file, parse_clawhub_slug, update_lock_file, ClawHubLock,
        ClawHubLockEntry,
    };
    use std::io::Write as _;
    use tempfile::tempdir;

    // Build an in-memory zip archive for use in extraction tests.
    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let buf = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(buf);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, data) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn parse_clawhub_slug_strips_clawhub_prefix() {
        assert_eq!(
            parse_clawhub_slug("clawhub:battery-helper").unwrap(),
            "battery-helper"
        );
        assert_eq!(
            parse_clawhub_slug("battery-helper").unwrap(),
            "battery-helper"
        );
    }

    #[test]
    fn parse_clawhub_slug_rejects_empty() {
        assert!(parse_clawhub_slug("clawhub:").is_err());
        assert!(parse_clawhub_slug("").is_err());
    }

    #[test]
    fn update_and_load_lock_file_round_trips() {
        let dir = tempdir().unwrap();
        update_lock_file(
            dir.path(),
            "test-skill",
            "Test Skill",
            Some("1.0.0"),
            &dir.path().join("skill-hubs/clawhub/test-skill"),
            None,
        )
        .unwrap();

        let lock = load_lock_file(&dir.path().join(".clawhub/lock.json"));
        assert_eq!(lock.skills.len(), 1);
        assert_eq!(lock.skills[0].slug, "test-skill");
        assert_eq!(lock.skills[0].version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn update_lock_file_upserts_existing_entry() {
        let dir = tempdir().unwrap();
        let install_dir = dir.path().join("skill-hubs/clawhub/test-skill");

        update_lock_file(dir.path(), "test-skill", "Test Skill", Some("1.0.0"), &install_dir, None)
            .unwrap();
        update_lock_file(dir.path(), "test-skill", "Test Skill", Some("1.1.0"), &install_dir, None)
            .unwrap();

        let lock = load_lock_file(&dir.path().join(".clawhub/lock.json"));
        assert_eq!(lock.skills.len(), 1);
        assert_eq!(lock.skills[0].version.as_deref(), Some("1.1.0"));
    }

    #[test]
    fn clawhub_list_returns_empty_when_no_lock_file() {
        let dir = tempdir().unwrap();
        let skill_hubs_dir = dir.path().join("workspace/skill-hubs");
        std::fs::create_dir_all(&skill_hubs_dir).unwrap();
        let result = super::clawhub_list(&skill_hubs_dir).unwrap();
        let skills = result["skills"].as_array().unwrap();
        assert!(skills.is_empty());
    }

    #[test]
    fn extract_zip_archive_writes_files_to_dest() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("skill");
        std::fs::create_dir_all(&dest).unwrap();

        let archive = build_zip(&[
            ("SKILL.md", b"# Test skill"),
            ("lib/helper.sh", b"#!/bin/sh\necho ok"),
        ]);

        extract_zip_archive(&archive, &dest, "test-skill").unwrap();

        assert!(dest.join("SKILL.md").exists());
        assert!(dest.join("lib/helper.sh").exists());
        assert_eq!(
            std::fs::read_to_string(dest.join("SKILL.md")).unwrap(),
            "# Test skill"
        );
    }

    #[test]
    fn extract_zip_archive_strips_slug_prefix_when_present() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("skill");
        std::fs::create_dir_all(&dest).unwrap();

        // Archive contains entries prefixed with the slug, as GitHub releases do.
        let archive = build_zip(&[
            ("my-skill/SKILL.md", b"# prefixed"),
            ("my-skill/lib/tool.sh", b"#!/bin/sh"),
        ]);

        extract_zip_archive(&archive, &dest, "my-skill").unwrap();

        // Prefix must be stripped: files land directly in dest, not dest/my-skill/.
        assert!(dest.join("SKILL.md").exists());
        assert!(dest.join("lib/tool.sh").exists());
        assert!(!dest.join("my-skill").exists());
    }

    #[test]
    fn extract_zip_archive_rejects_path_traversal() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("skill");
        std::fs::create_dir_all(&dest).unwrap();

        // A malicious entry that would escape dest via "..".
        let archive = build_zip(&[
            ("SKILL.md", b"safe"),
            ("../escape.txt", b"should not land outside dest"),
        ]);

        // extract_zip_archive must not error — it silently skips unsafe entries.
        extract_zip_archive(&archive, &dest, "test-skill").unwrap();

        // Safe entry must be written.
        assert!(dest.join("SKILL.md").exists());
        // Traversal target must not be created outside dest.
        assert!(!dir.path().join("escape.txt").exists());
    }

    #[test]
    fn extract_zip_archive_rejects_absolute_paths() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("skill");
        std::fs::create_dir_all(&dest).unwrap();

        let archive = build_zip(&[
            ("SKILL.md", b"safe"),
            ("/etc/passwd", b"should not overwrite"),
        ]);

        extract_zip_archive(&archive, &dest, "test-skill").unwrap();

        assert!(dest.join("SKILL.md").exists());
        // Absolute path entry must not create a file rooted at dest via join().
        assert!(!dest.join("etc/passwd").exists());
    }

    #[test]
    fn validate_extracted_skill_accepts_dir_with_skill_md() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("SKILL.md"), b"# ok").unwrap();
        assert!(super::validate_extracted_skill(dir.path(), "my-skill").is_ok());
    }

    #[test]
    fn validate_extracted_skill_rejects_dir_without_skill_md() {
        let dir = tempdir().unwrap();
        // No SKILL.md written — must be rejected.
        let err = super::validate_extracted_skill(dir.path(), "my-skill").unwrap_err();
        assert!(err.contains("SKILL.md"), "error should mention SKILL.md: {}", err);
    }

    #[test]
    fn atomic_replace_dir_installs_fresh_skill() {
        // Happy path: no existing install; staging is promoted cleanly.
        let dir = tempdir().unwrap();
        let staging = dir.path().join("staging");
        let install = dir.path().join("install");
        let backup = dir.path().join("backup");

        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(staging.join("SKILL.md"), b"# installed").unwrap();

        super::atomic_replace_dir(&staging, &install, &backup).unwrap();

        assert!(install.join("SKILL.md").exists());
        assert!(!staging.exists());
        assert!(!backup.exists());
    }

    #[test]
    fn atomic_replace_dir_preserves_existing_on_rename_failure() {
        // Regression guard: if staging does not exist (simulating a rename
        // failure), the previously-working install must be restored from backup.
        let dir = tempdir().unwrap();
        let staging = dir.path().join("staging"); // intentionally absent
        let install = dir.path().join("install");
        let backup = dir.path().join("backup");

        // Create the "old" working install.
        std::fs::create_dir_all(&install).unwrap();
        std::fs::write(install.join("SKILL.md"), b"old version").unwrap();

        let result = super::atomic_replace_dir(&staging, &install, &backup);
        assert!(result.is_err(), "should fail when staging is absent");

        // The old install must have been restored so the skill stays usable.
        assert!(
            install.join("SKILL.md").exists(),
            "old install must be restored after failure"
        );
        assert_eq!(
            std::fs::read_to_string(install.join("SKILL.md")).unwrap(),
            "old version"
        );
        // Backup slot must be cleaned up or restored — not left dangling.
        assert!(!backup.exists(), "backup must not be left dangling");
    }

    #[test]
    fn atomic_replace_dir_removes_stale_backup() {
        // If a stale backup exists from a previous failed run, it must be
        // removed before the new backup is created.
        let dir = tempdir().unwrap();
        let staging = dir.path().join("staging");
        let install = dir.path().join("install");
        let backup = dir.path().join("backup");

        // Stale backup from a previous incomplete run.
        std::fs::create_dir_all(&backup).unwrap();
        std::fs::write(backup.join("stale.txt"), b"stale").unwrap();

        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(staging.join("SKILL.md"), b"new version").unwrap();

        super::atomic_replace_dir(&staging, &install, &backup).unwrap();

        assert!(install.join("SKILL.md").exists());
        assert!(!backup.exists());
    }

    #[test]
    fn update_lock_file_writes_source_fields() {
        let dir = tempdir().unwrap();
        let install_dir = dir.path().join("skill-hubs/clawhub/my-skill");
        update_lock_file(dir.path(), "my-skill", "My Skill", Some("1.0.0"), &install_dir, None).unwrap();
        let lock = load_lock_file(&dir.path().join(".clawhub/lock.json"));
        assert_eq!(lock.skills.len(), 1);
        let entry = &lock.skills[0];
        assert!(entry.source_kind.as_deref() == Some("clawhub"), "source_kind must be set");
        assert!(entry.source_base_url.is_some(), "source_base_url must be set");
    }

    #[test]
    fn lock_entry_with_legacy_fields_deserializes_ok() {
        // Lock entries written before the source fields were added must still
        // deserialize without error.
        let json = r#"{
            "skills": [{
                "slug": "old-skill",
                "display_name": "Old Skill",
                "version": "0.9.0",
                "install_path": "/some/path",
                "installed_at_secs": 0
            }]
        }"#;
        let lock: ClawHubLock = serde_json::from_str(json).unwrap();
        assert_eq!(lock.skills.len(), 1);
        assert!(lock.skills[0].source_kind.is_none());
        assert!(lock.skills[0].source_base_url.is_none());
    }

    #[test]
    fn clawhub_update_returns_skipped_for_empty_lock() {
        // When the lock file has no tracked skills, update must return a
        // success response with an explicit skipped entry — not an error.
        let dir = tempdir().unwrap();
        let skill_hubs_dir = dir.path().join("workspace/skill-hubs");
        std::fs::create_dir_all(&skill_hubs_dir).unwrap();

        // No lock file exists — clawhub_update should handle the empty case.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = rt
            .block_on(super::clawhub_update(&skill_hubs_dir))
            .unwrap();
        let skipped = result["skipped"].as_array().unwrap();
        assert!(!skipped.is_empty(), "must report at least one skipped entry for empty lock");
        assert!(result["failed"].as_array().unwrap().is_empty());
        assert!(result["updated"].as_array().unwrap().is_empty());
    }
}
