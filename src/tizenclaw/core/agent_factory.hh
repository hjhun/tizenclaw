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
#ifndef AGENT_FACTORY_HH
#define AGENT_FACTORY_HH

#include <json.hpp>
#include <map>
#include <mutex>
#include <string>

#include "agent_role.hh"

namespace tizenclaw {

class AgentCore;

// AgentFactory allows the LLM to dynamically
// create new specialized agents at runtime
// via the spawn_agent tool.
class AgentFactory {
 public:
  AgentFactory(AgentCore* agent,
               SupervisorEngine* supervisor);

  // LLM entry point — called by spawn_agent tool
  [[nodiscard]] std::string SpawnAgent(
      const nlohmann::json& args);

  // List dynamically created agents
  [[nodiscard]] nlohmann::json ListDynamicAgents() const;

  // Remove a dynamic agent
  [[nodiscard]] std::string RemoveAgent(
      const std::string& name);

 private:
  // Validate agent name format
  [[nodiscard]] bool ValidateName(
      const std::string& name) const;

  // Persist role to agent_roles.json
  bool PersistRole(const AgentRole& role);

  AgentCore* agent_;
  SupervisorEngine* supervisor_;

  // Track dynamically created roles
  std::map<std::string, AgentRole> dynamic_roles_;
  mutable std::mutex roles_mutex_;

  // Safety limits
  static constexpr size_t kMaxDynamicAgents = 5;
  static constexpr size_t kMaxPromptLength = 4096;
  static constexpr size_t kMinNameLength = 3;
  static constexpr size_t kMaxNameLength = 30;
};

}  // namespace tizenclaw

#endif  // AGENT_FACTORY_HH
