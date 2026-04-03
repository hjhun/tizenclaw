pub struct RuntimeContext {
    pub os_info: String,
    pub active_model: String,
    pub working_dir: String,
}

pub struct SystemPromptBuilder {
    base_prompt: String,
    tool_declarations: Vec<String>,
    runtime_context: Option<RuntimeContext>,
    soul_content: Option<String>,
    available_skills: Vec<(String, String)>,
    long_term_memory: Option<String>,
}

impl Default for SystemPromptBuilder {
    fn default() -> Self {
        SystemPromptBuilder {
            base_prompt: "You are TizenClaw, an AI assistant running inside a Tizen OS device.".into(),
            tool_declarations: Vec::new(),
            runtime_context: None,
            soul_content: None,
            available_skills: Vec::new(),
            long_term_memory: None,
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

    pub fn add_long_term_memory(mut self, memory_str: String) -> Self {
        self.long_term_memory = Some(memory_str);
        self
    }

    pub fn add_tool_names(mut self, tools: Vec<String>) -> Self {
        self.tool_declarations = tools;
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

        // Long-Term Memory Injection
        if let Some(mem) = &self.long_term_memory {
            if !mem.is_empty() {
                lines.push("## Long-Term Memory".into());
                lines.push("Below are persistent memories, facts, and skills you have learned. Use these as primary context.".into());
                lines.push("<long_term_memory>".into());
                lines.push(mem.clone());
                lines.push("</long_term_memory>".into());
                lines.push("".into());
            }
        }

        // 2. Tooling Constraints & Execution Rules
        lines.push("## Tooling".into());
        lines.push("Tool availability (filtered by policy):".into());
        lines.push("Tool names are case-sensitive. Call tools exactly as listed.".into());
        if !self.tool_declarations.is_empty() {
            for t in &self.tool_declarations {
                lines.push(format!("- {}", t));
            }
        } else {
            lines.push("(No tools currently loaded)".into());
        }
        lines.push("".into());
        
        lines.push("## Tool Call Style".into());
        lines.push("Default: do not narrate routine, low-risk tool calls (just call the tool).".into());
        lines.push("Narrate only when it helps: multi-step work, complex/challenging problems, sensitive actions (e.g., deletions), or when explicitly asked.".into());
        lines.push("Keep narration brief and value-dense; avoid repeating obvious steps.".into());
        lines.push("Use plain human language for narration unless in a technical context.".into());
        lines.push("When a first-class tool exists for an action, use the tool directly instead of asking the user to run equivalent CLI commands.".into());
        lines.push("".into());

        // 3. Safety (Anthropic style boundaries)
        lines.push("## Safety".into());
        lines.push("You have no independent goals: do not pursue self-preservation, replication, resource acquisition, or power-seeking; avoid long-term plans beyond the user's request.".into());
        lines.push("Prioritize safety and human oversight over completion; if instructions conflict, pause and ask; comply with stop/pause/audit requests and never bypass safeguards.".into());
        lines.push("Do not manipulate or persuade anyone to expand access or disable safeguards. Do not copy yourself or change system prompts, safety rules, or tool policies unless explicitly requested.".into());
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
        assert!(prompt.contains("(No tools currently loaded)"));
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
            .add_tool_names(vec!["tool_a".into(), "tool_b".into()])
            .add_available_skills(vec![("skills/test/SKILL.md".into(), "A core skill".into())])
            .build();
        
        assert!(prompt.contains("- tool_a"));
        assert!(prompt.contains("- tool_b"));
        assert!(!prompt.contains("(No tools currently loaded)"));
        
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
}
