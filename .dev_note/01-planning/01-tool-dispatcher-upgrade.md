# Planning: TizenClaw Tool Dispatcher Upgrade

## 1. Issue Analysis
In the previous E2E test cycle, the `tizenclaw-cli` language agent failed to construct accurate parameters for several CLI utilities (e.g., `tizen-sensor-cli`, `tizen-web-search-cli`) despite strict usage guidelines being appended to their `tool.md` manifests.
Upon auditing the Rust `ToolDispatcher` logic (`src/tizenclaw/src/core/tool_dispatcher.rs`), we identified two fatal flaws:
1. **Context Truncation**: `parse_tool_md()` only extracts a single line starting with `Description: ` to pass to the LLM tool declaration. The entire `tool.md` usage tables, subcommand options, and critical parameter instructions are ignored and never transmitted to the cognitive backend.
2. **Whitespace Splitting**: `cmd_args` resolution uses `.split_whitespace()`. This inherently breaks any quoted arguments with spaces (e.g., `--query "Hello World"` becomes `--query`, `"Hello`, `World"`).

## 2. Requirements & Capabilities List
- **Full Markdown Context Mapping**: The `ToolDispatcher` must compile the entire contents of `tool.md` into the `description` field of the Tool declaration, ensuring the LLM gains total structural awareness of the target endpoint.
- **Safe Argument Parsing**: We need to replace `split_whitespace()` with a robust quote-aware splitting mechanism, or restructure the JSON schema to use object properties properly if possible. However, given we want to keep the single `args` string approach for generic CLI mapping, a quote-aware parsing approach (similar to `shlex`) is mandatory.

## 3. Integration Plan
- **Module Convention**: `tizenclaw` core subsystem (`src/tizenclaw/src/core/tool_dispatcher.rs`).
- **Execution Mode Classification**: **Daemon Sub-task** / **One-shot Worker** logic paths managing tool routing boundaries.
- **Design Goals**: Retain Zero-Cost abstractions while expanding cognitive context window injection gracefully.

Next phase is Design to outline the Rust changes.
