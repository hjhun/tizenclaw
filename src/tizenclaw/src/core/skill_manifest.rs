//! Skill manifest — Anthropic-standard SKILL.md parser.
//!
//! Supports ONLY the `SKILL.md` format (YAML frontmatter + Markdown body):
//!
//! ```markdown
//! ---
//! name: skill_name
//! description: "What the skill does"
//! category: appliance
//! risk_level: low
//! runtime: python
//! entry_point: main.py
//! ---
//! # Skill Title
//!
//! Documentation body.
//!
//! ```json:parameters
//! { "type": "object", "properties": { ... }, "required": [...] }
//! ```
//! ```
//!
//! The `manifest.json` fallback is intentionally NOT supported.

use serde_json::{json, Value};

/// Parsed skill manifest from a SKILL.md file.
#[derive(Clone, Debug)]
pub struct SkillManifest {
    pub name: String,
    pub description: String,
    pub version: String,
    pub runtime: String,
    pub entry_point: String,
    pub input_schema: Value,
    pub risk_level: String,
    pub category: String,
    pub author: String,
    pub permissions: Vec<String>,
}

impl Default for SkillManifest {
    fn default() -> Self {
        SkillManifest {
            name: String::new(),
            description: String::new(),
            version: "1.0.0".into(),
            runtime: "python".into(),
            entry_point: String::new(),
            input_schema: json!({"type": "object", "properties": {}, "required": []}),
            risk_level: "low".into(),
            category: "general".into(),
            author: String::new(),
            permissions: vec![],
        }
    }
}

impl SkillManifest {
    /// Check if a directory contains a valid SKILL.md.
    pub fn has_skill_md(skill_dir: &std::path::Path) -> bool {
        skill_dir.join("SKILL.md").exists()
    }

    /// Load a skill manifest from a SKILL.md file.
    ///
    /// Returns `None` if:
    /// - SKILL.md does not exist or is unreadable
    /// - Missing `---` frontmatter delimiters
    /// - Missing required `name` field
    pub fn load(skill_dir: &std::path::Path) -> Option<Self> {
        let skill_md_path = skill_dir.join("SKILL.md");
        let content = std::fs::read_to_string(&skill_md_path).ok()?;
        Self::parse_skill_md(&content)
    }

    /// Parse a SKILL.md string into a manifest.
    fn parse_skill_md(content: &str) -> Option<Self> {
        let trimmed = content.trim();
        if !trimmed.starts_with("---") {
            log::warn!("SKILL.md missing frontmatter delimiters");
            return None;
        }

        // Find the closing --- marker
        let after_first = &trimmed[3..];
        let second_delim = after_first.find("---")?;
        let yaml_block = &after_first[..second_delim];
        let body = &after_first[second_delim + 3..];

        // Parse YAML frontmatter
        let mut manifest = Self::parse_frontmatter(yaml_block)?;

        // Extract parameters from ```json:parameters block
        let params = Self::extract_parameters_block(body);
        if !params.is_null() && params.is_object() {
            manifest.input_schema = params;
        }

        Some(manifest)
    }

    /// Parse YAML frontmatter key-value pairs.
    fn parse_frontmatter(yaml: &str) -> Option<Self> {
        let mut manifest = SkillManifest::default();

        for line in yaml.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() { continue; }

            if let Some((key, value)) = trimmed.split_once(':') {
                let key = key.trim();
                let value = value.trim().trim_matches('"').trim_matches('\'');
                match key {
                    "name" => manifest.name = value.to_string(),
                    "description" => manifest.description = value.to_string(),
                    "version" => manifest.version = value.to_string(),
                    "runtime" => manifest.runtime = value.to_string(),
                    "entry_point" => manifest.entry_point = value.to_string(),
                    "risk_level" => manifest.risk_level = value.to_string(),
                    "category" => manifest.category = value.to_string(),
                    "author" => manifest.author = value.to_string(),
                    _ => {}
                }
            }
        }

