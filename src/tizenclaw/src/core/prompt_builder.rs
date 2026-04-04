pub struct RuntimeContext {
    pub os_info: String,
    pub active_model: String,
    pub working_dir: String,
}

pub struct SystemPromptBuilder {
    base_prompt: String,
    runtime_context: Option<RuntimeContext>,
    soul_content: Option<String>,
    available_skills: Vec<(String, String)>,
    available_tools: Vec<crate::llm::backend::LlmToolDecl>,
}

impl Default for SystemPromptBuilder {
    fn default() -> Self {
        SystemPromptBuilder {
            base_prompt: "You are TizenClaw, an AI assistant running inside a Tizen OS device.".into(),
            runtime_context: None,
            soul_content: None,
            available_skills: Vec::new(),
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

    pub fn add_available_tools(mut self, tools: Vec<crate::llm::backend::LlmToolDecl>) -> Self {
        self.available_tools = tools;
        self
    }

    pub fn add_available_skills(mut self, skills: Vec<(String, String)>) -> Self {
        self.available_skills = skills;
        self
    }

    pub fn set_runtime_context(mut self, os: String, model: String, cwd: String) -> Self {
        self.runtime_context = Some(RuntimeContext {
            os_info: os,
            active_model: model,
            working_dir: cwd,
        });
        self
    }

    pub fn build(self) -> String {
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



        // 2. Tool Call Style
        lines.push("## Tool Call Style".into());
        lines.push("Skip narration for routine calls. Use the native tool calling schema to execute system actions.".into());
        lines.push("".into());

        // 3. Safety
        lines.push("## Safety".into());
        lines.push("No self-preservation, replication, or power-seeking. Prioritize safety > completion. Pause and ask if instructions conflict. Never bypass safeguards or modify system prompts/policies without explicit request.".into());
        lines.push("".into());

        // 4. Memory & Document Skills Navigation
        lines.push("## Memory & Skills Reference".into());
        lines.push("Before answering anything about prior work, check past memories using available repository tools if any.".into());
        lines.push("Before replying, scan <available_skills> entries below:".into());
        lines.push("- If exactly one skill clearly applies: read its .md file using the `read_skill` tool, then follow it.".into());
        lines.push("- If multiple could apply: choose the most specific one, then read/follow it.".into());
        lines.push("- To create a new repeatable workflow, simply use your `create_skill` tool to save a new textual skill!".into());
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

        // 5. Platform Runtime Metadata
        if let Some(ctx) = self.runtime_context {
            lines.push("## Workspace Context & Runtime Metadata".into());
            lines.push(format!("Working Directory: {}", ctx.working_dir));
            lines.push("Treat this directory as the single global workspace for file operations unless explicitly instructed otherwise.".into());
            lines.push("If you need to know the current exact date and time, run the 'date' shell command via the exec tool.".into());
            lines.push(format!("Runtime Environment: os='{}' | active_model='{}'", ctx.os_info, ctx.active_model));
            lines.push("".into());
        }

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
            .build();
        
        assert!(prompt.contains("- skills/test/SKILL.md: A core skill"));
        assert!(!prompt.contains("(No custom textual skills found)"));
    }

    #[test]
    fn test_runtime_context() {
        let prompt = SystemPromptBuilder::new()
            .set_runtime_context("Ubuntu".into(), "Claude 3.5".into(), "/home/user".into())
            .build();
        
        assert!(prompt.contains("Working Directory: /home/user"));
        assert!(prompt.contains("os='Ubuntu'"));
        assert!(prompt.contains("active_model='Claude 3.5'"));
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
    fn test_tool_call_style_is_compact() {
        let prompt = SystemPromptBuilder::new().build();
        assert!(prompt.contains("## Tool Call Style"));
        // Must be a single-line description after optimization
        assert!(prompt.contains("Skip narration for routine calls"));
        // Ensure the old verbose multi-line form is gone
        assert!(!prompt.contains("Keep narration brief and value-dense"));
    }

    #[test]
    fn test_prompt_length_reduced() {
        // Verify that the optimized prompt is shorter than the old reference.
        // Old Safety+ToolCallStyle = ~6 lines. New = ~2 lines.
        // Use character count as proxy for token count.
        let prompt = SystemPromptBuilder::new().build();
        // Baseline: must contain all key sections
        assert!(prompt.contains("## Safety"));
        assert!(prompt.contains("## Memory & Skills Reference"));
        // The combined Safety + ToolCallStyle sections should be < 400 chars
        let safety_start = prompt.find("## Tool Call Style").unwrap_or(0);
        let memory_start = prompt.find("## Memory & Skills Reference").unwrap_or(prompt.len());
        let section_len = memory_start.saturating_sub(safety_start);
        assert!(
            section_len < 400,
            "Tool Call Style + Safety sections should be < 400 chars, got {}",
            section_len
        );
    }
}
