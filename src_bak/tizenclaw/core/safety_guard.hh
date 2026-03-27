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
#ifndef SAFETY_GUARD_HH_
#define SAFETY_GUARD_HH_

#include <json.hpp>
#include <map>
#include <mutex>
#include <string>
#include <vector>

#include "user_profile_store.hh"

namespace tizenclaw {

// Physical safety boundary for a numeric parameter.
// Enforces absolute min/max values that LLM cannot override.
struct SafetyBound {
  std::string param_name;   // JSON arg key (e.g. "temperature")
  double min_value = 0.0;
  double max_value = 0.0;
  std::string unit;         // "celsius", "rpm", "percent"
  std::string description;  // Human-readable explanation
};

// Physical action rate limit rule.
// Prevents rapid repeated physical actions.
struct ActionRateRule {
  int max_calls = 0;      // Max calls within window
  int window_seconds = 0;  // Time window
};

// Device profile defining device type and capabilities.
struct DeviceProfile {
  std::string device_type;  // "tv", "refrigerator", "oven", "washer"
  std::vector<std::string> capabilities;
  std::vector<std::string> excluded_tools;  // Tools not available
};

// Safety validation result
struct SafetyCheckResult {
  bool allowed = true;
  bool requires_confirmation = false;
  std::string reason;        // Block/warning reason
  std::string safe_value;    // Suggested clamped value
};

// SafetyGuard: Physical safety boundary layer.
// Sits between ToolPolicy and actual tool execution,
// enforcing hardware-level constraints that the LLM
// must never be allowed to override.
//
// Flow: LLM → ToolPolicy → SafetyGuard → Execute
class SafetyGuard {
 public:
  SafetyGuard() = default;
  ~SafetyGuard() = default;

  // Load safety bounds from JSON config file.
  // Returns true if loaded successfully.
  [[nodiscard]] bool LoadConfig(
      const std::string& config_path);

  // Load device profile (device type, excluded tools).
  [[nodiscard]] bool LoadDeviceProfile(
      const std::string& config_path);

  // Validate a tool call against safety bounds.
  // Returns SafetyCheckResult with allowed/blocked status.
  [[nodiscard]] SafetyCheckResult Validate(
      const std::string& tool_name,
      const nlohmann::json& args,
      UserRole role = UserRole::kGuest) const;

  // Check if a tool is excluded for this device type.
  [[nodiscard]] bool IsExcludedTool(
      const std::string& tool_name) const;

  // Check if action rate limit is exceeded.
  // Updates internal rate tracking.
  [[nodiscard]] bool CheckActionRateLimit(
      const std::string& tool_name);

  // Clamp a numeric argument to safe bounds.
  // Returns clamped value and whether clamping occurred.
  [[nodiscard]] std::pair<double, bool> ClampToSafe(
      const std::string& tool_name,
      const std::string& param_name,
      double value) const;

  // Get device profile
  [[nodiscard]] const DeviceProfile& GetDeviceProfile() const {
    return device_profile_;
  }

  // Get safety status as JSON (for dashboard)
  [[nodiscard]] nlohmann::json GetStatusJson() const;

 private:
  // Find bounds for a tool+param combination
  [[nodiscard]] const SafetyBound* FindBound(
      const std::string& tool_name,
      const std::string& param_name) const;

  // Validate a single parameter against its bound
  [[nodiscard]] SafetyCheckResult ValidateParam(
      const SafetyBound& bound,
      double value) const;

  // tool_name → list of safety bounds
  std::map<std::string, std::vector<SafetyBound>> bounds_;

  // tool_name → rate limit rule
  std::map<std::string, ActionRateRule> rate_rules_;

  // Rate limit tracking: tool_name → call timestamps
  std::map<std::string, std::vector<int64_t>> rate_history_;
  mutable std::mutex rate_mutex_;

  // Tools requiring user confirmation before execution
  std::vector<std::string> confirmation_tools_;

  // RBAC restricted tools
  std::vector<std::string> child_restricted_tools_;
  std::vector<std::string> guest_restricted_tools_;

  // Device profile
  DeviceProfile device_profile_;

  // Whether safety config is loaded
  bool config_loaded_ = false;
};

}  // namespace tizenclaw

#endif  // SAFETY_GUARD_HH_
