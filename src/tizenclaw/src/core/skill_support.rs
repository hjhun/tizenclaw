use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_SKILL_REFERENCE_DOC: &str = "SKILL_BEST_PRACTICE.md";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillReferenceDoc {
    pub name: String,
    pub absolute_path: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedSkillDocument {
    pub normalized_name: String,
    pub document: String,
    pub warnings: Vec<String>,
}

pub fn normalize_skill_name(name: &str) -> Result<String, String> {
    let mut normalized = String::new();
    let mut last_was_hyphen = false;

    for ch in name.trim().to_lowercase().chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            normalized.push(ch);
            last_was_hyphen = false;
        } else if (ch == '-' || ch == '_' || ch.is_ascii_whitespace()) && !normalized.is_empty()
            && !last_was_hyphen {
            normalized.push('-');
            last_was_hyphen = true;
        }
    }

    let normalized = normalized.trim_matches('-').to_string();
    if normalized.is_empty() {
        return Err(
            "Skill name must contain lowercase letters, numbers, or hyphens after normalization."
                .into(),
        );
    }
    if normalized.len() > 64 {
        return Err("Skill name must be 64 characters or fewer.".into());
    }
    if normalized.contains("anthropic") || normalized.contains("claude") {
        return Err("Skill name cannot contain reserved words 'anthropic' or 'claude'.".into());
    }

    Ok(normalized)
}

pub fn validate_description(description: &str) -> Result<String, String> {
    let normalized = description.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return Err("Skill description must be non-empty.".into());
    }
    if normalized.len() > 1024 {
        return Err("Skill description must be 1024 characters or fewer.".into());
    }
    if normalized.contains('<') || normalized.contains('>') {
        return Err("Skill description cannot contain XML tags.".into());
    }
    Ok(normalized)
}

pub fn strip_frontmatter(content: &str) -> &str {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---\n") && trimmed != "---" {
        return content.trim();
    }

    let mut lines = trimmed.lines();
    if lines.next() != Some("---") {
        return content.trim();
    }

    let mut byte_offset = 4usize;
    for line in lines {
        byte_offset += line.len() + 1;
        if line.trim() == "---" {
            return trimmed[byte_offset..].trim();
        }
    }

    content.trim()
}

pub fn prepare_skill_document(
    name: &str,
    description: &str,
    content: &str,
) -> Result<PreparedSkillDocument, String> {
    let normalized_name = normalize_skill_name(name)?;
    let normalized_description = validate_description(description)?;
    let body = strip_frontmatter(content);

    if body.is_empty() {
        return Err("Skill content body must not be empty.".into());
    }

    let mut warnings = Vec::new();
    if name.trim() != normalized_name {
        warnings.push(format!(
            "Normalized skill name from '{}' to '{}'.",
            name.trim(),
            normalized_name
        ));
    }

    let document = format!(
        "---\nname: {}\ndescription: {}\n---\n\n{}\n",
        normalized_name,
        yaml_quote(&normalized_description),
        body
    );

    Ok(PreparedSkillDocument {
        normalized_name,
        document,
        warnings,
    })
}

pub fn list_skill_reference_docs(docs_dir: &Path) -> Vec<SkillReferenceDoc> {
    let mut docs = Vec::new();
    let entries = match fs::read_dir(docs_dir) {
        Ok(entries) => entries,
        Err(_) => return docs,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|v| v.to_str()).unwrap_or_default();
        if !ext.eq_ignore_ascii_case("md") {
            continue;
        }

        let name = match path.file_name().and_then(|v| v.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };
        let content = fs::read_to_string(&path).unwrap_or_default();
        let description = extract_reference_description(&path, &content);

        docs.push(SkillReferenceDoc {
            name,
            absolute_path: path.to_string_lossy().to_string(),
            description,
        });
    }

    docs.sort_by(|left, right| left.name.cmp(&right.name));
    docs
}

