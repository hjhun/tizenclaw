// Copyright 2026 TizenClaw Authors
// Supervisor Agent Pattern implementation
#include "agent_role.hh"

#include <chrono>
#include <fstream>
#include <future>
#include <sstream>

#include "agent_core.hh"
#include "../../common/logging.hh"
#include "../storage/audit_logger.hh"

namespace tizenclaw {

SupervisorEngine::SupervisorEngine(
    AgentCore* agent)
    : agent_(agent) {
}

bool SupervisorEngine::LoadRoles(
    const std::string& config_path) {
  std::ifstream f(config_path);
  if (!f.is_open()) {
    LOG(WARNING)
        << "agent_roles.json not found: "
        << config_path;
    return false;
  }

  try {
    nlohmann::json config;
    f >> config;
    f.close();

    std::lock_guard<std::mutex> lock(
        roles_mutex_);
    roles_.clear();

    // Accept both "roles" and "agents" keys
    nlohmann::json roles_array;
    if (config.contains("roles") &&
        config["roles"].is_array()) {
      roles_array = config["roles"];
    } else if (config.contains("agents") &&
               config["agents"].is_array()) {
      roles_array = config["agents"];
    } else {
      LOG(WARNING)
          << "No 'roles' or 'agents' array in config";
      return false;
    }

    for (auto& role_json : roles_array) {
      AgentRole role;
      role.name =
          role_json.value("name", "");
      role.system_prompt =
          role_json.value("system_prompt", "");
      role.max_iterations =
          role_json.value("max_iterations", 10);

      // Accept both "allowed_tools" and "tools"
      std::string tools_key = "allowed_tools";
      if (!role_json.contains(tools_key) &&
          role_json.contains("tools")) {
        tools_key = "tools";
      }
      if (role_json.contains(tools_key) &&
          role_json[tools_key].is_array()) {
        for (auto& t :
             role_json[tools_key]) {
          role.allowed_tools.push_back(
              t.get<std::string>());
        }
      }

      if (role.name.empty() ||
          role.system_prompt.empty()) {
        LOG(WARNING)
            << "Skipping role with empty "
            << "name or system_prompt";
        continue;
      }

      roles_[role.name] = std::move(role);
    }

    LOG(INFO) << "Loaded " << roles_.size()
              << " agent roles";
    return !roles_.empty();
  } catch (const std::exception& e) {
    LOG(ERROR)
        << "Failed to parse agent_roles.json: "
        << e.what();
    return false;
  }
}

nlohmann::json SupervisorEngine::ListRoles()
    const {
  std::lock_guard<std::mutex> lock(
      roles_mutex_);
  nlohmann::json result =
      nlohmann::json::array();

  for (auto& [name, role] : roles_) {
    nlohmann::json r = {
        {"name", name},
        {"system_prompt",
         role.system_prompt.substr(
             0,
             std::min((size_t)100,
                      role.system_prompt.size()))
             + (role.system_prompt.size() > 100
                    ? "..."
                    : "")},
        {"allowed_tools", role.allowed_tools},
        {"max_iterations", role.max_iterations}};
    result.push_back(r);
  }

  return result;
}

const AgentRole* SupervisorEngine::GetRole(
    const std::string& name) const {
  std::lock_guard<std::mutex> lock(
      roles_mutex_);
  auto it = roles_.find(name);
  if (it != roles_.end()) {
    return &it->second;
  }
  return nullptr;
}

std::vector<std::string>
SupervisorEngine::GetRoleNames() const {
  std::lock_guard<std::mutex> lock(
      roles_mutex_);
  std::vector<std::string> names;
  for (auto& [name, role] : roles_) {
    (void)role;
    names.push_back(name);
  }
  return names;
}

std::string SupervisorEngine::RunSupervisor(
    const std::string& goal,
    const std::string& strategy,
    const std::string& session_id) {
  LOG(INFO) << "Supervisor: goal=\""
            << goal << "\" strategy="
            << strategy;

  AuditLogger::Instance().Log(
      AuditLogger::MakeEvent(
          AuditEventType::kToolExecution,
          session_id,
          {{"operation", "run_supervisor"},
           {"goal", goal.substr(
                0,
                std::min((size_t)100,
                         goal.size()))},
           {"strategy", strategy}}));

  // Step 1: Decompose goal into sub-tasks
  auto sub_tasks =
      DecomposeGoal(goal, session_id);

  if (sub_tasks.empty()) {
    LOG(WARNING)
        << "Supervisor: goal decomposition "
        << "returned no sub-tasks";
    return "Failed to decompose goal into "
           "sub-tasks. Please provide a more "
           "specific goal.";
  }

  LOG(INFO)
      << "Supervisor: decomposed into "
      << sub_tasks.size() << " sub-tasks";

  // Step 2: Delegate sub-tasks to role agents
  std::vector<DelegationResult> results;

  if (strategy == "parallel") {
    // Parallel delegation
    std::vector<std::future<DelegationResult>>
        futures;
    for (auto& [role_name, sub_task] :
         sub_tasks) {
      const AgentRole* role = GetRole(role_name);
      if (!role) {
        LOG(WARNING)
            << "Supervisor: unknown role: "
            << role_name;
        DelegationResult dr;
        dr.role_name = role_name;
        dr.sub_task = sub_task;
        dr.result =
            "Error: role '" + role_name +
            "' not configured";
        dr.success = false;
        results.push_back(dr);
        continue;
      }

      futures.push_back(std::async(
          std::launch::async,
          [this, role, sub_task, session_id]() {
            return DelegateToRole(
                *role, sub_task, session_id);
          }));
    }

    for (auto& f : futures) {
      results.push_back(f.get());
    }
  } else {
    // Sequential delegation (default)
    for (auto& [role_name, sub_task] :
         sub_tasks) {
      const AgentRole* role = GetRole(role_name);
      if (!role) {
        LOG(WARNING)
            << "Supervisor: unknown role: "
            << role_name;
        DelegationResult dr;
        dr.role_name = role_name;
        dr.sub_task = sub_task;
        dr.result =
            "Error: role '" + role_name +
            "' not configured";
        dr.success = false;
        results.push_back(dr);
        continue;
      }

      auto dr = DelegateToRole(
          *role, sub_task, session_id);
      results.push_back(dr);
    }
  }

  // Step 3: Validate and aggregate results
  std::string final_result =
      ValidateResults(goal, results, session_id);

  LOG(INFO)
      << "Supervisor: completed with "
      << results.size() << " delegations";

  return final_result;
}

std::vector<std::pair<std::string, std::string>>
SupervisorEngine::DecomposeGoal(
    const std::string& goal,
    const std::string& session_id) {
  // Build decomposition prompt
  std::ostringstream prompt;
  prompt << "You are a task decomposition "
         << "engine. Given a goal and a list of "
         << "available specialist roles, "
         << "decompose the goal into sub-tasks "
         << "assigned to appropriate roles.\n\n";

  // List available roles
  prompt << "Available roles:\n";
  {
    std::lock_guard<std::mutex> lock(
        roles_mutex_);
    for (auto& [name, role] : roles_) {
      prompt << "- " << name << ": "
             << role.system_prompt.substr(
                    0,
                    std::min(
                        (size_t)80,
                        role.system_prompt.size()))
             << "\n";
    }
  }

  prompt << "\nGoal: " << goal << "\n\n"
         << "Respond ONLY with a JSON array of "
         << "objects, each with 'role' and "
         << "'task' fields. Example:\n"
         << "[{\"role\": \"researcher\", "
         << "\"task\": \"Find information "
         << "about X\"}, "
         << "{\"role\": \"writer\", "
         << "\"task\": \"Write a summary "
         << "based on the research\"}]\n\n"
         << "IMPORTANT: Use only roles from "
         << "the available list above. "
         << "Output ONLY the JSON array, "
         << "nothing else.";

  // Use a temporary session for decomposition
  std::string decomp_session =
      "supervisor_decomp_" + session_id;
  std::string response =
      agent_->ProcessPrompt(
          decomp_session, prompt.str());

  // Clean up temporary session
  agent_->ClearSession(decomp_session);

  // Parse LLM response
  std::vector<std::pair<std::string, std::string>>
      sub_tasks;

  try {
    // Try to find JSON array in response
    size_t start = response.find('[');
    size_t end = response.rfind(']');

    if (start == std::string::npos ||
        end == std::string::npos ||
        end <= start) {
      LOG(WARNING)
          << "Supervisor: no JSON array "
          << "in decomposition response";
      return sub_tasks;
    }

    std::string json_str =
        response.substr(start, end - start + 1);
    auto tasks =
        nlohmann::json::parse(json_str);

    if (!tasks.is_array()) {
      LOG(WARNING)
          << "Supervisor: decomposition "
          << "result is not an array";
      return sub_tasks;
    }

    for (auto& t : tasks) {
      std::string role =
          t.value("role", "");
      std::string task =
          t.value("task", "");
      if (!role.empty() && !task.empty()) {
        sub_tasks.emplace_back(role, task);
      }
    }
  } catch (const std::exception& e) {
    LOG(WARNING)
        << "Supervisor: failed to parse "
        << "decomposition: " << e.what();
  }

  return sub_tasks;
}

DelegationResult SupervisorEngine::DelegateToRole(
    const AgentRole& role,
    const std::string& sub_task,
    const std::string& parent_session) {
  DelegationResult dr;
  dr.role_name = role.name;
  dr.sub_task = sub_task;

  LOG(INFO) << "Supervisor: delegating to role '"
            << role.name << "': " << sub_task;

  // Create role agent session
  nlohmann::json create_args = {
      {"name", std::string(kRolePrefix) +
                   role.name},
      {"system_prompt", role.system_prompt}};

  std::string create_result =
      agent_->ExecuteSessionOp(
          "create_session", create_args,
          parent_session);

  try {
    auto cr =
        nlohmann::json::parse(create_result);
    if (cr.contains("error")) {
      dr.result =
          "Failed to create role session: " +
          cr["error"].get<std::string>();
      dr.success = false;
      return dr;
    }

    dr.session_id =
        cr["session_id"].get<std::string>();
  } catch (...) {
    dr.result =
        "Failed to parse session creation result";
    dr.success = false;
    return dr;
  }

  // Build delegation prompt with tool
  // restrictions info
  std::string delegation_prompt = sub_task;
  if (!role.allowed_tools.empty()) {
    delegation_prompt +=
        "\n\n[Note: You may only use these "
        "tools: ";
    for (size_t i = 0;
         i < role.allowed_tools.size(); i++) {
      if (i > 0)
        delegation_prompt += ", ";
      delegation_prompt +=
          role.allowed_tools[i];
    }
    delegation_prompt += "]";
  }

  // Execute via ProcessPrompt on role session
  dr.result = agent_->ProcessPrompt(
      dr.session_id, delegation_prompt);
  dr.success =
      !dr.result.empty() &&
      dr.result.find("Error:") !=
          0;

  LOG(INFO) << "Supervisor: role '"
            << role.name
            << "' completed (success="
            << dr.success << ")";

  return dr;
}

std::string SupervisorEngine::ValidateResults(
    const std::string& goal,
    const std::vector<DelegationResult>& results,
    const std::string& session_id) {
  if (results.empty()) {
    return "No results to validate.";
  }

  // If only one result, return it directly
  if (results.size() == 1) {
    return results[0].result;
  }

  // Build validation prompt
  std::ostringstream prompt;
  prompt << "You are a result aggregator. "
         << "Given a goal and results from "
         << "multiple specialist agents, "
         << "synthesize them into a single "
         << "coherent response.\n\n"
         << "Original goal: " << goal
         << "\n\n"
         << "Results from specialist agents:\n";

  for (auto& dr : results) {
    prompt << "\n--- " << dr.role_name
           << " ---\n";
    if (dr.success) {
      prompt << dr.result << "\n";
    } else {
      prompt << "[FAILED] " << dr.result
             << "\n";
    }
  }

  prompt << "\nSynthesize these results into "
         << "a single comprehensive response "
         << "that addresses the original goal. "
         << "Note any failures or gaps.";

  // Use temporary session for validation
  std::string val_session =
      "supervisor_validate_" + session_id;
  std::string response =
      agent_->ProcessPrompt(
          val_session, prompt.str());

  agent_->ClearSession(val_session);

  return response;
}

}  // namespace tizenclaw
