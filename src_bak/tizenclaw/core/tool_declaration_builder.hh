/*
 * Copyright (c) 2026 Samsung Electronics Co., Ltd All Rights Reserved
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
#ifndef TOOL_DECLARATION_BUILDER_HH_
#define TOOL_DECLARATION_BUILDER_HH_

#include <map>
#include <string>
#include <vector>

#include "../llm/llm_backend.hh"
#include "action_bridge.hh"

namespace tizenclaw {

// Builds LLM tool declarations for all built-in
// tools, Action Framework tools, and CLI tools.
// Extracted from AgentCore::LoadSkillDeclarations()
// for maintainability and testability.
class ToolDeclarationBuilder {
 public:
  // Append all 30 built-in tool declarations
  static void AppendBuiltinTools(
      std::vector<LlmToolDecl>& tools);

  // Append Action Framework per-action tools
  // from cached MD schemas
  static void AppendActionTools(
      std::vector<LlmToolDecl>& tools,
      ActionBridge* action_bridge);

  // Append CLI tool declaration and scan
  // tool.md descriptors from disk
  static void AppendCliTools(
      std::vector<LlmToolDecl>& tools,
      std::map<std::string, std::string>& cli_dirs,
      std::map<std::string, std::string>& cli_docs);
};

}  // namespace tizenclaw

#endif  // TOOL_DECLARATION_BUILDER_HH_