pub fn read_skill_reference_doc(
    docs_dir: &Path,
    requested_name: &str,
) -> Result<SkillReferenceDoc, String> {
    let path = resolve_reference_doc_path(docs_dir, requested_name).ok_or_else(|| {
        format!(
            "Skill reference '{}' was not found under {:?}.",
            requested_name, docs_dir
        )
    })?;

    let content = fs::read_to_string(&path)
        .map_err(|err| format!("Failed to read skill reference '{}': {}", requested_name, err))?;
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or(DEFAULT_SKILL_REFERENCE_DOC)
        .to_string();

    Ok(SkillReferenceDoc {
        name,
        absolute_path: path.to_string_lossy().to_string(),
        description: content,
    })
}

fn resolve_reference_doc_path(docs_dir: &Path, requested_name: &str) -> Option<PathBuf> {
    let requested = requested_name.trim();
    if requested.is_empty() {
        let default_path = docs_dir.join(DEFAULT_SKILL_REFERENCE_DOC);
        return default_path.exists().then_some(default_path);
    }

    let candidates = fs::read_dir(docs_dir).ok()?;
    for entry in candidates.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = match path.file_name().and_then(|v| v.to_str()) {
            Some(v) => v,
            None => continue,
        };
        let stem = path.file_stem().and_then(|v| v.to_str()).unwrap_or_default();
        if file_name.eq_ignore_ascii_case(requested) || stem.eq_ignore_ascii_case(requested) {
            return Some(path);
        }
    }

    None
}

fn extract_reference_description(path: &Path, content: &str) -> String {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix("# ") {
            if !title.trim().is_empty() {
                return title.trim().to_string();
            }
        }
    }

    path.file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or(DEFAULT_SKILL_REFERENCE_DOC)
        .replace('_', " ")
}

fn yaml_quote(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ");
    format!("\"{}\"", escaped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_skill_name_converts_to_anthropic_style() {
        let normalized = normalize_skill_name(" Battery Helper_v2 ").unwrap();
        assert_eq!(normalized, "battery-helper-v2");
    }

    #[test]
    fn normalize_skill_name_rejects_reserved_words() {
        let err = normalize_skill_name("claude-helper").unwrap_err();
        assert!(err.contains("reserved words"));
    }

    #[test]
    fn prepare_skill_document_rebuilds_frontmatter() {
        let prepared = prepare_skill_document(
            "Battery Helper",
            "Handles battery checks for Tizen workflows.",
            "---\nname: old\ndescription: old\n---\n\n# Battery\nUse the device tool.",
        )
        .unwrap();

        assert_eq!(prepared.normalized_name, "battery-helper");
        assert!(prepared.document.starts_with("---\nname: battery-helper\n"));
        assert!(prepared
            .document
            .contains("description: \"Handles battery checks for Tizen workflows.\""));
        assert!(prepared.document.contains("# Battery"));
    }

    #[test]
    fn list_skill_reference_docs_reads_markdown_titles() {
        let dir = tempfile::tempdir().unwrap();
        let doc_path = dir.path().join("SKILL_BEST_PRACTICE.md");
        fs::write(&doc_path, "# Skill authoring best practices\n\nBody").unwrap();

        let docs = list_skill_reference_docs(dir.path());
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].name, "SKILL_BEST_PRACTICE.md");
        assert_eq!(docs[0].description, "Skill authoring best practices");
    }

    #[test]
    fn read_skill_reference_doc_defaults_to_best_practice() {
        let dir = tempfile::tempdir().unwrap();
        let doc_path = dir.path().join(DEFAULT_SKILL_REFERENCE_DOC);
        fs::write(&doc_path, "# Best\n\nBody").unwrap();

        let doc = read_skill_reference_doc(dir.path(), "").unwrap();
        assert_eq!(doc.name, DEFAULT_SKILL_REFERENCE_DOC);
        assert!(doc.description.contains("Body"));
    }
}
