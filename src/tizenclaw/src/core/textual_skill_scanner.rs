use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct TextualSkill {
    pub file_name: String,
    pub absolute_path: String,
    pub description: String,
    pub tags: Vec<String>,
    pub triggers: Vec<String>,
    pub examples: Vec<String>,
    pub openclaw_requires: Vec<String>,
    pub openclaw_install: Vec<String>,
    pub prelude_excerpt: String,
    pub code_fence_languages: Vec<String>,
    pub shell_prelude: bool,
    pub searchable_text: String,
}

/// Scans a directory for Anthropic's OpenClaw-style Textual Skills.
/// An official skill must exist at `<skills_dir>/<skill_name>/SKILL.md`.
/// Files like `cli tool.md` at the root of the directory are explicitly ignored.
pub fn scan_textual_skills(skills_dir: &str) -> Vec<TextualSkill> {
    let mut skills = Vec::new();
    let root = Path::new(skills_dir);
    if !root.exists() || !root.is_dir() {
        return skills;
    }

    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            let skill_folder = entry.path();

            // OpenClaw skills are strictly directories containing a SKILL.md
            if skill_folder.is_dir() {
                let skill_md_path = skill_folder.join("SKILL.md");

                if skill_md_path.exists() && skill_md_path.is_file() {
                    let skill_name = skill_folder
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown_skill")
                        .to_string();

                    let absolute_path = skill_md_path.to_string_lossy().to_string();
                    let content = match fs::read_to_string(&skill_md_path) {
                        Ok(content) => content,
                        Err(error) => {
                            log::warn!(
                                "TextualSkillScanner: failed to read '{}': {}",
                                skill_md_path.display(),
                                error
                            );
                            continue;
                        }
                    };
                    let metadata = extract_skill_metadata(&content, &skill_name);
                    let body = extract_skill_body(&content);
                    let (prelude_excerpt, code_fence_languages, shell_prelude) =
                        extract_skill_audit_metadata(&body);
                    let searchable_text = build_searchable_text(
                        &metadata.description,
                        &metadata.tags,
                        &metadata.triggers,
                        &metadata.examples,
                    );

                    skills.push(TextualSkill {
                        file_name: skill_name,
                        absolute_path,
                        description: metadata.description,
                        tags: metadata.tags,
                        triggers: metadata.triggers,
                        examples: metadata.examples,
                        openclaw_requires: metadata.openclaw_requires,
                        openclaw_install: metadata.openclaw_install,
                        prelude_excerpt,
                        code_fence_languages,
                        shell_prelude,
                        searchable_text,
                    });
                }
            }
        }
    }
    skills
}

pub fn scan_textual_skills_from_roots(roots: &[&str]) -> Vec<TextualSkill> {
    let mut deduped = BTreeMap::new();
    for root in roots {
        for skill in scan_textual_skills(root) {
            deduped.entry(skill.file_name.clone()).or_insert(skill);
        }
    }
    deduped.into_values().collect()
}

#[derive(Default)]
struct SkillMetadata {
    description: String,
    tags: Vec<String>,
    triggers: Vec<String>,
    examples: Vec<String>,
    openclaw_requires: Vec<String>,
    openclaw_install: Vec<String>,
}

fn extract_skill_metadata(content: &str, skill_name: &str) -> SkillMetadata {
    let yaml = extract_frontmatter(content);
    let mut metadata = SkillMetadata {
        description: format!("Custom skill: {}", skill_name),
        ..Default::default()
    };

    let mut section_stack: Vec<(usize, String)> = Vec::new();
    let mut active_list: Option<String> = None;

    for raw_line in yaml.lines() {
        let indent = raw_line.chars().take_while(|c| c.is_whitespace()).count();
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("- ") {
            let item = normalize_scalar(rest);
            match active_list.as_deref() {
                Some("tags") if !item.is_empty() => metadata.tags.push(item),
                Some("triggers") if !item.is_empty() => metadata.triggers.push(item),
                Some("examples") if !item.is_empty() => metadata.examples.push(item),
                Some("requires") | Some("metadata.openclaw.requires") if !item.is_empty() => {
                    metadata.openclaw_requires.push(item)
                }
                Some("install") | Some("metadata.openclaw.install") if !item.is_empty() => {
                    metadata.openclaw_install.push(item)
                }
                _ => {}
            }
            continue;
        }

        active_list = None;

        while let Some((stack_indent, _)) = section_stack.last() {
            if *stack_indent >= indent {
                section_stack.pop();
            } else {
                break;
            }
        }

        if let Some((key, raw_value)) = trimmed.split_once(':') {
            let key = key.trim().to_string();
            let value = normalize_scalar(raw_value);
            let mut path = section_stack
                .iter()
                .map(|(_, name)| name.as_str())
                .collect::<Vec<_>>();
            path.push(key.as_str());
            let full_key = path.join(".");

            if value.is_empty() {
                section_stack.push((indent, key));
                if full_key == "requires"
                    || full_key == "install"
                    || full_key == "metadata.openclaw.requires"
                    || full_key == "metadata.openclaw.install"
                    || full_key == "tags"
                    || full_key == "triggers"
                    || full_key == "examples"
                {
                    active_list = Some(full_key);
                }
                continue;
            }

            if full_key == "description" {
                metadata.description = value;
            } else if full_key == "tags" {
                metadata.tags.extend(parse_inline_list(&value));
            } else if full_key == "triggers" {
                metadata.triggers.extend(parse_inline_list(&value));
            } else if full_key == "examples" {
                metadata.examples.extend(parse_inline_list(&value));
            } else if full_key == "requires" {
                metadata.openclaw_requires.extend(parse_inline_list(&value));
            } else if full_key == "install" {
                metadata.openclaw_install.extend(parse_inline_list(&value));
            } else if full_key == "metadata.openclaw.requires" {
                metadata.openclaw_requires.extend(parse_inline_list(&value));
            } else if full_key == "metadata.openclaw.install" {
                metadata.openclaw_install.extend(parse_inline_list(&value));
            }
        }
    }

    dedupe_preserve_order(&mut metadata.tags);
    dedupe_preserve_order(&mut metadata.triggers);
    dedupe_preserve_order(&mut metadata.examples);
    dedupe_preserve_order(&mut metadata.openclaw_requires);
    dedupe_preserve_order(&mut metadata.openclaw_install);

    metadata
}

