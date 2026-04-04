//! Tool indexer — indexes available tools and generates documentation.
//!
//! Provides two complementary capabilities:
//! 1. In-memory search/filtering over `LlmToolDecl` (used by AgentCore)
//! 2. Filesystem-based metadata scanning + LLM-assisted markdown generation
//!    for `tools.md` and per-directory `index.md` files.

use crate::llm::backend::LlmToolDecl;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;

// ─────────────────────────────────────────────────────────────
// Data types for filesystem-based tool metadata
// ─────────────────────────────────────────────────────────────

/// Metadata for a single tool or skill discovered on disk.
#[derive(Clone, Debug)]
pub struct ToolMeta {
    pub name: String,
    pub description: String,
    pub category: String,
    pub dir_path: String,
    pub binary_path: Option<String>,
    pub commands: Vec<String>,
}

/// Metadata for a single subdirectory containing tools.
#[derive(Clone, Debug)]
pub struct CategoryMeta {
    pub name: String,
    pub dir_path: String,
    pub tools: Vec<ToolMeta>,
}

/// Complete metadata for the entire tools root directory.
#[derive(Clone, Debug)]
pub struct ToolsMetadata {
    pub root_dir: String,
    pub categories: Vec<CategoryMeta>,
}

impl ToolsMetadata {
    /// Total number of tools/skills discovered across all categories.
    pub fn total_tools(&self) -> usize {
        self.categories.iter().map(|c| c.tools.len()).sum()
    }
}

// ─────────────────────────────────────────────────────────────
// In-memory search (existing functionality, preserved)
// ─────────────────────────────────────────────────────────────

pub struct ToolIndexer {
    tools: Vec<LlmToolDecl>,
}

impl Default for ToolIndexer {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolIndexer {
    pub fn new() -> Self {
        ToolIndexer { tools: vec![] }
    }

