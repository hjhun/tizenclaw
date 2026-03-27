//! Skill repository — scans directories for skill packages with SKILL.md manifests.
//!
//! Skills are installed at `/opt/usr/share/tizen-tools/skills`.
//! Only SKILL.md-based skills are loaded (no manifest.json fallback).

use super::skill_manifest::SkillManifest;
use std::collections::HashMap;
use std::path::PathBuf;

/// Default skill installation directory.
const SKILLS_DIR: &str = "/opt/usr/share/tizen-tools/skills";

pub struct SkillRepository {
    skills: HashMap<String, (SkillManifest, PathBuf)>,
    skill_dirs: Vec<String>,
}

impl SkillRepository {
    pub fn new() -> Self {
        SkillRepository {
            skills: HashMap::new(),
            skill_dirs: vec![SKILLS_DIR.into()],
        }
    }

    pub fn add_skill_dir(&mut self, dir: &str) {
        self.skill_dirs.push(dir.to_string());
    }

    /// Scan all registered directories for SKILL.md-based skills.
    pub fn scan_all(&mut self) {
        self.skills.clear();
        for dir in self.skill_dirs.clone() {
            self.scan_dir(&dir);
        }
        log::info!(
            "SkillRepository: {} skills loaded from {} directories",
            self.skills.len(),
            self.skill_dirs.len()
        );
    }

    fn scan_dir(&mut self, dir: &str) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            // Only load skills with SKILL.md (Anthropic standard)
            if let Some(manifest) = SkillManifest::load(&path) {
                log::info!("Loaded skill '{}' from {:?}", manifest.name, path);
                self.skills.insert(manifest.name.clone(), (manifest, path));
            }
        }
    }

    pub fn get_skill(&self, name: &str) -> Option<&(SkillManifest, PathBuf)> {
        self.skills.get(name)
    }

    pub fn get_all_skills(&self) -> Vec<&SkillManifest> {
        self.skills.values().map(|(m, _)| m).collect()
    }

    pub fn get_skill_dir(&self, name: &str) -> Option<&PathBuf> {
        self.skills.get(name).map(|(_, p)| p)
    }

    pub fn remove_skill(&mut self, name: &str) -> bool {
        self.skills.remove(name).is_some()
    }

    pub fn skill_count(&self) -> usize {
        self.skills.len()
    }
}
