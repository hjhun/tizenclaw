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
#ifndef AUTO_SKILL_AGENT_HH
#define AUTO_SKILL_AGENT_HH

#include <atomic>
#include <functional>
#include <json.hpp>
#include <string>

namespace tizenclaw {

class AgentCore;

// Automatically generates custom skills when no existing tool
// can handle a user request. Uses a structured pipeline:
//   1. Prototype via execute_code (validate approach)
//   2. Register via manage_custom_skill (permanent skill)
//   3. Execute the new skill to answer the original request
//
// Integrates into AgentCore::ProcessPrompt as a post-response
// hook that detects "capability gap" patterns in the LLM's
// final text response.
class AutoSkillAgent {
 public:
  struct GenerationResult {
    bool success = false;
    std::string skill_name;
    std::string output;
    std::string error;
  };

  explicit AutoSkillAgent(AgentCore* core);

  // Check if the LLM response indicates a capability gap
  // (no tool available to handle the request).
  bool DetectCapabilityGap(const std::string& response) const;

  // Attempt to generate a custom skill for the given request.
  // Uses the LLM to generate code, tests via execute_code,
  // and registers via manage_custom_skill.
  GenerationResult TryGenerate(
      const std::string& session_id,
      const std::string& user_prompt,
      std::function<void(const std::string&)> on_chunk = nullptr);

 private:
  AgentCore* core_;
  static constexpr int kMaxRetries = 3;
};

}  // namespace tizenclaw

#endif  // AUTO_SKILL_AGENT_HH
