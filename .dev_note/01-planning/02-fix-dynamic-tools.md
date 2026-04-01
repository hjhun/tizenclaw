# Planning: Fix CLI Tool Routing & Dynamic Watcher Recursion

## 1. Cognitive Requirements & Target Analysis
Following the explicit removal of the monolithic `routing_guide.md`, the TizenClaw agent suffered a 100% loss of native Tizen API bridging capabilities via `execute_cli_*` wrappers.
This occurred due to `ToolDispatcher` employing misaligned YAML-frontmatter heuristics on unstructured embedded markdown, causing a null fallback mapping. The LLM then hallucinates Unix standards due to sheer architectural ignorance.
Additionally, the user explicitly commanded the addition of a dynamic "tizen-tools update refresh" functionality.

## 2. Agent Capabilities Context
- Extraneous metadata scraping must bridge embedded markdown into runtime LLM schemas identically to how RAG chunking works, ensuring `<tool_descriptions>` map optimally without external JSON.
- The `ToolWatcher` must abandon its root-only O(1) depth limit and adopt O(n) multi-level subdirectory walk strategies to dynamically intercept SDK pushes.

## 3. Agent System Integration Planning
- **Module Convention:** `src/tizenclaw/src/core/tool_dispatcher.rs` and `src/tizenclaw/src/core/tool_watcher.rs`.
- **Execution Mode:** Rust implementation updates.
- **Environmental Context:** The embedded Tizen QEMU build targets `/opt/usr/share/tizen-tools/cli/*.` which fundamentally contradicts the current `/usr/bin/` hardcode parsing fallback.
