//! Context fusion engine — merges multiple context sources for LLM prompts.

use serde_json::{json, Value};

pub struct ContextFusionEngine;

impl ContextFusionEngine {
    pub fn new() -> Self { ContextFusionEngine }

    /// Fuse multiple context sources into a single prompt section.
    pub fn fuse(&self, contexts: &[(&str, Value)]) -> String {
        let mut parts = vec![];
        for (source, data) in contexts {
            if data.is_null() { continue; }
            let section = match data {
                Value::String(s) if !s.is_empty() => format!("[{}] {}", source, s),
                Value::Object(obj) if !obj.is_empty() => {
                    let items: Vec<String> = obj.iter()
                        .map(|(k, v)| format!("  {}: {}", k, v))
                        .collect();
                    format!("[{}]\n{}", source, items.join("\n"))
                }
                _ => continue,
            };
            parts.push(section);
        }
        parts.join("\n\n")
    }
}