        if manifest.name.is_empty() {
            return None;
        }
        Some(manifest)
    }

    /// Extract the JSON parameters block from the Markdown body.
    ///
    /// Looks for ````json:parameters ... ``` ` code fence.
    fn extract_parameters_block(body: &str) -> Value {
        const MARKER: &str = "```json:parameters";

        let start = match body.find(MARKER) {
            Some(pos) => pos,
            None => return Value::Null,
        };

        let block_start = match body[start..].find('\n') {
            Some(pos) => start + pos + 1,
            None => return Value::Null,
        };

        let block_end = match body[block_start..].find("```") {
            Some(pos) => block_start + pos,
            None => return Value::Null,
        };

        let json_str = body[block_start..block_end].trim();
        match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(e) => {
                log::warn!("Failed to parse parameters block: {}", e);
                Value::Null
            }
        }
    }

    /// Generate a SKILL.md string from this manifest.
    pub fn to_skill_md(&self) -> String {
        let mut out = String::new();

        // YAML frontmatter
        out.push_str("---\n");
        out.push_str(&format!("name: {}\n", self.name));
        if !self.description.is_empty() {
            out.push_str(&format!("description: \"{}\"\n", self.description));
        }
        if !self.category.is_empty() {
            out.push_str(&format!("category: {}\n", self.category));
        }
        out.push_str(&format!("risk_level: {}\n", self.risk_level));
        if !self.runtime.is_empty() {
            out.push_str(&format!("runtime: {}\n", self.runtime));
        }
        if !self.entry_point.is_empty() {
            out.push_str(&format!("entry_point: {}\n", self.entry_point));
        }
        out.push_str("---\n\n");

        // Heading
        out.push_str(&format!("# {}\n\n", self.name));
        if !self.description.is_empty() {
            out.push_str(&format!("{}\n\n", self.description));
        }

        // Parameters block
        if let Ok(json_str) = serde_json::to_string_pretty(&self.input_schema) {
            out.push_str("```json:parameters\n");
            out.push_str(&json_str);
            out.push('\n');
            out.push_str("```\n");
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SKILL_MD: &str = r#"---
name: get_weather
description: "Get weather for a city"
category: utility
risk_level: low
runtime: python
entry_point: main.py
---

# get_weather

Get weather for a city.

```json:parameters
{
  "type": "object",
  "properties": {
    "city": {"type": "string"}
  },
  "required": ["city"]
}
```
"#;

    #[test]
    fn test_parse_full_skill_md() {
        let manifest = SkillManifest::parse_skill_md(SAMPLE_SKILL_MD).unwrap();
        assert_eq!(manifest.name, "get_weather");
        assert_eq!(manifest.description, "Get weather for a city");
        assert_eq!(manifest.category, "utility");
        assert_eq!(manifest.risk_level, "low");
        assert_eq!(manifest.runtime, "python");
        assert_eq!(manifest.entry_point, "main.py");
    }

    #[test]
    fn test_parse_parameters_block() {
        let manifest = SkillManifest::parse_skill_md(SAMPLE_SKILL_MD).unwrap();
        assert_eq!(manifest.input_schema["type"], "object");
        assert!(manifest.input_schema["properties"]["city"].is_object());
        assert_eq!(manifest.input_schema["required"][0], "city");
    }

    #[test]
    fn test_missing_name_returns_none() {
        let md = "---\ndescription: no name\n---\n";
        assert!(SkillManifest::parse_skill_md(md).is_none());
    }

    #[test]
    fn test_no_frontmatter_returns_none() {
        let md = "# Just a heading\nNo frontmatter here.";
        assert!(SkillManifest::parse_skill_md(md).is_none());
    }

    #[test]
    fn test_defaults() {
        let d = SkillManifest::default();
        assert_eq!(d.version, "1.0.0");
        assert_eq!(d.runtime, "python");
        assert_eq!(d.risk_level, "low");
        assert_eq!(d.category, "general");
    }

    #[test]
    fn test_to_skill_md_roundtrip() {
        let manifest = SkillManifest::parse_skill_md(SAMPLE_SKILL_MD).unwrap();
        let generated = manifest.to_skill_md();
        assert!(generated.contains("name: get_weather"));
        assert!(generated.contains("```json:parameters"));
        // Re-parse the generated markdown
        let re_parsed = SkillManifest::parse_skill_md(&generated).unwrap();
        assert_eq!(re_parsed.name, manifest.name);
        assert_eq!(re_parsed.description, manifest.description);
    }

    #[test]
    fn test_no_parameters_block_uses_default() {
        let md = "---\nname: simple\n---\nNo params block.";
        let manifest = SkillManifest::parse_skill_md(md).unwrap();
        assert_eq!(manifest.input_schema["type"], "object");
    }

    #[test]
    fn test_minimal_frontmatter() {
        let md = "---\nname: minimal_tool\n---\n";
        let m = SkillManifest::parse_skill_md(md).unwrap();
        assert_eq!(m.name, "minimal_tool");
        assert_eq!(m.runtime, "python"); // default
    }

    #[test]
    fn test_quoted_values() {
        let md = "---\nname: quoted\ndescription: 'Has single quotes'\n---\n";
        let m = SkillManifest::parse_skill_md(md).unwrap();
        assert_eq!(m.description, "Has single quotes");
    }
}

