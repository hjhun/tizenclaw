//! Skill verifier — validates skill packages for safety and correctness.

use super::skill_manifest::SkillManifest;
use serde_json::{json, Value};

#[derive(Clone, Debug)]
pub struct VerificationResult {
    pub passed: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

pub struct SkillVerifier {
    max_code_size: usize,
    allowed_runtimes: Vec<String>,
    blocked_imports: Vec<String>,
}

impl SkillVerifier {
    pub fn new() -> Self {
        SkillVerifier {
            max_code_size: 1024 * 1024, // 1MB
            allowed_runtimes: vec!["python".into(), "node".into(), "native".into(), "cli".into()],
            blocked_imports: vec![
                "subprocess".into(),
                "shutil.rmtree".into(),
                "os.system".into(),
                "ctypes".into(),
            ],
        }
    }

    pub fn verify(&self, manifest: &SkillManifest, skill_dir: &std::path::Path) -> VerificationResult {
        let mut result = VerificationResult {
            passed: true,
            errors: vec![],
            warnings: vec![],
        };

        // 1. Runtime check
        if !self.allowed_runtimes.contains(&manifest.runtime) {
            result.errors.push(format!("Unsupported runtime: {}", manifest.runtime));
            result.passed = false;
        }

        // 2. Name validation
        if !manifest.name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            result.errors.push("Skill name contains invalid characters".into());
            result.passed = false;
        }

        // 3. Entry point exists
        if !manifest.entry_point.is_empty() {
            let ep = skill_dir.join(&manifest.entry_point);
            if !ep.exists() {
                result.errors.push(format!("Entry point not found: {}", manifest.entry_point));
                result.passed = false;
            }
        }

        // 4. Code size check
        if let Ok(entries) = std::fs::read_dir(skill_dir) {
            let total_size: u64 = entries.flatten()
                .filter_map(|e| e.metadata().ok())
                .map(|m| m.len())
                .sum();
            if total_size > self.max_code_size as u64 {
                result.errors.push(format!("Skill package too large: {} bytes", total_size));
                result.passed = false;
            }
        }

        // 5. Blocked import scan (Python only)
        if manifest.runtime == "python" {
            let code_path = if manifest.entry_point.is_empty() {
                skill_dir.join("main.py")
            } else {
                skill_dir.join(&manifest.entry_point)
            };
            if let Ok(code) = std::fs::read_to_string(&code_path) {
                for blocked in &self.blocked_imports {
                    if code.contains(blocked.as_str()) {
                        result.warnings.push(format!("Potentially dangerous import: {}", blocked));
                    }
                }
            }
        }

        // 6. Risk level validation
        if manifest.risk_level == "high" {
            result.warnings.push("Skill has high risk level — requires explicit approval".into());
        }

        log::info!("SkillVerifier: {} — {} (errors={}, warnings={})",
            manifest.name,
            if result.passed { "PASSED" } else { "FAILED" },
            result.errors.len(), result.warnings.len());

        result
    }
}
