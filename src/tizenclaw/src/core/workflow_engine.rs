//! Workflow engine — executes multi-step markdown-defined workflows.

use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, PartialEq)]
pub enum WorkflowStepType {
    Tool,
    #[default]
    Prompt,
    Condition,
}

#[derive(Clone, Debug, Default)]
pub struct WorkflowStep {
    pub id: String,
    pub step_type: WorkflowStepType,
    pub tool_name: String,
    pub instruction: String, // multiline prompt instruction
    pub args: Value,
    pub output_var: String,
    
    // Condition handling
    pub condition: String,
    pub then_step: String,
    pub else_step: String,
    
    // Error handling
    pub skip_on_failure: bool,
    pub max_retries: usize,
}

#[derive(Clone, Debug)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub trigger: String,
    pub raw_markdown: String,
    pub steps: Vec<WorkflowStep>,
}

pub struct WorkflowEngine {
    workflows: HashMap<String, Workflow>,
}

impl Default for WorkflowEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowEngine {
    pub fn new() -> Self {
        WorkflowEngine { workflows: HashMap::new() }
    }

    pub fn load_workflows(&mut self) {
        self.load_workflows_from("");
    }

    pub fn load_workflows_from(&mut self, dir: &str) {
        let dir = if dir.is_empty() { "/opt/usr/share/tizenclaw/workflows" } else { dir };
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "md").unwrap_or(false) && path.file_name() != Some(std::ffi::OsStr::new("index.md")) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Some(wf) = Self::parse_markdown(&content) {
                        self.workflows.insert(wf.id.clone(), wf);
                    }
                }
            }
        }
        log::info!("WorkflowEngine: loaded {} workflows", self.workflows.len());
    }

    pub fn get_workflow(&self, id: &str) -> Option<&Workflow> {
        self.workflows.get(id)
    }

    pub fn list_workflows(&self) -> Vec<Value> {
        self.workflows.values().map(|wf| {
            json!({
                "id": wf.id,
                "name": wf.name,
                "description": wf.description,
                "trigger": wf.trigger,
                "steps_count": wf.steps.len()
            })
        }).collect()
    }

    pub fn delete_workflow(&mut self, id: &str) -> bool {
        self.workflows.remove(id).is_some()
    }

    pub fn create_from_markdown(&mut self, markdown: &str) -> Result<String, String> {
        match Self::parse_markdown(markdown) {
            Some(wf) => {
                let id = wf.id.clone();
                self.workflows.insert(id.clone(), wf);
                Ok(id)
            }
            None => Err("Failed to parse workflow markdown".into()),
        }
    }

    /// Interpolates string templates like `{{var}}` into values from `vars`.
    /// E.g. `Hello {{name}}` -> `Hello TizenClaw`
    pub fn interpolate(template: &str, vars: &HashMap<String, Value>) -> String {
        let mut result = template.to_string();
        for (key, val) in vars {
            let val_str = if val.is_string() {
                val.as_str().unwrap_or("").to_string()
            } else {
                val.to_string()
            };
            result = result.replace(&format!("{{{{{}}}}}", key), &val_str);
        }
        result
    }

    /// Recursively interpolates JSON values.
    pub fn interpolate_json(j: &Value, vars: &HashMap<String, Value>) -> Value {
        match j {
            Value::String(s) => {
                let interpolated = Self::interpolate(s, vars);
                // Attempt to parse back to json if it looks like an object/array,
                // otherwise return as string. (Optional, based on need)
                Value::String(interpolated)
            }
            Value::Object(map) => {
                let mut new_map = serde_json::Map::new();
                for (k, v) in map {
                    new_map.insert(k.clone(), Self::interpolate_json(v, vars));
                }
                Value::Object(new_map)
            }
            Value::Array(arr) => {
                let new_arr = arr.iter().map(|v| Self::interpolate_json(v, vars)).collect();
                Value::Array(new_arr)
            }
            _ => j.clone(),
        }
    }

    /// Evaluates a simple condition expression `{{a}} == b`.
    /// More complex conditions can be parsed here.
    pub fn eval_condition(condition: &str, vars: &HashMap<String, Value>) -> bool {
        let expr = Self::interpolate(condition, vars).trim().to_string();
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() == 3 {
            let left = parts[0];
            let op = parts[1];
            let right = parts[2];
            match op {
                "==" => left == right,
                "!=" => left != right,
                ">"|" <" | ">=" | "<=" => {
                    let l: f64 = left.parse().unwrap_or(0.0);
                    let r: f64 = right.parse().unwrap_or(0.0);
                    match op {
                        ">" => l > r,
                        "<" => l < r,
                        ">=" => l >= r,
                        "<=" => l <= r,
                        _ => false
                    }
                }
                _ => false,
            }
        } else {
            // Unrecognized condition format
            false
        }
    }

    // Markdown Parser implementation (no external regex dependency)
    fn parse_markdown(markdown: &str) -> Option<Workflow> {
        let lines: Vec<&str> = markdown.lines().collect();
        let mut wf = Workflow {
            id: String::new(),
            name: String::new(),
            description: String::new(),
            trigger: "manual".to_string(),
            raw_markdown: markdown.to_string(),
            steps: vec![],
        };

        // 1. Extract Frontmatter
        let mut body_start = 0;
        if lines.first().map(|l| l.trim()) == Some("---") {
            let mut i = 1;
            while i < lines.len() {
                let text = lines[i].trim();
                if text == "---" {
                    body_start = i + 1;
                    break;
                }
                if let Some((k, v)) = text.split_once(':') {
                    let key = k.trim();
                    let val = v.trim().trim_matches('"');
                    match key {
                        "name" => wf.name = val.to_string(),
                        "description" => wf.description = val.to_string(),
                        "trigger" => wf.trigger = val.to_string(),
                        "id" => wf.id = val.to_string(),
                        _ => {}
                    }
                }
                i += 1;
            }
        }

        if wf.name.is_empty() {
            return None;
        }

        if wf.id.is_empty() {
            wf.id = wf.name.to_lowercase().replace(' ', "_");
        }

        // 2. Extract Steps
        let mut current_step: Option<(String, String, Vec<String>)> = None; // (id, title, buffer)

        for line in lines.iter().skip(body_start) {
            // "## Step 1: Check System"
            if line.starts_with("## Step ") {
                if let Some((id, _, buf)) = current_step.take() {
                    wf.steps.push(Self::parse_step_block(&id, &buf));
                }
                // extract Step N and title
                if let Some(colon_idx) = line.find(':') {
                    let step_mark = line[3..colon_idx].trim().to_string(); // "Step 1"
                    let step_id = step_mark.to_lowercase().replace(' ', "_");
                    current_step = Some((step_id.clone(), String::new(), vec![]));
                }
            } else if let Some((_, _, ref mut buf)) = current_step {
                buf.push(line.to_string());
            }
        }

        if let Some((id, _, buf)) = current_step {
            wf.steps.push(Self::parse_step_block(&id, &buf));
        }

        Some(wf)
    }

    fn parse_step_block(id: &str, body: &[String]) -> WorkflowStep {
        let mut step = WorkflowStep {
            id: id.to_string(),
            ..Default::default()
        };

        let mut in_multiline = false;
        let mut multiline_key = String::new();
        let mut multiline_val = vec![];

        for line in body {
            let trimmed = line.trim();
            if in_multiline {
                // Check if a new key starts
                if trimmed.starts_with("- ") && trimmed.contains(':') {
                    in_multiline = false;
                    if multiline_key == "instruction" {
                        step.instruction = multiline_val.join("\n").trim().to_string();
                    }
                    multiline_val.clear();
                } else {
                    multiline_val.push(line.to_string());
                    continue;
                }
            }

            if trimmed.is_empty() { continue; }

            let prefix = "- ";
            let text = trimmed.strip_prefix(prefix).unwrap_or(trimmed);
            
            if let Some((k, v)) = text.split_once(':') {
                let key = k.trim();
                let val = v.trim();

                match key {
                    "type" => {
                        step.step_type = match val {
                            "tool" => WorkflowStepType::Tool,
                            "condition" => WorkflowStepType::Condition,
                            _ => WorkflowStepType::Prompt,
                        };
                    }
                    "tool_name" => step.tool_name = val.to_string(),
                    "output_var" => step.output_var = val.to_string(),
                    "condition" => step.condition = val.to_string(),
                    "then_step" => step.then_step = val.to_string(),
                    "else_step" => step.else_step = val.to_string(),
                    "skip_on_failure" => step.skip_on_failure = val == "true",
                    "max_retries" => step.max_retries = val.parse().unwrap_or(0),
                    "args" => {
                        step.args = serde_json::from_str(val).unwrap_or(json!({}));
                    }
                    "instruction" => {
                        if val == "|" {
                            in_multiline = true;
                            multiline_key = key.to_string();
                        } else {
                            step.instruction = val.to_string();
                        }
                    }
                    _ => {}
                }
            }
        }

        if in_multiline && multiline_key == "instruction" {
            step.instruction = multiline_val.join("\n").trim().to_string();
        }

        if step.output_var.is_empty() {
            step.output_var = step.id.clone();
        }

        step
    }
}
