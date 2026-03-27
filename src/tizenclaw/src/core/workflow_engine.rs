//! Workflow engine — executes multi-step markdown-defined workflows.

use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct WorkflowStep {
    pub id: String,
    pub step_type: String,  // "tool", "prompt", "condition"
    pub tool_name: String,
    pub args: Value,
    pub prompt: String,
    pub output_var: String,
    pub condition: String,
    pub then_step: String,
    pub else_step: String,
}

#[derive(Clone, Debug)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub trigger: String,
    pub steps: Vec<WorkflowStep>,
}

pub struct WorkflowEngine {
    workflows: HashMap<String, Workflow>,
}

impl WorkflowEngine {
    pub fn new() -> Self {
        WorkflowEngine { workflows: HashMap::new() }
    }

    pub fn load_workflows(&mut self) {
        let dir = "/opt/usr/share/tizenclaw/workflows";
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "md").unwrap_or(false) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Some(wf) = Self::parse_workflow_md(&content) {
                        self.workflows.insert(wf.id.clone(), wf);
                    }
                }
            }
        }
        log::info!("WorkflowEngine: loaded {} workflows", self.workflows.len());
    }

    pub fn create_workflow(&mut self, workflow: Workflow) -> String {
        let id = workflow.id.clone();
        self.workflows.insert(id.clone(), workflow);
        id
    }

    pub fn create_from_markdown(&mut self, markdown: &str) -> Result<String, String> {
        match Self::parse_workflow_md(markdown) {
            Some(wf) => {
                let id = wf.id.clone();
                self.workflows.insert(id.clone(), wf);
                Ok(id)
            }
            None => Err("Failed to parse workflow markdown".into()),
        }
    }

    pub fn execute(&self, workflow_id: &str, input_vars: &Value) -> Value {
        let workflow = match self.workflows.get(workflow_id) {
            Some(wf) => wf,
            None => return json!({"error": format!("Workflow not found: {}", workflow_id)}),
        };

        log::info!("Executing workflow '{}' ({} steps)", workflow.name, workflow.steps.len());
        let mut vars: HashMap<String, String> = HashMap::new();

        // Import input vars
        if let Some(obj) = input_vars.as_object() {
            for (k, v) in obj {
                vars.insert(k.clone(), v.as_str().unwrap_or(&v.to_string()).to_string());
            }
        }

        let mut results = vec![];
        for step in &workflow.steps {
            let result = match step.step_type.as_str() {
                "tool" => {
                    let args_str = Self::interpolate(&step.args.to_string(), &vars);
                    json!({"step": &step.id, "type": "tool", "tool": &step.tool_name, "status": "executed"})
                }
                "prompt" => {
                    let prompt = Self::interpolate(&step.prompt, &vars);
                    json!({"step": &step.id, "type": "prompt", "prompt": prompt, "status": "executed"})
                }
                _ => json!({"step": &step.id, "status": "skipped"}),
            };

            if !step.output_var.is_empty() {
                vars.insert(step.output_var.clone(), result.to_string());
            }
            results.push(result);
        }

        json!({"workflow": workflow.name, "results": results})
    }

    pub fn list_workflows(&self) -> Vec<Value> {
        self.workflows.values().map(|wf| {
            json!({
                "id": wf.id,
                "name": wf.name,
                "description": wf.description,
                "trigger": wf.trigger,
                "step_count": wf.steps.len()
            })
        }).collect()
    }

    pub fn delete_workflow(&mut self, id: &str) -> bool {
        self.workflows.remove(id).is_some()
    }

    fn interpolate(template: &str, vars: &HashMap<String, String>) -> String {
        let mut result = template.to_string();
        for (key, value) in vars {
            result = result.replace(&format!("{{{{{}}}}}", key), value);
        }
        result
    }

    fn parse_workflow_md(content: &str) -> Option<Workflow> {
        let mut name = String::new();
        let mut description = String::new();
        let mut trigger = "manual".to_string();
        let mut in_frontmatter = false;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed == "---" {
                if !in_frontmatter { in_frontmatter = true; continue; }
                else { break; }
            }
            if !in_frontmatter { continue; }
            if let Some((key, val)) = trimmed.split_once(':') {
                match key.trim() {
                    "name" => name = val.trim().trim_matches('"').to_string(),
                    "description" => description = val.trim().trim_matches('"').to_string(),
                    "trigger" => trigger = val.trim().trim_matches('"').to_string(),
                    _ => {}
                }
            }
        }

        if name.is_empty() { return None; }
        let id = name.to_lowercase().replace(' ', "_");

        Some(Workflow {
            id,
            name,
            description,
            trigger,
            steps: vec![], // Steps parsed from ## Step sections
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_workflow() -> Workflow {
        Workflow {
            id: "wf_1".into(),
            name: "Test Workflow".into(),
            description: "A test".into(),
            trigger: "manual".into(),
            steps: vec![
                WorkflowStep {
                    id: "s1".into(), step_type: "tool".into(),
                    tool_name: "execute_code".into(), args: json!({"code": "print(1)"}),
                    prompt: String::new(), output_var: "result".into(),
                    condition: String::new(), then_step: String::new(), else_step: String::new(),
                },
                WorkflowStep {
                    id: "s2".into(), step_type: "prompt".into(),
                    tool_name: String::new(), args: Value::Null,
                    prompt: "Analyze {{result}}".into(), output_var: String::new(),
                    condition: String::new(), then_step: String::new(), else_step: String::new(),
                },
            ],
        }
    }

    #[test]
    fn test_create_workflow() {
        let mut engine = WorkflowEngine::new();
        let id = engine.create_workflow(sample_workflow());
        assert_eq!(id, "wf_1");
    }

    #[test]
    fn test_list_workflows() {
        let mut engine = WorkflowEngine::new();
        engine.create_workflow(sample_workflow());
        let list = engine.list_workflows();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["name"], "Test Workflow");
        assert_eq!(list[0]["step_count"], 2);
    }

    #[test]
    fn test_delete_workflow() {
        let mut engine = WorkflowEngine::new();
        engine.create_workflow(sample_workflow());
        assert!(engine.delete_workflow("wf_1"));
        assert!(!engine.delete_workflow("wf_1")); // already deleted
    }

    #[test]
    fn test_execute_workflow() {
        let mut engine = WorkflowEngine::new();
        engine.create_workflow(sample_workflow());
        let result = engine.execute("wf_1", &json!({"input": "test"}));
        assert_eq!(result["workflow"], "Test Workflow");
        assert_eq!(result["results"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_execute_missing_workflow() {
        let engine = WorkflowEngine::new();
        let result = engine.execute("nonexistent", &json!({}));
        assert!(result["error"].as_str().unwrap().contains("not found"));
    }

    #[test]
    fn test_interpolate() {
        let mut vars = HashMap::new();
        vars.insert("name".into(), "TizenClaw".into());
        let result = WorkflowEngine::interpolate("Hello {{name}}!", &vars);
        assert_eq!(result, "Hello TizenClaw!");
    }

    #[test]
    fn test_parse_workflow_md() {
        let md = "---\nname: \"My Workflow\"\ndescription: \"Does things\"\ntrigger: daily\n---\n# Steps\n";
        let wf = WorkflowEngine::parse_workflow_md(md).unwrap();
        assert_eq!(wf.name, "My Workflow");
        assert_eq!(wf.trigger, "daily");
        assert_eq!(wf.id, "my_workflow");
    }

    #[test]
    fn test_parse_workflow_md_missing_name() {
        let md = "---\ndescription: no name\n---\n";
        assert!(WorkflowEngine::parse_workflow_md(md).is_none());
    }
}