fn normalize_scalar(raw: &str) -> String {
    raw.trim().trim_matches('"').trim_matches('\'').to_string()
}

fn parse_inline_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(normalize_scalar)
        .filter(|item| !item.is_empty())
        .collect()
}

fn dedupe_preserve_order(values: &mut Vec<String>) {
    let mut seen = HashSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

fn extract_frontmatter(content: &str) -> String {
    let mut lines = content.lines();
    if !matches!(lines.next().map(str::trim), Some("---")) {
        return String::new();
    }

    let mut yaml_lines = Vec::new();
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        yaml_lines.push(line.to_string());
    }
    yaml_lines.join("\n")
}

fn extract_skill_body(content: &str) -> String {
    if content.starts_with("---") {
        let mut lines = content.lines();
        let _ = lines.next();
        let mut past_frontmatter = false;
        let mut remaining = Vec::new();
        for line in lines {
            if past_frontmatter {
                remaining.push(line);
            } else if line.trim() == "---" {
                past_frontmatter = true;
            }
        }
        remaining.join("\n")
    } else {
        content.to_string()
    }
}

fn extract_skill_audit_metadata(body: &str) -> (String, Vec<String>, bool) {
    let mut prelude_lines = Vec::new();
    let mut code_fence_languages = Vec::new();
    let mut in_fence = false;

    for raw_line in body.lines() {
        let trimmed = raw_line.trim();
        if let Some(rest) = trimmed.strip_prefix("```") {
            let language = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            if !in_fence && !language.is_empty() {
                code_fence_languages.push(language);
            }
            in_fence = !in_fence;
            continue;
        }

        if in_fence || trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('#') {
            if !prelude_lines.is_empty() {
                break;
            }
            continue;
        }

        prelude_lines.push(trimmed.to_string());
        if prelude_lines.len() >= 3 {
            break;
        }
    }

    code_fence_languages.sort();
    code_fence_languages.dedup();
    let shell_prelude = code_fence_languages
        .iter()
        .any(|language| matches!(language.as_str(), "bash" | "sh" | "shell" | "zsh"));
    let mut excerpt = prelude_lines.join(" ");
    if excerpt.chars().count() > 240 {
        excerpt = excerpt.chars().take(240).collect();
    }

    (excerpt, code_fence_languages, shell_prelude)
}

