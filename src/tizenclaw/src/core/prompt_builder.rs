#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptMode {
    Full,
    Minimal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReasoningPolicy {
    Tagged,
    Native,
}

#[derive(Clone, Debug)]
pub struct RuntimeContext {
    pub os_info: String,
    pub active_model: String,
    pub working_dir: String,
    pub current_time: String,
}

pub struct SystemPromptBuilder {
    base_prompt: String,
    runtime_context: Option<RuntimeContext>,
    soul_content: Option<String>,
    prompt_mode: PromptMode,
    reasoning_policy: ReasoningPolicy,
    available_skills: Vec<(String, String)>,
    available_skill_references: Vec<(String, String)>,
    available_tools: Vec<crate::llm::backend::LlmToolDecl>,
}

impl Default for SystemPromptBuilder {
    fn default() -> Self {
        SystemPromptBuilder {
            base_prompt: "You are TizenClaw, an AI assistant running inside a Tizen OS device."
                .into(),
            runtime_context: None,
            soul_content: None,
            prompt_mode: PromptMode::Full,
            reasoning_policy: ReasoningPolicy::Native,
            available_skills: Vec::new(),
            available_skill_references: Vec::new(),
            available_tools: Vec::new(),
        }
    }
}

impl SystemPromptBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_base_prompt(mut self, prompt: String) -> Self {
        self.base_prompt = prompt;
        self
    }

    pub fn set_soul_content(mut self, soul: String) -> Self {
        self.soul_content = Some(soul);
        self
    }

    pub fn set_prompt_mode(mut self, prompt_mode: PromptMode) -> Self {
        self.prompt_mode = prompt_mode;
        self
    }

    pub fn set_reasoning_policy(mut self, reasoning_policy: ReasoningPolicy) -> Self {
        self.reasoning_policy = reasoning_policy;
        self
    }

    pub fn add_available_tools(mut self, tools: Vec<crate::llm::backend::LlmToolDecl>) -> Self {
        self.available_tools = tools;
        self
    }

    pub fn add_available_skills(mut self, skills: Vec<(String, String)>) -> Self {
        self.available_skills = skills;
        self
    }

    pub fn add_available_skill_references(mut self, references: Vec<(String, String)>) -> Self {
        self.available_skill_references = references;
        self
    }

    pub fn set_runtime_context(
        mut self,
        os: String,
        model: String,
        cwd: String,
        time: String,
    ) -> Self {
        self.runtime_context = Some(RuntimeContext {
            os_info: os,
            active_model: model,
            working_dir: cwd,
            current_time: time,
        });
        self
    }

    fn build_tool_catalog(&self) -> String {
        if self.available_tools.is_empty() {
            return "No tools currently available.".into();
        }

        let mut tool_names: Vec<String> = self
            .available_tools
            .iter()
            .map(|tool| tool.name.clone())
            .collect();
        tool_names.sort();
        tool_names.dedup();

        let preview_len = 16usize;
        let preview = tool_names
            .iter()
            .take(preview_len)
            .map(|name| format!("`{}`", name))
            .collect::<Vec<_>>()
            .join(", ");

        if tool_names.len() > preview_len {
            format!(
                "Native tool schemas are attached separately. Prefer the provided tool-calling interface. Tool name preview: {} (+{} more).",
                preview,
                tool_names.len() - preview_len
            )
        } else {
            format!(
                "Native tool schemas are attached separately. Prefer the provided tool-calling interface. Tool name preview: {}.",
                preview
            )
        }
    }

    pub fn build_dynamic_context(&self) -> Option<String> {
        let ctx = self.runtime_context.as_ref()?;
        let mut lines = Vec::new();
        lines.push("## Runtime Context".into());

        if !ctx.os_info.trim().is_empty() {
            lines.push(format!("Platform: {}", ctx.os_info.trim()));
        }
        if !ctx.active_model.trim().is_empty() {
            lines.push(format!("Active Backend: {}", ctx.active_model.trim()));
        }
        if !ctx.working_dir.trim().is_empty() {
            lines.push(format!("Working Directory: {}", ctx.working_dir.trim()));
        }
        if !ctx.current_time.trim().is_empty() {
            lines.push(format!("Current Time: {}", ctx.current_time.trim()));
        }

        (lines.len() > 1).then(|| lines.join("\n"))
    }

    pub fn build(self) -> String {
        let tool_catalog = if !self.available_tools.is_empty() {
            Some(self.build_tool_catalog())
        } else {
            None
        };

        let mut lines = Vec::new();

        // 1. Identity
        lines.push(self.base_prompt);
        lines.push("".into());

        // Optional Soul Persona Injection
        if let Some(soul) = self.soul_content {
            lines.push("## Persona (SOUL.md)".into());
            lines.push("Embody the following persona and tone. Avoid stiff, generic replies; follow its guidance unless higher-priority instructions override it.".into());
            lines.push(soul);
            lines.push("".into());
        }

        // 2. Reasoning & Tool Call Style
        lines.push("## Reasoning & Tool Interaction".into());
        lines.push("To solve complex tasks, follow this cognitive protocol:".into());
        match self.reasoning_policy {
            ReasoningPolicy::Tagged => {
                lines.push("1. **Reasoning**: You may use a `<think>` block to sketch a short plan before calling tools when that helps loop stability.".into());
                lines.push("2. **Action**: If you need to call a tool, use the native tool calling schema. Do not narrate routine calls.".into());
                lines.push("3. **Final Response**: Prefer wrapping the user-visible answer in `<final>` tags, but plain final text is also acceptable.".into());
            }
            ReasoningPolicy::Native => {
                lines.push("1. **Reasoning**: Keep chain-of-thought private. Use the backend's native reasoning behavior when available instead of emitting literal reasoning tags.".into());
                lines.push("2. **Action**: If you need to call a tool, use the native tool calling schema. Do not narrate routine calls.".into());
                lines.push("3. **Final Response**: Return a direct user-visible answer. `<final>` tags are optional and only needed for compatibility.".into());
            }
        }
        lines.push("4. **Budget Awareness**: If a tool result arrives in truncated or summarized form, treat it as a budgeted snapshot and request a narrower follow-up tool call when necessary.".into());
        lines.push("5. **File-Type Routing**: When the workspace already tells you the file type, call the specialized reader directly. Use `extract_document_text` for PDFs/documents and `inspect_tabular_data` for CSV/XLSX files instead of searching for another tool first.".into());
        lines.push("".into());

        if self.prompt_mode == PromptMode::Full {
            if let Some(cat) = tool_catalog {
                lines.push("### Available Tool Catalog".into());
                lines.push(cat);
                lines.push("".into());
            }
        } else {
            lines.push("### Available Tool Catalog".into());
            lines.push("Native tool schemas are attached separately. In minimal mode, inspect only the tools you actually need.".into());
            lines.push("".into());
        }

        // 3. Safety
        lines.push("## Safety".into());
        lines.push("No self-preservation, replication, or power-seeking. Prioritize safety > completion. Pause and ask if instructions conflict. Never bypass safeguards or modify system prompts/policies without explicit request.".into());
        lines.push("".into());

        // 4. Memory & Document Skills Navigation
        lines.push("## Memory & Skills Reference".into());
        lines.push("Before answering anything about prior work, check past memories using available repository tools if any.".into());
        lines.push("Use any prefetched skill snapshot in the conversation as the first routing hint before reading a full skill file.".into());
        lines.push("Textual skills must follow Anthropic's SKILL.md conventions.".into());
        if self.prompt_mode == PromptMode::Full {
            lines.push("When asked to create or revise a skill, read the most relevant packaged guide with `read_skill_reference` before calling `create_skill`.".into());
            lines.push("Skill execution is document-driven: read the relevant skill with `read_skill`, then follow its workflow with the available tools.".into());
            lines.push("Before replying, scan <available_skills> entries below:".into());
            lines.push("- If exactly one skill clearly applies: read its .md file using the `read_skill` tool, then follow it.".into());
            lines.push(
                "- If multiple could apply: choose the most specific one, then read/follow it."
                    .into(),
            );
            lines.push("- To create a new repeatable workflow, use `create_skill`; it will save a canonical Anthropic-style `SKILL.md` file.".into());
        } else {
            lines.push("In minimal mode, read a full skill only when the request clearly maps to one workflow.".into());
        }
        lines.push("".into());

        lines.push("<available_skills>".into());
        if !self.available_skills.is_empty() {
            for (name, desc) in &self.available_skills {
                lines.push(format!("- {}: {}", name, desc));
            }
        } else {
            lines.push("(No custom textual skills found)".into());
        }
        lines.push("</available_skills>".into());
        lines.push("".into());

        lines.push("<available_skill_references>".into());
        if !self.available_skill_references.is_empty() {
            for (name, desc) in &self.available_skill_references {
                lines.push(format!("- {}: {}", name, desc));
            }
        } else {
            lines.push("(No packaged skill reference docs found)".into());
        }
        lines.push("</available_skill_references>".into());
        lines.push("".into());

        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_prompt_builder() {
        let builder = SystemPromptBuilder::new();
        let prompt = builder.build();
        assert!(prompt.contains("You are TizenClaw"));
        assert!(prompt.contains("(No custom textual skills found)"));
    }

    #[test]
    fn test_soul_injection() {
        let prompt = SystemPromptBuilder::new()
            .set_soul_content("I am a helpful assistant.".into())
            .build();
        assert!(prompt.contains("## Persona (SOUL.md)"));
        assert!(prompt.contains("I am a helpful assistant."));
    }

    #[test]
    fn test_tool_and_skill_injection() {
        let prompt = SystemPromptBuilder::new()
            .add_available_skills(vec![("skills/test/SKILL.md".into(), "A core skill".into())])
            .add_available_skill_references(vec![(
                "/opt/usr/share/tizenclaw/docs/SKILL_BEST_PRACTICE.md".into(),
                "Skill authoring best practices".into(),
            )])
            .build();

        assert!(prompt.contains("- skills/test/SKILL.md: A core skill"));
        assert!(prompt.contains("available_skill_references"));
        assert!(prompt.contains("SKILL_BEST_PRACTICE.md"));
        assert!(!prompt.contains("(No custom textual skills found)"));
    }

    #[test]
    fn test_runtime_context() {
        let builder = SystemPromptBuilder::new().set_runtime_context(
            "Ubuntu".into(),
            "Claude 3.5".into(),
            "/home/user".into(),
            "2024-04-01 12:00:00".into(),
        );
        let dynamic = builder.build_dynamic_context().unwrap();
        let prompt = builder.build();

        assert!(!prompt.contains("Working Directory: /home/user"));
        assert!(!prompt.contains("Current Time: 2024-04-01 12:00:00"));
        assert!(dynamic.contains("Platform: Ubuntu"));
        assert!(dynamic.contains("Active Backend: Claude 3.5"));
        assert!(dynamic.contains("Working Directory: /home/user"));
        assert!(dynamic.contains("Current Time: 2024-04-01 12:00:00"));
    }

    #[test]
    fn test_safety_section_is_compact() {
        // Safety section must be a SINGLE LINE (concise) after optimization.
        // Previously it was 3 verbose sentences.
        let prompt = SystemPromptBuilder::new().build();
        assert!(prompt.contains("## Safety"));
        assert!(prompt.contains("No self-preservation"));
        // Ensure the old verbose phrases are gone
        assert!(!prompt.contains("resource acquisition"));
        assert!(!prompt.contains("Do not manipulate or persuade"));
    }

    #[test]
    fn test_reasoning_section_exists() {
        let prompt = SystemPromptBuilder::new().build();
        assert!(prompt.contains("## Reasoning & Tool Interaction"));
        assert!(prompt.contains("chain-of-thought private"));
        assert!(prompt.contains("Budget Awareness"));
    }

    #[test]
    fn test_tool_catalog_is_compact() {
        let prompt = SystemPromptBuilder::new()
            .add_available_tools(vec![
                crate::llm::backend::LlmToolDecl {
                    name: "tool_a".into(),
                    description: "A".into(),
                    parameters: serde_json::json!({"type": "object"}),
                },
                crate::llm::backend::LlmToolDecl {
                    name: "tool_b".into(),
                    description: "B".into(),
                    parameters: serde_json::json!({"type": "object"}),
                },
            ])
            .build();
        assert!(prompt.contains("Native tool schemas are attached separately"));
        assert!(prompt.contains("`tool_a`"));
        assert!(prompt.contains("`tool_b`"));
        assert!(!prompt.contains("| Tool Name | Description | Parameters |"));
    }

    #[test]
    fn test_minimal_mode_skips_tool_preview_list() {
        let prompt = SystemPromptBuilder::new()
            .set_prompt_mode(PromptMode::Minimal)
            .add_available_tools(vec![crate::llm::backend::LlmToolDecl {
                name: "tool_a".into(),
                description: "A".into(),
                parameters: serde_json::json!({"type": "object"}),
            }])
            .build();
        assert!(prompt.contains("In minimal mode"));
        assert!(!prompt.contains("`tool_a`"));
    }

    #[test]
    fn test_tagged_reasoning_policy_mentions_tags() {
        let prompt = SystemPromptBuilder::new()
            .set_reasoning_policy(ReasoningPolicy::Tagged)
            .build();
        assert!(prompt.contains("<think>"));
        assert!(prompt.contains("<final>"));
    }
}