    /// Update the index with new tool declarations.
    pub fn update(&mut self, tools: Vec<LlmToolDecl>) {
        self.tools = tools;
        log::debug!("ToolIndexer: indexed {} tools", self.tools.len());
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

// ─────────────────────────────────────────────────────────────
// Filesystem-based metadata scanning + LLM index generation
// ─────────────────────────────────────────────────────────────

const HASH_FILE: &str = ".index_hash";
const EMBEDDED_CATEGORY_NAME: &str = "embedded";

/// Scan all tool/skill metadata from a root directory.
///
/// Iterates every immediate subdirectory of `root_dir` (e.g. `cli/`,
/// `embedded/`, `skills/`) and within each, looks for per-tool
/// subdirectories containing `tool.md`, `index.md`, or `SKILL.md`
/// descriptors.
pub fn scan_tools_metadata(root_dir: &str) -> ToolsMetadata {
    let root = Path::new(root_dir);
    let mut categories = Vec::new();

    if !root.exists() || !root.is_dir() {
        log::warn!("ToolIndexer: root dir '{}' does not exist", root_dir);
        return ToolsMetadata { root_dir: root_dir.to_string(), categories };
    }

    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("ToolIndexer: cannot read root dir '{}': {}", root_dir, e);
            return ToolsMetadata { root_dir: root_dir.to_string(), categories };
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let cat_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        // Skip hidden directories and non-tool directories
        if cat_name.starts_with('.') || cat_name == "CMakeFiles" {
            continue;
        }

        let mut tools = Vec::new();
        if let Ok(sub_entries) = std::fs::read_dir(&path) {
            for sub_entry in sub_entries.flatten() {
                let sub_path = sub_entry.path();
                if !sub_path.is_dir() {
                    continue;
                }
                let sub_name = sub_path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                if sub_name.starts_with('.') || sub_name == "CMakeFiles" {
                    continue;
                }

                tools.extend(parse_tool_dir(&sub_path, &cat_name));
            }
        }

        // Also check if the category dir itself has a descriptor
        // (e.g. embedded/ has .md files directly, not subdirs)
        if tools.is_empty() {
            // Check for flat .md files (like embedded/ directory pattern)
            if let Ok(flat_entries) = std::fs::read_dir(&path) {
                for flat_entry in flat_entries.flatten() {
                    let fp = flat_entry.path();
                    if fp.is_file() {
                        let fname = fp.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("")
                            .to_string();
                        if fname.ends_with(".md")
                            && fname != "index.md"
                            && fname != "tools.md"
                            && fname != "CMakeLists.txt"
                        {
                            let tool_name = fname.trim_end_matches(".md").to_string();
                            let desc = extract_description_from_md(&fp);
                            tools.push(ToolMeta {
                                name: tool_name,
                                description: desc,
                                category: cat_name.clone(),
                                dir_path: path.to_string_lossy().to_string(),
                                binary_path: None,
                                commands: vec![],
                            });
                        }
                    }
                }
            }
        }

        if !tools.is_empty() {
            categories.push(CategoryMeta {
                name: cat_name,
                dir_path: path.to_string_lossy().to_string(),
                tools,
            });
        }
    }

    log::info!(
        "ToolIndexer: scanned {} categories, {} total tools from '{}'",
        categories.len(),
        categories.iter().map(|c| c.tools.len()).sum::<usize>(),
        root_dir,
    );

    ToolsMetadata { root_dir: root_dir.to_string(), categories }
}

/// Scan metadata from the standard tools root plus an optional flat
/// embedded-descriptor root.
pub fn scan_tools_metadata_with_embedded(
    root_dir: &str,
    embedded_dir: Option<&str>,
) -> ToolsMetadata {
    let mut metadata = scan_tools_metadata(root_dir);
    metadata
        .categories
        .retain(|category| category.name != EMBEDDED_CATEGORY_NAME);

    if let Some(dir) = embedded_dir {
        if let Some(category) = scan_flat_markdown_category(dir, EMBEDDED_CATEGORY_NAME) {
            metadata.categories.push(category);
        }
    }

    metadata
}

/// Parse a single tool directory for metadata.
fn parse_tool_dir(dir: &Path, category: &str) -> Vec<ToolMeta> {
    let dir_name_str = dir.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string();
    let dir_name = dir_name_str.clone();

    // Look for descriptors in priority order
    let descriptor_names = ["tool.md", "SKILL.md", "index.md"];
    let mut content = String::new();

    for name in &descriptor_names {
        let p = dir.join(name);
        if p.exists() {
            if let Ok(c) = std::fs::read_to_string(&p) {
                content = c;
                break;
            }
        }
    }

    let mut name = String::new();
    let mut description = String::new();
    let mut binary_path = None;
    let mut commands = Vec::new();

    if content.is_empty() {
        // No main descriptor, try parsing all .md files as individual actions/tools
        let mut results = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() {
                    let fname = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                    if fname.ends_with(".md") && fname != "index.md" && fname != "tools.md" && !fname.starts_with('.') {
                        let action_name = fname.trim_end_matches(".md").to_string();
                        let desc = extract_description_from_md(&p);
                        results.push(ToolMeta {
                            name: action_name.clone(),
                            description: format!("[Action: {}] {}", action_name, desc),
                            category: category.to_string(),
                            dir_path: dir.to_string_lossy().to_string(),
                            binary_path: None,
                            commands: vec![fname],
                        });
                    }
                }
            }
        }
        if !results.is_empty() {
            return results;
        }
    }

    // Parse YAML frontmatter and content
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(v) = trimmed.strip_prefix("name:") {
            name = v.trim().trim_matches('"').to_string();
        } else if let Some(v) = trimmed.strip_prefix("description:") {
            description = v.trim().trim_matches('"').to_string();
        } else if let Some(v) = trimmed.strip_prefix("binary:") {
            binary_path = Some(v.trim().trim_matches('"').to_string());
        } else if trimmed.starts_with("# ") && name.is_empty() {
            name = trimmed[2..].trim().to_string();
        } else if trimmed.starts_with("| `") && trimmed.contains('|') {
            // Table row — likely a command reference
            let parts: Vec<&str> = trimmed.split('|').collect();
            if parts.len() >= 3 {
                let cmd = parts[1].trim().trim_matches('`').trim();
                if !cmd.is_empty() && cmd != "Command" {
                    commands.push(cmd.to_string());
                }
            }
        }
    }

    if name.is_empty() {
        name = dir_name.clone();
    }
    if description.is_empty() && !content.is_empty() {
        // Use first non-empty, non-heading line as description
        for line in content.lines() {
            let t = line.trim();
            if !t.is_empty() && !t.starts_with('#') && !t.starts_with("---")
                && !t.starts_with("name:") && !t.starts_with("description:")
            {
                description = if t.len() > 200 { t[..200].to_string() } else { t.to_string() };
                break;
            }
        }
    }

    // Check for co-located binary
    if binary_path.is_none() {
        let local_bin = dir.join(&dir_name);
        if local_bin.exists() && local_bin.is_file() {
            binary_path = Some(local_bin.to_string_lossy().to_string());
        }
    }

    vec![ToolMeta {
        name,
        description,
        category: category.to_string(),
        dir_path: dir.to_string_lossy().to_string(),
        binary_path,
        commands,
    }]
}

