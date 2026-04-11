use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PromptFragmentKind {
    System,
    Context,
    Instruction,
    Memory,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptFragment {
    pub kind: PromptFragmentKind,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PromptAssembly {
    pub fragments: Vec<PromptFragment>,
}

impl PromptAssembly {
    pub fn render(&self) -> String {
        self.fragments
            .iter()
            .map(|fragment| fragment.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_assembly_renders_stable_order() {
        let prompt = PromptAssembly {
            fragments: vec![
                PromptFragment {
                    kind: PromptFragmentKind::System,
                    content: "system".to_string(),
                },
                PromptFragment {
                    kind: PromptFragmentKind::Instruction,
                    content: "instruction".to_string(),
                },
            ],
        };

        assert_eq!(prompt.render(), "system\n\ninstruction");
    }
}
