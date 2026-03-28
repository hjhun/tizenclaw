use std::fs;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct TextualSkill {
    pub file_name: String,
    pub absolute_path: String,
    pub description: String,
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
                    let content = fs::read_to_string(&skill_md_path).unwrap_or_default();
                    let description = extract_description(&content, &skill_name);

                    skills.push(TextualSkill {
                        file_name: skill_name,
                        absolute_path,
                        description,
                    });
                }
            }
        }
    }
    skills
}

/// Extract 'description:' from YAML frontmatter strictly as supported by Anthropic OpenClaw.
/// Does NOT implement custom specs or random heading fallbacks.
fn extract_description(content: &str, skill_name: &str) -> String {
    let mut in_yaml = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            in_yaml = !in_yaml;
            continue;
        }
        if in_yaml && trimmed.starts_with("description:") {
            let val = trimmed.trim_start_matches("description:").trim().trim_matches('"').trim_matches('\'');
            if !val.is_empty() {
                return val.to_string();
            }
        }
    }
    
    // If no description exists in YAML, return a generic fallback matching the skill name
    format!("Custom skill: {}", skill_name)
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
}
