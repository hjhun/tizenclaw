//! Tool indexer — indexes and searches available tools for relevance filtering.

use crate::llm::backend::LlmToolDecl;

pub struct ToolIndexer {
    tools: Vec<LlmToolDecl>,
}

impl ToolIndexer {
    pub fn new() -> Self {
        ToolIndexer { tools: vec![] }
    }

    /// Update the index with new tool declarations.
    pub fn update(&mut self, tools: Vec<LlmToolDecl>) {
        self.tools = tools;
        log::info!("ToolIndexer: indexed {} tools", self.tools.len());
    }

    /// Get all indexed tools.
    pub fn get_all(&self) -> &[LlmToolDecl] {
        &self.tools
    }

    /// Search for tools matching a query by name or description.
    pub fn search(&self, query: &str, max_results: usize) -> Vec<&LlmToolDecl> {
        let query_lower = query.to_lowercase();
        let words: Vec<&str> = query_lower.split_whitespace().collect();

        let mut scored: Vec<(usize, &LlmToolDecl)> = self.tools.iter()
            .map(|t| {
                let name_lower = t.name.to_lowercase();
                let desc_lower = t.description.to_lowercase();
                let mut score = 0usize;

                // Exact name match = highest score
                if name_lower == query_lower { score += 100; }
                // Name contains query
                if name_lower.contains(&query_lower) { score += 50; }

                // Word matches in name and description
                for word in &words {
                    if name_lower.contains(word) { score += 20; }
                    if desc_lower.contains(word) { score += 5; }
                }
                (score, t)
            })
            .filter(|(score, _)| *score > 0)
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().take(max_results).map(|(_, t)| t).collect()
    }

    /// Filter tools relevant to a given prompt.
    pub fn filter_relevant(&self, prompt: &str, max_tools: usize) -> Vec<LlmToolDecl> {
        if self.tools.len() <= max_tools {
            return self.tools.clone();
        }

        let results = self.search(prompt, max_tools);
        if results.is_empty() {
            // Fallback: return first N tools
            self.tools.iter().take(max_tools).cloned().collect()
        } else {
            results.into_iter().cloned().collect()
        }
    }
}