fn scan_flat_markdown_category(dir: &str, category_name: &str) -> Option<CategoryMeta> {
    let path = Path::new(dir);
    if !path.exists() || !path.is_dir() {
        return None;
    }

    let entries = std::fs::read_dir(path).ok()?;
    let mut tools = Vec::new();

    for entry in entries.flatten() {
        let file_path = entry.path();
        if !file_path.is_file() {
            continue;
        }

        let file_name = file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if !file_name.ends_with(".md")
            || file_name == "index.md"
            || file_name == "tools.md"
            || file_name.starts_with('.')
        {
            continue;
        }

        tools.push(ToolMeta {
            name: file_name.trim_end_matches(".md").to_string(),
            description: extract_description_from_md(&file_path),
            category: category_name.to_string(),
            dir_path: path.to_string_lossy().to_string(),
            binary_path: None,
            commands: vec![],
        });
    }

    if tools.is_empty() {
        return None;
    }

    tools.sort_by(|left, right| left.name.cmp(&right.name));

    Some(CategoryMeta {
        name: category_name.to_string(),
        dir_path: path.to_string_lossy().to_string(),
        tools,
    })
}

/// Extract a single-line description from a markdown file.
fn extract_description_from_md(path: &Path) -> String {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    // Try YAML frontmatter first
    for line in content.lines() {
        let t = line.trim();
        if let Some(v) = t.strip_prefix("description:") {
            return v.trim().trim_matches('"').to_string();
        }
    }

    // Fallback: first non-heading content line
    for line in content.lines() {
        let t = line.trim();
        if !t.is_empty() && !t.starts_with('#') && !t.starts_with("---") {
            return if t.len() > 200 { t[..200].to_string() } else { t.to_string() };
        }
    }

    String::new()
}

/// Check whether the tool directory has changed since the last indexing.
/// Returns `true` if re-indexing is needed.
pub fn needs_reindex(root_dir: &str) -> bool {
    needs_reindex_for_roots(root_dir, &[root_dir])
}

/// Check whether any scanned tool roots changed since the last indexing.
/// The hash file is stored under `hash_root_dir`.
pub fn needs_reindex_for_roots(hash_root_dir: &str, scan_roots: &[&str]) -> bool {
    let root = Path::new(hash_root_dir);
    let hash_path = root.join(HASH_FILE);

    let current_hash = compute_roots_hash(scan_roots);

    if let Ok(stored) = std::fs::read_to_string(&hash_path) {
        let stored_hash: u64 = stored.trim().parse().unwrap_or(0);
        if stored_hash == current_hash {
            return false;
        }
    }

    true
}

/// Save the current directory hash after successful indexing.
pub fn save_index_hash(root_dir: &str) {
    save_index_hash_for_roots(root_dir, &[root_dir]);
}

/// Save the current multi-root hash after successful indexing.
pub fn save_index_hash_for_roots(hash_root_dir: &str, scan_roots: &[&str]) {
    let root = Path::new(hash_root_dir);
    let hash_path = root.join(HASH_FILE);
    let hash = compute_roots_hash(scan_roots);
    let _ = std::fs::write(&hash_path, hash.to_string());
}

