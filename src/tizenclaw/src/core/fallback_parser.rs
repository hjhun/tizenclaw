//! Fallback Parser — Extracts tool calls from plain text.
//!
//! Handles cases where the LLM fails to use the structured tool calling API
//! but produces valid tool call patterns in its response text.

use regex::Regex;
use serde_json::{json, Value};
use crate::llm::backend::LlmToolCall;

pub struct FallbackParser;

impl FallbackParser {
    /// Parse tool calls from the given text.
    /// Supports patterns like:
    /// 1. <tool_call>name({"arg": "val"})</tool_call>
    /// 2. ```json {"tool": "name", "arguments": {...}} ```
    pub fn parse(text: &str) -> Vec<LlmToolCall> {
        let mut tool_calls = Vec::new();

        // 1. XML-style tag parser: <tool_call>name(json_args)</tool_call>
        let xml_re = Regex::new(r"(?s)<tool_call>\s*(\w+)\s*\((.*?)\)\s*</tool_call>").unwrap();
        for cap in xml_re.captures_iter(text) {
            let name = cap[1].to_string();
            let args_raw = &cap[2];
            let args: Value = serde_json::from_str(args_raw).unwrap_or(json!({}));
            tool_calls.push(LlmToolCall {
                id: format!("call_fb_{}", &uuid::Uuid::new_v4().to_string()[..8]),
                name,
                args,
            });
        }

        // 1.5. Pure XML Model-Agnostic parser: <CallTool name="..." args="{...}" />
        let calltool_re = Regex::new(r#"(?s)<CallTool\s+name="([^"]+)"\s+args='([^']*)'\s*/>|<CallTool\s+name="([^"]+)"\s+args="([^"]*)"\s*/>"#).unwrap();
        for cap in calltool_re.captures_iter(text) {
            let (name, args_raw) = if let Some(n) = cap.get(1) {
                (n.as_str().to_string(), cap.get(2).map_or("", |m| m.as_str()))
            } else {
                (cap.get(3).unwrap().as_str().to_string(), cap.get(4).map_or("", |m| m.as_str()))
            };
            
            // Clean up escaped quotes if any
            let clean_args = args_raw.replace("\\\"", "\"");
            let args: Value = serde_json::from_str(&clean_args).unwrap_or(json!({}));
            tool_calls.push(LlmToolCall {
                id: format!("call_xml_{}", &uuid::Uuid::new_v4().to_string()[..8]),
                name,
                args,
            });
        }


        // 2. JSON block parser: ```json {"tool": "name", "arguments": {...}} ```
        if tool_calls.is_empty() {
             let json_re = Regex::new(r"(?s)```json\s*(\{.*?\})\s*```").unwrap();
             for cap in json_re.captures_iter(text) {
                 if let Ok(v) = serde_json::from_str::<Value>(&cap[1]) {
                     if let (Some(name), Some(args)) = (v["tool"].as_str(), v.get("arguments")) {
                         tool_calls.push(LlmToolCall {
                             id: format!("call_fb_j_{}", &uuid::Uuid::new_v4().to_string()[..8]),
                             name: name.to_string(),
                             args: args.clone(),
                         });
                     } else if let (Some(name), Some(args)) = (v["name"].as_str(), v.get("args")) {
                         // Alternative naming
                         tool_calls.push(LlmToolCall {
                             id: format!("call_fb_j_{}", &uuid::Uuid::new_v4().to_string()[..8]),
                             name: name.to_string(),
                             args: args.clone(),
                         });
                     }
                 }
             }
        }

        tool_calls
    }

    /// Extract <NewSummary>...</NewSummary> from the text for Fact-based Compaction
    pub fn extract_summary(text: &str) -> Option<String> {
        let re = Regex::new(r"(?s)<NewSummary>(.*?)</NewSummary>").unwrap();
        re.captures(text).map(|cap| cap[1].trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_style_parsing() {
        let text = "I will call the tool now: <tool_call>ls({\"path\": \"/tmp\"})</tool_call>";
        let calls = FallbackParser::parse(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "ls");
        assert_eq!(calls[0].args["path"], "/tmp");
    }

    #[test]
    fn test_json_block_parsing() {
        let text = "Use this: \n```json\n{\"tool\": \"read_file\", \"arguments\": {\"path\": \"test.txt\"}}\n```";
        let calls = FallbackParser::parse(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].args["path"], "test.txt");
    }
}
