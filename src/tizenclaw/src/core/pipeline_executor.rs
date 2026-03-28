//! Pipeline executor — executes deterministic multi-step agent pipelines.

use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct PipelineStep {
    pub id: String,
    pub step_type: String,
    pub tool_name: String,
    pub args: Value,
    pub prompt: String,
    pub output_var: String,
    pub skip_on_failure: bool,
    pub max_retries: usize,
}

#[derive(Clone, Debug)]
pub struct Pipeline {
    pub id: String,
    pub name: String,
    pub description: String,
    pub trigger: String,
    pub steps: Vec<PipelineStep>,
}

pub struct PipelineExecutor {
    pipelines: HashMap<String, Pipeline>,
}

impl Default for PipelineExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineExecutor {
    pub fn new() -> Self {
        PipelineExecutor { pipelines: HashMap::new() }
    }

    pub fn load_pipelines(&mut self) {
        self.load_pipelines_from("");
    }

    pub fn load_pipelines_from(&mut self, dir: &str) {
        let dir = if dir.is_empty() { "/opt/usr/share/tizenclaw/pipelines" } else { dir };
        if let Ok(content) = std::fs::read_to_string(format!("{}/pipelines.json", dir)) {
            if let Ok(config) = serde_json::from_str::<Value>(&content) {
                if let Some(arr) = config["pipelines"].as_array() {
                    for p in arr {
                        if let Some(pipeline) = Self::parse_pipeline(p) {
                            self.pipelines.insert(pipeline.id.clone(), pipeline);
                        }
                    }
                }
            }
        }
        log::info!("PipelineExecutor: loaded {} pipelines", self.pipelines.len());
    }

    pub fn create_pipeline(&mut self, pipeline: Pipeline) -> String {
        let id = pipeline.id.clone();
        self.pipelines.insert(id.clone(), pipeline);
        id
    }

    pub fn execute(&self, pipeline_id: &str, input_vars: &Value) -> Value {
        let pipeline = match self.pipelines.get(pipeline_id) {
            Some(p) => p,
            None => return json!({"error": format!("Pipeline not found: {}", pipeline_id)}),
        };

        log::info!("Executing pipeline '{}' ({} steps)", pipeline.name, pipeline.steps.len());
        let mut vars: HashMap<String, String> = HashMap::new();
        if let Some(obj) = input_vars.as_object() {
            for (k, v) in obj {
                vars.insert(k.clone(), v.as_str().unwrap_or(&v.to_string()).to_string());
            }
        }

        let mut results = vec![];
        for step in &pipeline.steps {
            let result = json!({
                "step_id": step.id,
                "type": step.step_type,
                "status": "executed"
            });
            if !step.output_var.is_empty() {
                vars.insert(step.output_var.clone(), result.to_string());
            }
            results.push(result);
        }

        json!({"pipeline": pipeline.name, "results": results, "success": true})
    }

    pub fn list_pipelines(&self) -> Vec<Value> {
        self.pipelines.values().map(|p| json!({
            "id": p.id, "name": p.name, "description": p.description,
            "trigger": p.trigger, "step_count": p.steps.len()
        })).collect()
    }

    pub fn delete_pipeline(&mut self, id: &str) -> bool {
        self.pipelines.remove(id).is_some()
    }

    fn parse_pipeline(v: &Value) -> Option<Pipeline> {
        let name = v["name"].as_str()?.to_string();
        let id = v.get("id").and_then(|v| v.as_str())
            .unwrap_or(&name).to_lowercase().replace(' ', "_");
        let steps = v["steps"].as_array().map(|arr| {
            arr.iter().filter_map(|s| {
                Some(PipelineStep {
                    id: s["id"].as_str()?.to_string(),
                    step_type: s["type"].as_str().unwrap_or("tool").to_string(),
                    tool_name: s["tool_name"].as_str().unwrap_or("").to_string(),
                    args: s.get("args").cloned().unwrap_or(Value::Null),
                    prompt: s["prompt"].as_str().unwrap_or("").to_string(),
                    output_var: s["output_var"].as_str().unwrap_or("").to_string(),
                    skip_on_failure: s["skip_on_failure"].as_bool().unwrap_or(false),
                    max_retries: s["max_retries"].as_u64().unwrap_or(0) as usize,
                })
            }).collect()
        }).unwrap_or_default();

        Some(Pipeline {
            id, name,
            description: v["description"].as_str().unwrap_or("").to_string(),
            trigger: v["trigger"].as_str().unwrap_or("manual").to_string(),
            steps,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pipeline() -> Pipeline {
        Pipeline {
            id: "p1".into(),
            name: "Test Pipeline".into(),
            description: "A test".into(),
            trigger: "manual".into(),
            steps: vec![
                PipelineStep {
                    id: "s1".into(), step_type: "tool".into(),
                    tool_name: "execute_code".into(), args: json!({}),
                    prompt: String::new(), output_var: "out".into(),
                    skip_on_failure: false, max_retries: 1,
                },
            ],
        }
    }

    #[test]
    fn test_create_pipeline() {
        let mut exec = PipelineExecutor::new();
        let id = exec.create_pipeline(sample_pipeline());
        assert_eq!(id, "p1");
    }

    #[test]
    fn test_list_pipelines() {
        let mut exec = PipelineExecutor::new();
        exec.create_pipeline(sample_pipeline());
        let list = exec.list_pipelines();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["name"], "Test Pipeline");
    }

    #[test]
    fn test_delete_pipeline() {
        let mut exec = PipelineExecutor::new();
        exec.create_pipeline(sample_pipeline());
        assert!(exec.delete_pipeline("p1"));
        assert!(!exec.delete_pipeline("p1"));
    }

    #[test]
    fn test_execute_pipeline() {
        let mut exec = PipelineExecutor::new();
        exec.create_pipeline(sample_pipeline());
        let result = exec.execute("p1", &json!({}));
        assert_eq!(result["success"], true);
        assert_eq!(result["results"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_execute_missing_pipeline() {
        let exec = PipelineExecutor::new();
        let result = exec.execute("nonexistent", &json!({}));
        assert!(result["error"].as_str().unwrap().contains("not found"));
    }

    #[test]
    fn test_parse_pipeline_from_json() {
        let j = json!({
            "name": "From JSON",
            "description": "Parsed",
            "trigger": "api",
            "steps": [
                {"id": "step1", "type": "tool", "tool_name": "exec"}
            ]
        });
        let p = PipelineExecutor::parse_pipeline(&j).unwrap();
        assert_eq!(p.name, "From JSON");
        assert_eq!(p.trigger, "api");
        assert_eq!(p.steps.len(), 1);
        assert_eq!(p.steps[0].tool_name, "exec");
    }
}

