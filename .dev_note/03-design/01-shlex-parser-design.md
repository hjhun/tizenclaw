# ToolDispatcher Architecture Design (Quote-Aware Arguments)

## 1. Problem
Currently, `ToolDispatcher::execute` parses parameters from the LLM via `.split_whitespace()`.
```rust
if let Some(args_str) = args.get("args").and_then(|v| v.as_str()) {
    cmd_args = args_str.split_whitespace().map(|s| s.to_string()).collect();
}
```
This fails for LLM calls with string values: `tizenclaw-cli "search web for --query 'Tizen 10.0'"` becomes `--query`, `'Tizen`, `10.0'`.

## 2. Zero-Cost Abstraction Resolution (Shlex Alternative)
We need a simple inline parsing logic (since pulling the external `shlex` crate might require a `Cargo.toml` modification which we prefer to avoid if there are strict zero dependency rules, but since this is `tizenclaw` core, we can just write a fast zero-allocation hand-rolled parser).

### Algorithm:
1. Iterate over chars of `args_str`.
2. Skip whitespaces.
3. If hitting `"` or `'`, scan until the closing quote, push the string inside the quotes (without the quotes) to the vector.
4. If hitting any other char, scan until the next whitespace, push to the vector.

## 3. Markdown Context Injection
`parse_tool_md()` must compile the full text of `tool.md` into `ToolDecl.description` instead of stripping it.
To avoid context window limits from excessive tokens, we'll slice the `content` up to 1024 characters.
```rust
let full_desc = content.trim();
let truncated_desc = if full_desc.len() > 1024 {
    &full_desc[0..1024]
} else {
    full_desc
};
```

This ensures `Send + Sync` guarantees are maintained without introducing thread blocks or FFI boundary instability.