/// Compute a fast hash of the directory tree structure + modification times.
fn compute_dir_hash(dir: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();

    fn walk(dir: &Path, hasher: &mut DefaultHasher, depth: usize) {
        if depth > 3 { return; }
        if let Ok(entries) = std::fs::read_dir(dir) {
            let mut names: Vec<(String, std::time::SystemTime)> = entries
                .flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    // Skip hidden files and generated index artifacts
                    if name.starts_with('.')
                        || name == "tools.md"
                        || name == "index.md"
                    {
                        return None;
                    }
                    let modified = e.metadata().ok()?.modified().ok()?;
                    Some((name, modified))
                })
                .collect();
            names.sort_by(|a, b| a.0.cmp(&b.0));
            for (name, modified) in &names {
                name.hash(hasher);
                modified.hash(hasher);
            }
            // Recurse into subdirs
            if let Ok(entries2) = std::fs::read_dir(dir) {
                for e in entries2.flatten() {
                    if e.path().is_dir() {
                        let n = e.file_name().to_string_lossy().to_string();
                        if !n.starts_with('.') && n != "CMakeFiles" {
                            walk(&e.path(), hasher, depth + 1);
                        }
                    }
                }
            }
        }
    }

    walk(dir, &mut hasher, 0);
    hasher.finish()
}

fn compute_roots_hash(root_dirs: &[&str]) -> u64 {
    let mut hasher = DefaultHasher::new();
    let mut roots = root_dirs.to_vec();
    roots.sort();

    for root in roots {
        root.hash(&mut hasher);
        compute_dir_hash(Path::new(root)).hash(&mut hasher);
    }

    hasher.finish()
}

