//! Offline fallback — rule-based responses when LLM backends are unavailable.

use serde_json::Value;

pub struct FallbackRule {
    pub patterns: Vec<String>,
    pub tool_name: String,
    pub args: Value,
    pub direct_response: String,
}

pub struct FallbackMatch {
    pub matched: bool,
    pub tool_name: String,
    pub args: Value,
    pub direct_response: String,
}

pub struct OfflineFallback {
    rules: Vec<FallbackRule>,
}

impl OfflineFallback {
    pub fn new() -> Self {
        OfflineFallback { rules: vec![] }
    }

    pub fn load_config(&mut self, path: &str) {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return,
        };
        let config: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return,
        };

        if let Some(rules) = config["rules"].as_array() {
            for rule in rules {
                let patterns: Vec<String> = rule["patterns"].as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_lowercase())).collect())
                    .unwrap_or_default();
                self.rules.push(FallbackRule {
                    patterns,
                    tool_name: rule["tool_name"].as_str().unwrap_or("").to_string(),
                    args: rule.get("args").cloned().unwrap_or(Value::Null),
                    direct_response: rule["direct_response"].as_str().unwrap_or("").to_string(),
                });
            }
        }
        log::info!("OfflineFallback: loaded {} rules", self.rules.len());
    }

    pub fn match_prompt(&self, prompt: &str) -> FallbackMatch {
        let lower = prompt.to_lowercase();
        for rule in &self.rules {
            for pattern in &rule.patterns {
                if lower.contains(pattern) {
                    return FallbackMatch {
                        matched: true,
                        tool_name: rule.tool_name.clone(),
                        args: rule.args.clone(),
                        direct_response: rule.direct_response.clone(),
                    };
                }
            }
        }
        FallbackMatch {
            matched: false,
            tool_name: String::new(),
            args: Value::Null,
            direct_response: String::new(),
        }
    }
}
