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
#include "auto_skill_agent.hh"

#include <string>

#include "../../common/logging.hh"
#include "agent_core.hh"

namespace tizenclaw {

AutoSkillAgent::AutoSkillAgent(AgentCore* core) : core_(core) {}

bool AutoSkillAgent::DetectCapabilityGap(
    const std::string& response) const {
  // Pattern matching for common "can't do this" phrases
  // in both English and Korean from LLM responses.
  static const std::vector<std::string> gap_patterns = {
      "I don't have a tool",
      "I don't have access to a tool",
      "no existing tool",
      "no tool available",
      "don't have the ability",
      "don't have a way to",
      "cannot directly",
      "I'm unable to",
      "I can't directly",
      "not currently able",
      "no suitable tool",
      "적합한 도구가 없",
      "해당 기능이 없",
      "도구가 없어",
      "직접적으로 할 수 없",
      "해당 도구를 가지고 있지 않",
      "지원하지 않",
  };

  for (const auto& pattern : gap_patterns) {
    if (response.find(pattern) != std::string::npos) {
      LOG(INFO) << "Capability gap detected: \""
                << pattern << "\"";
      return true;
    }
  }
  return false;
}

AutoSkillAgent::GenerationResult AutoSkillAgent::TryGenerate(
    const std::string& session_id,
    const std::string& user_prompt,
    std::function<void(const std::string&)> on_chunk) {
  GenerationResult result;
  LOG(INFO) << "AutoSkillAgent: Attempting auto-generation"
            << " for: "
            << user_prompt.substr(
                   0, std::min((size_t)100,
                               user_prompt.size()));

  // Construct a meta-prompt that instructs the LLM to:
  // 1. Generate prototype code
  // 2. Test it via execute_code
  // 3. Register it via manage_custom_skill
  std::string meta_prompt =
      "The user asked: \"" + user_prompt +
      "\"\n\n"
      "No existing tool can handle this request. "
      "You MUST create a new custom skill to fulfill it. "
      "Follow this exact sequence:\n\n"
      "1. First, use `execute_code` to prototype and test "
      "Python code that accomplishes the task. The code must "
      "print valid JSON to stdout.\n"
      "2. If the prototype succeeds, use "
      "`manage_custom_skill` with operation=\"create\" to "
      "register it as a permanent skill. Use a descriptive "
      "snake_case name, proper description, and "
      "parameters_schema.\n"
      "3. After registration, execute the newly created "
      "skill to get the actual result.\n"
      "4. Finally, respond with the answer to the user's "
      "original question AND mention that a new skill was "
      "created.\n\n"
      "Available Tizen libraries for ctypes: "
      "libcapi-system-info.so.0, "
      "libcapi-system-device.so.0, "
      "libcapi-appfw-app-manager.so.0, "
      "libcapi-appfw-app-control.so.0, "
      "libcapi-network-connection.so.1, "
      "libglib-2.0.so, libicuuc.so, "
      "libsqlite3.so, libcurl.so.\n\n"
      "IMPORTANT: Do NOT say you cannot do it. "
      "Create the skill and answer the question.";

  // Use a dedicated session for auto-generation
  // to avoid polluting the user's conversation
  std::string auto_session =
      session_id + "_auto_skill";

  std::string response = core_->ProcessPrompt(
      auto_session, meta_prompt, on_chunk);

  // Clean up the auto-generation session
  core_->ClearSession(auto_session);

  if (response.empty() ||
      response.find("Error:") == 0) {
    result.success = false;
    result.error = "Auto-generation failed: " + response;
    LOG(WARNING) << result.error;
    return result;
  }

  // Check if a skill was actually created by looking
  // for success indicators in the response
  bool skill_created =
      response.find("created") != std::string::npos ||
      response.find("registered") != std::string::npos ||
      response.find("생성") != std::string::npos ||
      response.find("등록") != std::string::npos;

  result.success = skill_created;
  result.output = response;

  if (skill_created) {
    LOG(INFO) << "AutoSkillAgent: Skill auto-generated"
              << " successfully";
  } else {
    result.error =
        "Auto-generation completed but no skill "
        "was registered";
    LOG(WARNING) << result.error;
  }

  return result;
}

}  // namespace tizenclaw
