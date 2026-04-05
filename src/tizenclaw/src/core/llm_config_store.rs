use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

fn ensure_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    value.as_object_mut().expect("object just initialized")
}

fn parse_path(path: &str) -> Result<Vec<&str>, String> {
    let parts: Vec<&str> = path
        .split('.')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return Err("Config path must not be empty".into());
    }
    Ok(parts)
}

pub fn default_document() -> Value {
    json!({
        "active_backend": "gemini",
        "fallback_backends": ["openai", "ollama"],
        "benchmark": {
            "pinchbench": {
                "actual_tokens": {
                    "prompt": 0,
                    "completion": 0,
                    "total": 0
                },
                "target": {
                    "score": 0.0,
                    "summary": "",
                    "suite": "all"
                }
            }
        },
        "backends": {
            "gemini": {
                "api_key": "",
                "model": "gemini-2.5-flash",
                "temperature": 0.7,
                "max_tokens": 4096
            },
            "openai": {
                "api_key": "",
                "model": "gpt-4o",
                "endpoint": "https://api.openai.com/v1"
            },
            "anthropic": {
                "api_key": "",
                "model": "claude-sonnet-4-20250514",
                "endpoint": "https://api.anthropic.com/v1",
                "temperature": 0.7,
                "max_tokens": 4096
            },
            "xai": {
                "api_key": "",
                "model": "grok-3",
                "endpoint": "https://api.x.ai/v1"
            },
            "ollama": {
                "model": "llama3",
                "endpoint": "http://localhost:11434"
            }
        },
        "features": {
            "image_generation": {
                "provider": "openai",
                "api_key": "",
                "model": "gpt-image-1",
                "endpoint": "https://api.openai.com/v1",
                "size": "1024x1024",
                "background": "auto"
            }
        }
    })
}

pub fn config_path(config_dir: &Path) -> PathBuf {
    config_dir.join("llm_config.json")
}

pub fn load(config_dir: &Path) -> Result<Value, String> {
    let path = config_path(config_dir);
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(default_document());
        }
        Err(err) => {
            return Err(format!(
                "Failed to read LLM config '{}': {}",
                path.display(),
                err
            ));
        }
    };

    serde_json::from_str(&content)
        .map_err(|err| format!("Failed to parse LLM config '{}': {}", path.display(), err))
}

pub fn save(config_dir: &Path, doc: &Value) -> Result<PathBuf, String> {
    std::fs::create_dir_all(config_dir).map_err(|err| {
        format!(
            "Failed to create config dir '{}': {}",
            config_dir.display(),
            err
        )
    })?;

    let path = config_path(config_dir);
    let serialized = serde_json::to_string_pretty(doc)
        .map_err(|err| format!("Failed to serialize LLM config: {}", err))?;
    std::fs::write(&path, serialized)
        .map_err(|err| format!("Failed to write LLM config '{}': {}", path.display(), err))?;
    Ok(path)
}

pub fn get_value(doc: &Value, path: Option<&str>) -> Result<Value, String> {
    let Some(path) = path else {
        return Ok(doc.clone());
    };

    let mut cursor = doc;
    for part in parse_path(path)? {
        cursor = cursor
            .get(part)
            .ok_or_else(|| format!("Config path '{}' was not found", path))?;
    }
    Ok(cursor.clone())
}

pub fn set_value(doc: &mut Value, path: &str, new_value: Value) -> Result<(), String> {
    let parts = parse_path(path)?;
    let mut cursor = doc;

    for part in &parts[..parts.len().saturating_sub(1)] {
        let object = ensure_object(cursor);
        cursor = object
            .entry((*part).to_string())
            .or_insert_with(|| Value::Object(Map::new()));
    }

    let object = ensure_object(cursor);
    object.insert(parts[parts.len() - 1].to_string(), new_value);
    Ok(())
}

pub fn unset_value(doc: &mut Value, path: &str) -> Result<Value, String> {
    let parts = parse_path(path)?;
    let mut cursor = doc;

    for part in &parts[..parts.len().saturating_sub(1)] {
        cursor = cursor
            .get_mut(*part)
            .ok_or_else(|| format!("Config path '{}' was not found", path))?;
    }

    let object = cursor
        .as_object_mut()
        .ok_or_else(|| format!("Config path '{}' does not point to an object field", path))?;
    object
        .remove(parts[parts.len() - 1])
        .ok_or_else(|| format!("Config path '{}' was not found", path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_value_reads_nested_fields() {
        let doc = default_document();
        let value = get_value(&doc, Some("backends.gemini.model")).unwrap();
        assert_eq!(value, json!("gemini-2.5-flash"));
    }

    #[test]
    fn set_value_creates_nested_objects() {
        let mut doc = json!({});
        set_value(&mut doc, "benchmark.pinchbench.target.score", json!(0.92)).unwrap();

        assert_eq!(
            get_value(&doc, Some("benchmark.pinchbench.target.score")).unwrap(),
            json!(0.92)
        );
    }

    #[test]
    fn unset_value_removes_field() {
        let mut doc = default_document();
        let removed = unset_value(&mut doc, "benchmark.pinchbench.target.summary").unwrap();
        assert_eq!(removed, json!(""));
        assert!(get_value(&doc, Some("benchmark.pinchbench.target.summary")).is_err());
    }
}
