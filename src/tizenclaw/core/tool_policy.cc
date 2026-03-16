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
#include "tool_policy.hh"

#include <fstream>
#include <functional>
#include <sstream>

#include "../../common/logging.hh"

namespace tizenclaw {

ToolPolicy::ToolPolicy() = default;

bool ToolPolicy::LoadConfig(const std::string& config_path) {
  std::ifstream f(config_path);
  if (!f.is_open()) {
    LOG(INFO) << "No tool policy config at " << config_path
              << ", using defaults";
    return true;  // defaults are fine
  }

  try {
    nlohmann::json j;
    f >> j;

    if (j.contains("max_repeat_count")) {
      config_.max_repeat_count = j["max_repeat_count"].get<int>();
    }

    if (j.contains("blocked_skills")) {
      for (auto& s : j["blocked_skills"]) {
        config_.blocked_skills.insert(s.get<std::string>());
      }
    }

    if (j.contains("risk_overrides")) {
      for (auto& [k, v] : j["risk_overrides"].items()) {
        config_.risk_levels[k] = ParseRiskLevel(v.get<std::string>());
      }
    }

    if (j.contains("max_iterations")) {
      config_.max_iterations = j["max_iterations"].get<int>();
    }

    if (j.contains("aliases") && j["aliases"].is_object()) {
      for (auto& [k, v] : j["aliases"].items()) {
        if (v.is_string()) {
          config_.aliases[k] = v.get<std::string>();
        }
      }
    }

    LOG(INFO) << "Tool policy loaded: " << "max_repeat="
              << config_.max_repeat_count
              << ", blocked=" << config_.blocked_skills.size()
              << ", overrides=" << config_.risk_levels.size()
              << ", aliases=" << config_.aliases.size();
    return true;
  } catch (const std::exception& e) {
    LOG(ERROR) << "Failed to parse tool policy: " << e.what();
    return false;
  }
}

void ToolPolicy::LoadManifestRiskLevel(const std::string& skill_name,
                                       const nlohmann::json& manifest) {
  if (manifest.contains("risk_level")) {
    std::string level_str = manifest["risk_level"].get<std::string>();
    // Config overrides take precedence
    if (config_.risk_levels.find(skill_name) == config_.risk_levels.end()) {
      config_.risk_levels[skill_name] = ParseRiskLevel(level_str);
    }
  }
}

std::string ToolPolicy::CheckPolicy(const std::string& session_id,
                                    const std::string& skill_name,
                                    const nlohmann::json& args) {
  std::lock_guard<std::mutex> lock(mutex_);

  // 1. Check blocked list
  if (config_.blocked_skills.count(skill_name) > 0) {
    return "Tool '" + skill_name + "' is blocked by security policy.";
  }

  // 2. Loop detection
  std::string hash = HashCall(skill_name, args);
  int& count = call_history_[session_id][hash];
  count++;

  if (count > config_.max_repeat_count) {
    return "Tool '" + skill_name +
           "' with identical arguments has been "
           "called " +
           std::to_string(count) +
           " times (limit: " + std::to_string(config_.max_repeat_count) +
           "). Execution blocked to prevent "
           "infinite loop. Please try a "
           "different approach.";
  }

  return "";  // allowed
}

void ToolPolicy::ResetSession(const std::string& session_id) {
  std::lock_guard<std::mutex> lock(mutex_);
  call_history_.erase(session_id);
  idle_history_.erase(session_id);
}

bool ToolPolicy::CheckIdleProgress(const std::string& session_id,
                                   const std::string& iteration_output) {
  std::lock_guard<std::mutex> lock(mutex_);

  auto& history = idle_history_[session_id];
  history.push_back(iteration_output);

  // Keep only the last kIdleWindowSize entries
  while (static_cast<int>(history.size()) > kIdleWindowSize) {
    history.erase(history.begin());
  }

  // Need at least kIdleWindowSize entries
  if (static_cast<int>(history.size()) < kIdleWindowSize) {
    return false;
  }

  // Check if all entries in window are the
  // same (idle = no progress)
  const auto& first = history.front();
  for (const auto& entry : history) {
    if (entry != first) {
      return false;
    }
  }

  return true;
}

int ToolPolicy::GetMaxIterations() const { return config_.max_iterations; }

void ToolPolicy::ResetIdleTracking(const std::string& session_id) {
  std::lock_guard<std::mutex> lock(mutex_);
  idle_history_.erase(session_id);
}

RiskLevel ToolPolicy::GetRiskLevel(const std::string& skill_name) const {
  auto it = config_.risk_levels.find(skill_name);
  if (it != config_.risk_levels.end()) {
    return it->second;
  }
  return RiskLevel::kNormal;
}

const std::map<std::string, std::string>&
ToolPolicy::GetAliases() const {
  return config_.aliases;
}

std::string ToolPolicy::RiskLevelToString(RiskLevel level) {
  switch (level) {
    case RiskLevel::kLow:
      return "low";
    case RiskLevel::kHigh:
      return "high";
    default:
      return "normal";
  }
}

std::string ToolPolicy::HashCall(const std::string& name,
                                 const nlohmann::json& args) const {
  // Simple hash: skill_name + sorted JSON args
  std::string input = name + ":";
  if (!args.is_null()) {
    input +=
        args.dump(-1, ' ', false, nlohmann::json::error_handler_t::replace);
  }

  // Use std::hash for simplicity
  // (no crypto dependency needed)
  std::size_t h = std::hash<std::string>{}(input);
  std::ostringstream oss;
  oss << std::hex << h;
  return oss.str();
}

RiskLevel ToolPolicy::ParseRiskLevel(const std::string& str) {
  if (str == "low") return RiskLevel::kLow;
  if (str == "high") return RiskLevel::kHigh;
  return RiskLevel::kNormal;
}

}  // namespace tizenclaw
