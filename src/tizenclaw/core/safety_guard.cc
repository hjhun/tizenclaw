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
#include "safety_guard.hh"

#include <chrono>
#include <fstream>

#include "../../common/logging.hh"

namespace tizenclaw {

bool SafetyGuard::LoadConfig(
    const std::string& config_path) {
  std::ifstream f(config_path);
  if (!f.is_open()) {
    LOG(INFO) << "SafetyGuard: no config at "
              << config_path
              << " (safety bounds disabled)";
    return false;
  }

  try {
    nlohmann::json j;
    f >> j;

    // Parse safety bounds
    if (j.contains("bounds") && j["bounds"].is_object()) {
      for (auto& [tool_name, params] : j["bounds"].items()) {
        if (!params.is_array()) continue;
        std::vector<SafetyBound> tool_bounds;
        for (auto& p : params) {
          SafetyBound b;
          b.param_name = p.value("param", "");
          b.min_value = p.value("min", 0.0);
          b.max_value = p.value("max", 0.0);
          b.unit = p.value("unit", "");
          b.description = p.value("description", "");
          if (!b.param_name.empty()) {
            tool_bounds.push_back(std::move(b));
          }
        }
        if (!tool_bounds.empty()) {
          bounds_[tool_name] = std::move(tool_bounds);
        }
      }
    }

    // Parse action rate limits
    if (j.contains("rate_limits") &&
        j["rate_limits"].is_object()) {
      for (auto& [tool_name, rule] : j["rate_limits"].items()) {
        ActionRateRule r;
        r.max_calls = rule.value("max_calls", 5);
        r.window_seconds = rule.value("window_seconds", 60);
        rate_rules_[tool_name] = r;
      }
    }

    // Parse confirmation-required tools
    if (j.contains("confirmation_required") &&
        j["confirmation_required"].is_array()) {
      for (auto& t : j["confirmation_required"]) {
        if (t.is_string()) {
          confirmation_tools_.push_back(
              t.get<std::string>());
        }
      }
    }

    config_loaded_ = true;
    LOG(INFO) << "SafetyGuard: loaded "
              << bounds_.size() << " tool bounds, "
              << rate_rules_.size() << " rate rules, "
              << confirmation_tools_.size()
              << " confirmation tools";
    return true;
  } catch (const std::exception& e) {
    LOG(ERROR) << "SafetyGuard: config parse error: "
               << e.what();
    return false;
  }
}

bool SafetyGuard::LoadDeviceProfile(
    const std::string& config_path) {
  std::ifstream f(config_path);
  if (!f.is_open()) {
    LOG(INFO) << "SafetyGuard: no device profile at "
              << config_path;
    return false;
  }

  try {
    nlohmann::json j;
    f >> j;

    device_profile_.device_type =
        j.value("device_type", "generic");

    if (j.contains("capabilities") &&
        j["capabilities"].is_array()) {
      for (auto& c : j["capabilities"]) {
        if (c.is_string()) {
          device_profile_.capabilities.push_back(
              c.get<std::string>());
        }
      }
    }

    if (j.contains("excluded_tools") &&
        j["excluded_tools"].is_array()) {
      for (auto& t : j["excluded_tools"]) {
        if (t.is_string()) {
          device_profile_.excluded_tools.push_back(
              t.get<std::string>());
        }
      }
    }

    // Merge device-specific safety bounds
    if (j.contains("safety_bounds") &&
        j["safety_bounds"].is_object()) {
      for (auto& [tool_name, params] :
           j["safety_bounds"].items()) {
        if (!params.is_array()) continue;
        for (auto& p : params) {
          SafetyBound b;
          b.param_name = p.value("param", "");
          b.min_value = p.value("min", 0.0);
          b.max_value = p.value("max", 0.0);
          b.unit = p.value("unit", "");
          b.description = p.value(
              "description", "device-specific bound");
          if (!b.param_name.empty()) {
            bounds_[tool_name].push_back(
                std::move(b));
          }
        }
      }
    }

    LOG(INFO) << "SafetyGuard: device profile '"
              << device_profile_.device_type
              << "' loaded, "
              << device_profile_.excluded_tools.size()
              << " excluded tools";
    return true;
  } catch (const std::exception& e) {
    LOG(ERROR) << "SafetyGuard: device profile "
               << "parse error: " << e.what();
    return false;
  }
}

SafetyCheckResult SafetyGuard::Validate(
    const std::string& tool_name,
    const nlohmann::json& args) const {
  SafetyCheckResult result;

  // Check device exclusion list
  if (IsExcludedTool(tool_name)) {
    result.allowed = false;
    result.reason =
        "Tool '" + tool_name +
        "' is not available on this " +
        device_profile_.device_type + " device";
    return result;
  }

  // Check safety bounds for numeric parameters
  auto bounds_it = bounds_.find(tool_name);
  if (bounds_it != bounds_.end()) {
    for (const auto& bound : bounds_it->second) {
      if (!args.contains(bound.param_name))
        continue;

      const auto& val = args[bound.param_name];
      double numeric_val = 0.0;

      if (val.is_number()) {
        numeric_val = val.get<double>();
      } else if (val.is_string()) {
        try {
          numeric_val = std::stod(
              val.get<std::string>());
        } catch (...) {
          continue;  // Not a numeric param
        }
      } else {
        continue;
      }

      auto param_result =
          ValidateParam(bound, numeric_val);
      if (!param_result.allowed) {
        return param_result;
      }
    }
  }

  // Check confirmation requirement
  for (const auto& ct : confirmation_tools_) {
    if (ct == tool_name) {
      result.requires_confirmation = true;
      result.reason =
          "This action requires user confirmation "
          "before execution";
      break;
    }
  }

  return result;
}

bool SafetyGuard::IsExcludedTool(
    const std::string& tool_name) const {
  for (const auto& t :
       device_profile_.excluded_tools) {
    if (t == tool_name) return true;
  }
  return false;
}

bool SafetyGuard::CheckActionRateLimit(
    const std::string& tool_name) {
  auto rule_it = rate_rules_.find(tool_name);
  if (rule_it == rate_rules_.end()) return false;

  const auto& rule = rule_it->second;
  auto now = std::chrono::duration_cast<
                 std::chrono::seconds>(
                 std::chrono::system_clock::now()
                     .time_since_epoch())
                 .count();

  std::lock_guard<std::mutex> lock(rate_mutex_);
  auto& history = rate_history_[tool_name];

  // Remove expired entries
  int64_t cutoff = now - rule.window_seconds;
  while (!history.empty() &&
         history.front() < cutoff) {
    history.erase(history.begin());
  }

  // Check limit
  if (static_cast<int>(history.size()) >=
      rule.max_calls) {
    return true;  // Rate limited
  }

  // Record this call
  history.push_back(now);
  return false;
}

std::pair<double, bool> SafetyGuard::ClampToSafe(
    const std::string& tool_name,
    const std::string& param_name,
    double value) const {
  const auto* bound = FindBound(tool_name, param_name);
  if (!bound) return {value, false};

  if (value < bound->min_value) {
    return {bound->min_value, true};
  }
  if (value > bound->max_value) {
    return {bound->max_value, true};
  }
  return {value, false};
}

nlohmann::json SafetyGuard::GetStatusJson() const {
  nlohmann::json status;
  status["config_loaded"] = config_loaded_;
  status["device_type"] =
      device_profile_.device_type;
  status["bounds_count"] =
      static_cast<int>(bounds_.size());
  status["rate_rules_count"] =
      static_cast<int>(rate_rules_.size());
  status["confirmation_tools_count"] =
      static_cast<int>(confirmation_tools_.size());
  status["excluded_tools_count"] =
      static_cast<int>(
          device_profile_.excluded_tools.size());

  // List bounded tools
  nlohmann::json bounded = nlohmann::json::array();
  for (const auto& [name, _] : bounds_) {
    bounded.push_back(name);
  }
  status["bounded_tools"] = bounded;

  return status;
}

const SafetyBound* SafetyGuard::FindBound(
    const std::string& tool_name,
    const std::string& param_name) const {
  auto it = bounds_.find(tool_name);
  if (it == bounds_.end()) return nullptr;

  for (const auto& b : it->second) {
    if (b.param_name == param_name) return &b;
  }
  return nullptr;
}

SafetyCheckResult SafetyGuard::ValidateParam(
    const SafetyBound& bound,
    double value) const {
  SafetyCheckResult result;

  if (value < bound.min_value) {
    result.allowed = false;
    result.reason =
        "Safety violation: " + bound.param_name +
        " value " + std::to_string(value) +
        " " + bound.unit +
        " is below minimum safe limit of " +
        std::to_string(bound.min_value) +
        " " + bound.unit;
    if (!bound.description.empty()) {
      result.reason += " (" + bound.description + ")";
    }
    result.safe_value =
        std::to_string(bound.min_value);
    return result;
  }

  if (value > bound.max_value) {
    result.allowed = false;
    result.reason =
        "Safety violation: " + bound.param_name +
        " value " + std::to_string(value) +
        " " + bound.unit +
        " exceeds maximum safe limit of " +
        std::to_string(bound.max_value) +
        " " + bound.unit;
    if (!bound.description.empty()) {
      result.reason += " (" + bound.description + ")";
    }
    result.safe_value =
        std::to_string(bound.max_value);
    return result;
  }

  return result;
}

}  // namespace tizenclaw
