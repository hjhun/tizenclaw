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
#ifndef AGENT_ROLE_HH
#define AGENT_ROLE_HH

#include <json.hpp>
#include <map>
#include <mutex>
#include <string>
#include <vector>

namespace tizenclaw {

class AgentCore;  // forward declaration

// Agent role definition loaded from
// agent_roles.json
struct AgentRole {
  std::string name;
  std::string system_prompt;
  std::vector<std::string> allowed_tools;
  int max_iterations = 10;
};

// Result of a single delegation
struct DelegationResult {
  std::string role_name;
  std::string session_id;
  std::string sub_task;
  std::string result;
  bool success = false;
};

// Active delegation entry for tracking
struct ActiveDelegation {
  std::string role_name;
  std::string sub_task;
  std::string session_id;
  int64_t start_time_ms = 0;   // epoch ms
  bool completed = false;
  int64_t end_time_ms = 0;
  bool success = false;
};

// Supervisor engine for multi-agent
// orchestration
class SupervisorEngine {
 public:
  explicit SupervisorEngine(AgentCore* agent);

  // Load role definitions from JSON config
  [[nodiscard]] bool LoadRoles(const std::string& config_path);

  // Run supervisor loop:
  // decompose → delegate → collect → validate
  [[nodiscard]] std::string RunSupervisor(const std::string& goal,
                                          const std::string& strategy,
                                          const std::string& session_id);

  // List configured roles
  [[nodiscard]] nlohmann::json ListRoles() const;

  // Get role by name (nullptr if not found)
  [[nodiscard]] const AgentRole* GetRole(const std::string& name) const;

  // Get all role names
  [[nodiscard]] std::vector<std::string> GetRoleNames() const;

  // Dynamic role management
  void RegisterRole(const AgentRole& role);
  void UnregisterRole(const std::string& name);

  // Active delegation tracking
  [[nodiscard]] nlohmann::json GetAgentStatus() const;
  [[nodiscard]] nlohmann::json ListActiveDelegations() const;

 private:
  // Decompose goal into (role, sub_task) pairs
  // via LLM
  std::vector<std::pair<std::string, std::string>> DecomposeGoal(
      const std::string& goal, const std::string& session_id);

  // Delegate sub-task to a role agent session
  DelegationResult DelegateToRole(const AgentRole& role,
                                  const std::string& sub_task,
                                  const std::string& parent_session);

  // Validate and aggregate results via LLM
  std::string ValidateResults(const std::string& goal,
                              const std::vector<DelegationResult>& results,
                              const std::string& session_id);

  AgentCore* agent_;
  std::map<std::string, AgentRole> roles_;
  mutable std::mutex roles_mutex_;

  // Active delegation tracking
  std::vector<ActiveDelegation> active_delegations_;
  std::vector<ActiveDelegation> delegation_history_;
  mutable std::mutex delegation_mutex_;
  static constexpr size_t kMaxHistory = 20;
  int total_delegations_ = 0;
  int successful_delegations_ = 0;

  // Session prefix for role agents
  static constexpr const char* kRolePrefix = "role_";
};

}  // namespace tizenclaw

#endif  // AGENT_ROLE_HH
