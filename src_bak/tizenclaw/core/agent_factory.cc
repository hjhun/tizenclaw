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
#include "agent_factory.hh"

#include <fstream>
#include <regex>

#include "../../common/logging.hh"
#include "agent_core.hh"

namespace tizenclaw {

AgentFactory::AgentFactory(AgentCore* agent,
                           SupervisorEngine* supervisor)
    : agent_(agent), supervisor_(supervisor) {}

bool AgentFactory::ValidateName(
    const std::string& name) const {
  if (name.size() < kMinNameLength ||
      name.size() > kMaxNameLength) {
    return false;
  }

  // Only lowercase letters and underscores
  static const std::regex name_re("^[a-z_]{3,30}$");
  return std::regex_match(name, name_re);
}

std::string AgentFactory::SpawnAgent(
    const nlohmann::json& args) {
  nlohmann::json result;

  // Extract arguments
  std::string name =
      args.value("name", "");
  std::string system_prompt =
      args.value("system_prompt", "");
  int max_iterations =
      args.value("max_iterations", 10);
  bool persistent =
      args.value("persistent", false);

  // Validate name
  if (!ValidateName(name)) {
    result = {
        {"error",
         "Invalid agent name. Must be 3-30 "
         "lowercase letters/underscores "
         "(e.g. 'network_analyst')"}};
    return result.dump();
  }

  // Validate system_prompt
  if (system_prompt.empty()) {
    result = {
        {"error",
         "system_prompt is required"}};
    return result.dump();
  }
  if (system_prompt.size() > kMaxPromptLength) {
    result = {
        {"error",
         "system_prompt exceeds max length ("
         + std::to_string(kMaxPromptLength)
         + " chars)"}};
    return result.dump();
  }

  // Check dynamic agent limit
  {
    std::lock_guard<std::mutex> lock(roles_mutex_);
    if (dynamic_roles_.size() >= kMaxDynamicAgents) {
      result = {
          {"error",
           "Maximum dynamic agents reached ("
           + std::to_string(kMaxDynamicAgents)
           + "). Remove an existing one first."}};
      return result.dump();
    }

    // Check if name already exists (dynamic)
    if (dynamic_roles_.count(name) > 0) {
      result = {
          {"error",
           "Dynamic agent '" + name
           + "' already exists"}};
      return result.dump();
    }
  }

  // Check if name already exists (static)
  if (supervisor_ &&
      supervisor_->GetRole(name) != nullptr) {
    result = {
        {"error",
         "Agent role '" + name
         + "' already exists in config"}};
    return result.dump();
  }

  // Build AgentRole
  AgentRole role;
  role.name = name;
  role.system_prompt = system_prompt;
  role.max_iterations = max_iterations;

  if (args.contains("allowed_tools") &&
      args["allowed_tools"].is_array()) {
    for (const auto& t : args["allowed_tools"]) {
      if (t.is_string()) {
        role.allowed_tools.push_back(
            t.get<std::string>());
      }
    }
  }

  // Register with SupervisorEngine
  if (supervisor_) {
    supervisor_->RegisterRole(role);
  }

  // Track dynamically
  {
    std::lock_guard<std::mutex> lock(roles_mutex_);
    dynamic_roles_[name] = role;
  }

  // Persist if requested
  if (persistent) {
    PersistRole(role);
  }

  // Create a session for this agent
  std::string session_id =
      "agent_" + name + "_"
      + std::to_string(
            std::chrono::system_clock::now()
                .time_since_epoch()
                .count() % 100000);

  LOG(INFO) << "AgentFactory: spawned agent '"
            << name << "' (session="
            << session_id
            << ", persistent=" << persistent
            << ")";

  result = {
      {"status", "ok"},
      {"agent_name", name},
      {"session_id", session_id},
      {"persistent", persistent},
      {"max_iterations", max_iterations},
      {"message",
       "Agent '" + name + "' created. Use "
       "run_supervisor with goal to delegate "
       "tasks to this agent."}};
  return result.dump();
}

nlohmann::json AgentFactory::ListDynamicAgents() const {
  std::lock_guard<std::mutex> lock(roles_mutex_);

  auto agents = nlohmann::json::array();
  for (const auto& [name, role] : dynamic_roles_) {
    agents.push_back({
        {"name", name},
        {"system_prompt_preview",
         role.system_prompt.substr(
             0, std::min(role.system_prompt.size(),
                         size_t(80)))
         + "..."},
        {"max_iterations", role.max_iterations},
        {"allowed_tools", role.allowed_tools}});
  }
  return agents;
}

std::string AgentFactory::RemoveAgent(
    const std::string& name) {
  nlohmann::json result;

  {
    std::lock_guard<std::mutex> lock(roles_mutex_);
    auto it = dynamic_roles_.find(name);
    if (it == dynamic_roles_.end()) {
      result = {
          {"error",
           "Dynamic agent '" + name
           + "' not found"}};
      return result.dump();
    }
    dynamic_roles_.erase(it);
  }

  // Unregister from SupervisorEngine
  if (supervisor_) {
    supervisor_->UnregisterRole(name);
  }

  LOG(INFO) << "AgentFactory: removed agent '"
            << name << "'";

  result = {
      {"status", "ok"},
      {"removed", name}};
  return result.dump();
}

bool AgentFactory::PersistRole(
    const AgentRole& role) {
  std::string path =
      std::string(APP_DATA_DIR)
      + "/config/agent_roles.json";

  // Read existing config
  std::ifstream in(path);
  if (!in.is_open()) {
    LOG(WARNING) << "AgentFactory: cannot open "
                 << path << " for persistence";
    return false;
  }

  nlohmann::json config;
  try {
    in >> config;
  } catch (const std::exception& e) {
    LOG(WARNING) << "AgentFactory: JSON parse "
                 << "error: " << e.what();
    in.close();
    return false;
  }
  in.close();

  // Add new role
  nlohmann::json role_json = {
      {"name", role.name},
      {"type", "worker"},
      {"description",
       "Dynamically created agent"},
      {"system_prompt", role.system_prompt},
      {"tools", role.allowed_tools},
      {"auto_start", false}};

  if (!config.contains("agents")) {
    config["agents"] = nlohmann::json::array();
  }
  config["agents"].push_back(role_json);

  // Write back
  std::ofstream out(path);
  if (!out.is_open()) {
    LOG(WARNING) << "AgentFactory: cannot write "
                 << path;
    return false;
  }
  out << config.dump(2);
  out.close();

  LOG(INFO) << "AgentFactory: persisted agent '"
            << role.name << "' to " << path;
  return true;
}

}  // namespace tizenclaw