fn build_searchable_text(
    description: &str,
    tags: &[String],
    triggers: &[String],
    examples: &[String],
) -> String {
    format!(
        "{} {} {} {}",
        description,
        tags.join(" "),
        triggers.join(" "),
        examples.join(" ")
    )
    .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_scan_textual_skills_valid() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("hello_world");
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_file = skill_dir.join("SKILL.md");

        let content = "---\ndescription: \"A test skill\"\n---\n# Hello\nBody text";
        fs::write(&skill_file, content).unwrap();

        let skills = scan_textual_skills(&dir.path().to_string_lossy());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].file_name, "hello_world");
        assert_eq!(skills[0].description, "A test skill");
        assert!(skills[0].tags.is_empty());
        assert!(skills[0].triggers.is_empty());
        assert!(skills[0].openclaw_requires.is_empty());
        assert!(skills[0].prelude_excerpt.contains("Body text"));
        assert!(!skills[0].shell_prelude);
    }

    #[test]
    fn test_ignores_loose_markdown_files() {
        let dir = tempfile::tempdir().unwrap();
        // Loose file that should be ignored
        let loose_file = dir.path().join("cli tool.md");
        fs::write(&loose_file, "# Im not a skill").unwrap();

        let skills = scan_textual_skills(&dir.path().to_string_lossy());
        assert_eq!(skills.len(), 0);
    }

    #[test]
    fn test_fallback_description() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("no_desc");
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_file = skill_dir.join("SKILL.md");

        let content = "# No Frontmatter\nJust text.";
        fs::write(&skill_file, content).unwrap();

        let skills = scan_textual_skills(&dir.path().to_string_lossy());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].description, "Custom skill: no_desc");
    }

    #[test]
    fn test_extracts_openclaw_metadata_lists() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("metadata_skill");
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_file = skill_dir.join("SKILL.md");

        let content = "---\ndescription: Metadata skill\nmetadata:\n  openclaw:\n    requires:\n      - uv\n      - node\n    install:\n      - uv sync\n      - npm install\n---\n# Skill";
        fs::write(&skill_file, content).unwrap();

        let skills = scan_textual_skills(&dir.path().to_string_lossy());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].openclaw_requires, vec!["uv", "node"]);
        assert_eq!(skills[0].openclaw_install, vec!["uv sync", "npm install"]);
    }

    #[test]
    fn test_extracts_trigger_metadata_lists() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("battery_helper");
        fs::create_dir_all(&skill_dir).unwrap();
        let skill_file = skill_dir.join("SKILL.md");

        let content = "---\ndescription: Battery helper\ntags:\n  - battery\n  - power\ntriggers:\n  - check battery\nexamples:\n  - check battery status\n---\n# Skill\nInspect device power state";
        fs::write(&skill_file, content).unwrap();

        let skills = scan_textual_skills(&dir.path().to_string_lossy());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].tags, vec!["battery", "power"]);
        assert_eq!(skills[0].triggers, vec!["check battery"]);
        assert_eq!(skills[0].examples, vec!["check battery status"]);
        assert!(skills[0].searchable_text.contains("battery helper"));
        assert!(skills[0].searchable_text.contains("check battery"));
    }

    #[test]
    fn scan_textual_skills_returns_empty_for_missing_dir() {
        let missing = tempfile::tempdir().unwrap();
        let path = missing.path().join("does-not-exist");
        let skills = scan_textual_skills(path.to_string_lossy().as_ref());
        assert!(skills.is_empty());
    }

    #[test]
    fn test_extracts_inline_comma_separated_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("battery_inline");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: Battery helper\ntags: battery, power\ntriggers: what is the battery level?, battery level\nmetadata:\n  openclaw:\n    requires: battery_tool, power_tool\n    install: sudo apt install battery-tool, npm i power-tool\n---\n# Skill\n",
        )
        .unwrap();

        let skills = scan_textual_skills(dir.path().to_string_lossy().as_ref());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].tags, vec!["battery", "power"]);
        assert_eq!(
            skills[0].triggers,
            vec!["what is the battery level?", "battery level"]
        );
        assert_eq!(
            skills[0].openclaw_requires,
            vec!["battery_tool", "power_tool"]
        );
    }

    #[test]
    fn test_extracts_top_level_requires_and_install_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("battery_requires");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: Battery helper\nrequires:\n  - battery_tool\ninstall: sudo apt install battery-tool\n---\n# Skill\n",
        )
        .unwrap();

        let skills = scan_textual_skills(dir.path().to_string_lossy().as_ref());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].openclaw_requires, vec!["battery_tool"]);
        assert_eq!(
            skills[0].openclaw_install,
            vec!["sudo apt install battery-tool"]
        );
    }

    #[test]
    fn scan_textual_skills_from_roots_deduplicates_names() {
        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir1.path().join("helper")).unwrap();
        fs::create_dir_all(dir2.path().join("helper")).unwrap();
        fs::write(
            dir1.path().join("helper/SKILL.md"),
            "---\ndescription: \"First\"\n---",
        )
        .unwrap();
        fs::write(
            dir2.path().join("helper/SKILL.md"),
            "---\ndescription: \"Second\"\n---",
        )
        .unwrap();

        let skills = scan_textual_skills_from_roots(&[
            dir1.path().to_string_lossy().as_ref(),
            dir2.path().to_string_lossy().as_ref(),
        ]);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].description, "First");
    }

    #[test]
    fn extracts_skill_prelude_and_shell_fences() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join("shell_helper");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\ndescription: Shell helper\n---\n# Shell Helper\nInspect runtime wrappers before execution.\nCollect trust metadata first.\n```bash\necho hi\n```\n",
        )
        .unwrap();

        let skills = scan_textual_skills(&dir.path().to_string_lossy());
        assert_eq!(skills.len(), 1);
        assert!(skills[0]
            .prelude_excerpt
            .contains("Inspect runtime wrappers"));
        assert_eq!(skills[0].code_fence_languages, vec!["bash"]);
        assert!(skills[0].shell_prelude);
    }
}
