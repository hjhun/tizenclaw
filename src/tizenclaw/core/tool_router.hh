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
#ifndef TOOL_ROUTER_HH_
#define TOOL_ROUTER_HH_

#include <json.hpp>
#include <map>
#include <mutex>
#include <string>
#include <vector>

#include "capability_registry.hh"

namespace tizenclaw {

// Runtime tool routing layer.
// Resolves tool name conflicts by redirecting
// calls to higher-priority alternatives.
//
// Priority order (lower number = higher priority):
//   kAction(0) > kBuiltin(1) > kSystemCli(2)
//   > kSkill(3) > kCli(4) > kRpk(5)
//
// Two resolution mechanisms:
//   1. Manual aliases from tool_policy.json
//   2. Auto-detected overlaps from
//      CapabilityRegistry (same category,
//      different source priority)
class ToolRouter {
 public:
  ToolRouter() = default;

  // Resolve a tool name to the best alternative.
  // Returns the redirected name, or the original
  // if no better option exists.
  [[nodiscard]] std::string Resolve(
      const std::string& name) const;

  // Load manual alias map from JSON object.
  // Format: {"old_name": "new_name", ...}
  void LoadAliases(const nlohmann::json& aliases);

  // Register an auto-detected overlap pair.
  // lower_priority_tool → higher_priority_tool
  void RegisterOverlap(
      const std::string& lower,
      const std::string& higher);

  // Check if a tool name has a redirect
  [[nodiscard]] bool HasRedirect(
      const std::string& name) const;

  // Get all registered redirects (aliases +
  // overlaps) for diagnostics
  [[nodiscard]] std::map<std::string, std::string>
  GetAllRedirects() const;

  // Get the numeric priority for a capability
  // source. Lower = higher priority.
  static int SourcePriority(CapabilitySource src);

  // Clear all aliases and overlaps
  void Clear();

 private:
  // Manual alias overrides: old_name → new_name
  std::map<std::string, std::string> aliases_;
  // Auto-detected overlap: lower → higher
  std::map<std::string, std::string> overlaps_;
  mutable std::mutex mutex_;
};

}  // namespace tizenclaw

#endif  // TOOL_ROUTER_HH_
