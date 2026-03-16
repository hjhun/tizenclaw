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
#ifndef TOOL_POLICY_HH
#define TOOL_POLICY_HH

#include <json.hpp>
#include <map>
#include <mutex>
#include <set>
#include <string>
#include <vector>

namespace tizenclaw {

enum class RiskLevel {
  kLow,     // Read-only (get_battery_info, etc.)
  kNormal,  // Default
  kHigh,    // Side-effect (send_app_control, etc.)
};

struct ToolPolicyConfig {
  // Per-skill risk level overrides
  std::map<std::string, RiskLevel> risk_levels;
  // Max repeated calls (same skill + same args)
  int max_repeat_count = 3;
  // Skills blocked entirely
  std::set<std::string> blocked_skills;
  // Max agentic loop iterations
  int max_iterations = 5;
  // Tool alias redirections: old_name -> new_name
  std::map<std::string, std::string> aliases;
};

class ToolPolicy {
 public:
  ToolPolicy();

  // Load policy config from JSON file
  // Returns true if loaded (or defaults used)
  [[nodiscard]] bool LoadConfig(const std::string& config_path);

  // Load risk_level from skill manifest
  void LoadManifestRiskLevel(const std::string& skill_name,
                             const nlohmann::json& manifest);

  // Check if a tool call is allowed.
  // Returns empty string if allowed,
  // violation reason string if blocked.
  [[nodiscard]] std::string CheckPolicy(const std::string& session_id,
                                        const std::string& skill_name,
                                        const nlohmann::json& args);

  // Track iteration outputs for idle detection.
  // Returns true if idle (no progress).
  [[nodiscard]] bool CheckIdleProgress(const std::string& session_id,
                                       const std::string& iteration_output);

  // Get max iterations for agentic loop
  [[nodiscard]] int GetMaxIterations() const;

  // Reset per-session call tracking
  void ResetSession(const std::string& session_id);

  // Reset idle tracking for a session
  void ResetIdleTracking(const std::string& session_id);

  // Get risk level for a skill
  [[nodiscard]] RiskLevel GetRiskLevel(const std::string& skill_name) const;

  // Get alias mappings for ToolRouter
  [[nodiscard]] const std::map<std::string, std::string>&
  GetAliases() const;

  // Convert RiskLevel to string
  static std::string RiskLevelToString(RiskLevel level);

 private:
  // Generate hash key for loop detection
  std::string HashCall(const std::string& name,
                       const nlohmann::json& args) const;

  // Parse risk level string to enum
  static RiskLevel ParseRiskLevel(const std::string& str);

  ToolPolicyConfig config_;

  // Track repeated calls per session:
  // session_id -> {call_hash -> count}
  std::map<std::string, std::map<std::string, int>> call_history_;

  // Track iteration outputs for idle detection
  // session_id -> recent iteration signatures
  std::map<std::string, std::vector<std::string>> idle_history_;
  static constexpr int kIdleWindowSize = 3;

  std::mutex mutex_;
};

}  // namespace tizenclaw

#endif  // TOOL_POLICY_HH