/// Build a structured prompt for the LLM to generate `tools.md` and
/// per-category `index.md` files from the scanned metadata.
///
/// The prompt instructs the LLM to output a single JSON containing
/// the markdown content for each file, enabling deterministic parsing.
pub fn build_indexing_prompt(metadata: &ToolsMetadata) -> String {
    let mut prompt = String::new();

    prompt.push_str(
        "You are a documentation generator for the TizenClaw AI Agent system. \
         Based on the tool metadata provided below, generate high-quality \
         markdown documentation files.\n\n"
    );

    prompt.push_str("## Scanned Tool Metadata\n\n");

    for cat in &metadata.categories {
        prompt.push_str(&format!("### Category: `{}` ({} tools)\n", cat.name, cat.tools.len()));
        for tool in &cat.tools {
            prompt.push_str(&format!("- **{}**: {}\n", tool.name, tool.description));
            if let Some(bin) = &tool.binary_path {
                prompt.push_str(&format!("  - Binary: `{}`\n", bin));
            }
            if !tool.commands.is_empty() {
                prompt.push_str(&format!(
                    "  - Commands: {}\n",
                    tool.commands.iter()
                        .take(10)
                        .map(|c| format!("`{}`", c))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        prompt.push('\n');
    }

    prompt.push_str(
        "## Output Requirements\n\n\
         Generate a JSON object with the following structure:\n\
         ```json\n\
         {\n\
           \"tools_md\": \"<full markdown content for tools.md>\",\n\
           \"indices\": {\n\
             \"<category_name>\": \"<full markdown content for that category's index.md>\"\n\
           }\n\
         }\n\
         ```\n\n\
         ### tools.md Requirements:\n\
         - Title: `# TizenClaw Tool Catalog`\n\
         - Include a summary table showing each category, tool count, and brief description\n\
         - For each category, list all tools with name, description, and key capabilities\n\
         - Include the generation timestamp\n\
         - Use clear markdown tables for structured data\n\
         - The content should allow a user or agent to IMMEDIATELY understand \
           what capabilities are available\n\n\
         ### index.md Requirements (per category):\n\
         - If the category name is `actions`, you MUST create a table with the exact columns: `| Action Name | Description | Markdown |`.\n\
         - In the `actions` table, the `Markdown` column must use the filename provided in the `Commands` list from the metadata (e.g., `[launch.md](launch.md)` or similar if appropriate).\n\
         - For all other categories, format with a detailed tool reference table with commands/parameters if available.\n\
         - Usage examples where applicable\n\
         - Installation path information\n\n\
         Output ONLY the raw JSON object. Do NOT wrap it in markdown code blocks."
    );

    prompt
}

/// Parse the LLM response JSON and write the generated files to disk.
///
/// Returns the number of files successfully written.
pub fn apply_llm_index_result(result: &str, root_dir: &str, metadata: &ToolsMetadata) -> usize {
    let clean = result.trim();
    // Strip markdown code fences if present
    let json_str = if clean.starts_with("```json") {
        clean.trim_start_matches("```json").trim_end_matches("```").trim()
    } else if clean.starts_with("```") {
        clean.trim_start_matches("```").trim_end_matches("```").trim()
    } else {
        clean
    };

    let parsed: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(e) => {
            log::error!("ToolIndexer: Failed to parse LLM index response: {}", e);
            return 0;
        }
    };

    let root = Path::new(root_dir);
    let mut written = 0;

    // Write tools.md
    if let Some(tools_md) = parsed.get("tools_md").and_then(|v| v.as_str()) {
        let path = root.join("tools.md");
        match std::fs::write(&path, tools_md) {
            Ok(_) => {
                log::info!("ToolIndexer: wrote {}", path.display());
                written += 1;
            }
            Err(e) => log::error!("ToolIndexer: failed to write tools.md: {}", e),
        }
    }

    // Write per-category index.md
    if let Some(indices) = parsed.get("indices").and_then(|v| v.as_object()) {
        for (cat_name, content) in indices {
            if let Some(md_content) = content.as_str() {
                let cat_dir = metadata
                    .categories
                    .iter()
                    .find(|category| category.name == *cat_name)
                    .map(|category| Path::new(&category.dir_path).to_path_buf())
                    .unwrap_or_else(|| root.join(cat_name));
                if cat_dir.exists() && cat_dir.is_dir() {
                    let index_path = cat_dir.join("index.md");
                    match std::fs::write(&index_path, md_content) {
                        Ok(_) => {
                            log::info!("ToolIndexer: wrote {}", index_path.display());
                            written += 1;
                        }
                        Err(e) => log::error!(
                            "ToolIndexer: failed to write {}/index.md: {}",
                            cat_name, e
                        ),
                    }
                }
            }
        }
    }

    written
}

/// Generate a minimal fallback `tools.md` without LLM assistance.
/// Used when no LLM backend is available.
pub fn generate_fallback_index(metadata: &ToolsMetadata, root_dir: &str) {
    let root = Path::new(root_dir);
    let mut md = String::new();

    md.push_str("# TizenClaw Tool Catalog\n\n");
    md.push_str(&format!(
        "> Auto-generated at startup | {} tools across {} categories\n\n",
        metadata.total_tools(),
        metadata.categories.len(),
    ));

    md.push_str("## Summary\n\n");
    md.push_str("| Category | Tool Count | Description |\n");
    md.push_str("|----------|-----------|-------------|\n");
    for cat in &metadata.categories {
        md.push_str(&format!(
            "| {} | {} | {} |\n",
            cat.name,
            cat.tools.len(),
            cat.tools.iter().take(3).map(|t| t.name.as_str()).collect::<Vec<_>>().join(", "),
        ));
    }
    md.push('\n');

    for cat in &metadata.categories {
        md.push_str(&format!("## {}\n\n", cat.name));
        md.push_str("| Tool | Description |\n");
        md.push_str("|------|-------------|\n");
        for tool in &cat.tools {
            let desc = if tool.description.len() > 80 {
                format!("{}...", &tool.description[..77])
            } else {
                tool.description.clone()
            };
            md.push_str(&format!("| {} | {} |\n", tool.name, desc));
        }
        md.push('\n');
    }

    let tools_md_path = root.join("tools.md");
    if let Err(e) = std::fs::write(&tools_md_path, &md) {
        log::error!("ToolIndexer: fallback write failed: {}", e);
    } else {
        log::info!("ToolIndexer: wrote fallback tools.md ({} bytes)", md.len());
    }
}

// ─────────────────────────────────────────────────────────────
// Fallback: Live tool query from filesystem (no index files)
// ─────────────────────────────────────────────────────────────

/// Query available tools directly from the filesystem without relying
/// on pre-generated `tools.md` or `index.md` files.
///
/// This is the **last-resort fallback** — used when the startup indexer
/// fails to generate documentation (e.g. LLM offline, disk full, etc.).
/// Returns a formatted markdown string listing all discovered tools.
pub fn query_tools_live(root_dir: &str) -> String {
    let metadata = scan_tools_metadata(root_dir);

    if metadata.total_tools() == 0 {
        return "No tools found in the tools directory.".to_string();
    }

    let mut result = String::new();
    result.push_str(&format!(
        "# Available Tools ({} total)\n\n",
        metadata.total_tools(),
    ));

    for cat in &metadata.categories {
        result.push_str(&format!("## {} ({} tools)\n\n", cat.name, cat.tools.len()));
        for tool in &cat.tools {
            result.push_str(&format!("- **{}**", tool.name));
            if !tool.description.is_empty() {
                result.push_str(&format!(": {}", tool.description));
            }
            if let Some(bin) = &tool.binary_path {
                result.push_str(&format!(" (`{}`)", bin));
            }
            result.push('\n');
        }
        result.push('\n');
    }

    result
}

/// Try to read tools.md; if unavailable, perform a live filesystem
/// query as fallback.  Returns the tool catalog as a markdown string.
pub fn get_tool_catalog(root_dir: &str) -> String {
    let root = Path::new(root_dir);
    let tools_md = root.join("tools.md");

    // Try reading the pre-generated index first
    if let Ok(content) = std::fs::read_to_string(&tools_md) {
        if !content.trim().is_empty() {
            return content;
        }
    }

    // Fallback: live query
    log::info!(
        "ToolIndexer: tools.md not available, performing live query"
    );
    query_tools_live(root_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn scan_tools_metadata_with_embedded_adds_flat_category() {
        let tools_root = TempDir::new().unwrap();
        let cli_dir = tools_root.path().join("cli");
        let tool_dir = cli_dir.join("sample-cli");
        std::fs::create_dir_all(&tool_dir).unwrap();
        std::fs::write(
            tool_dir.join("tool.md"),
            "name: sample-cli\ndescription: Sample CLI tool\n",
        )
        .unwrap();

        let embedded_root = TempDir::new().unwrap();
        std::fs::write(
            embedded_root.path().join("create_task.md"),
            "# create_task\n\nCreate a task.\n",
        )
        .unwrap();

        let metadata = scan_tools_metadata_with_embedded(
            &tools_root.path().to_string_lossy(),
            Some(&embedded_root.path().to_string_lossy()),
        );

        assert_eq!(metadata.categories.len(), 2);
        assert!(metadata.categories.iter().any(|category| category.name == "cli"));
        let embedded = metadata
            .categories
            .iter()
            .find(|category| category.name == "embedded")
            .unwrap();
        assert_eq!(embedded.tools.len(), 1);
        assert_eq!(embedded.tools[0].name, "create_task");
    }

    #[test]
    fn apply_llm_index_result_writes_embedded_index_to_embedded_dir() {
        let tools_root = TempDir::new().unwrap();
        std::fs::create_dir_all(tools_root.path().join("cli")).unwrap();

        let embedded_root = TempDir::new().unwrap();
        std::fs::write(
            embedded_root.path().join("create_task.md"),
            "# create_task\n\nCreate a task.\n",
        )
        .unwrap();

        let metadata = scan_tools_metadata_with_embedded(
            &tools_root.path().to_string_lossy(),
            Some(&embedded_root.path().to_string_lossy()),
        );
        let result = "{\n\
          \"tools_md\": \"# TizenClaw Tool Catalog\\n\",\n\
          \"indices\": {\n\
            \"embedded\": \"# Embedded\\n\"\n\
          }\n\
        }";

        let written = apply_llm_index_result(
            result,
            &tools_root.path().to_string_lossy(),
            &metadata,
        );

        assert_eq!(written, 2);
        assert!(embedded_root.path().join("index.md").exists());
        assert!(!tools_root.path().join("embedded/index.md").exists());
    }

    #[test]
    fn scan_tools_metadata_with_embedded_ignores_legacy_root_category() {
        let tools_root = TempDir::new().unwrap();
        let legacy_embedded = tools_root.path().join("embedded");
        std::fs::create_dir_all(&legacy_embedded).unwrap();
        std::fs::write(
            legacy_embedded.join("create_task.md"),
            "# create_task\n\nLegacy descriptor.\n",
        )
        .unwrap();

        let embedded_root = TempDir::new().unwrap();
        std::fs::write(
            embedded_root.path().join("create_task.md"),
            "# create_task\n\nNew descriptor.\n",
        )
        .unwrap();

        let metadata = scan_tools_metadata_with_embedded(
            &tools_root.path().to_string_lossy(),
            Some(&embedded_root.path().to_string_lossy()),
        );

        let embedded_categories = metadata
            .categories
            .iter()
            .filter(|category| category.name == "embedded")
            .count();
        assert_eq!(embedded_categories, 1);
        assert_eq!(
            metadata
                .categories
                .iter()
                .find(|category| category.name == "embedded")
                .unwrap()
                .dir_path,
            embedded_root.path().to_string_lossy()
        );
    }
}
