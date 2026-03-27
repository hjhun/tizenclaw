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
#ifndef OFFLINE_FALLBACK_HH_
#define OFFLINE_FALLBACK_HH_

#include <json.hpp>
#include <map>
#include <string>
#include <vector>

namespace tizenclaw {

// A single keyword-to-tool mapping rule.
// When a user prompt contains the keyword(s),
// the corresponding tool is called directly
// without LLM involvement.
struct FallbackRule {
  std::string name;         // Rule identifier
  std::vector<std::string> keywords;  // Match any
  std::string tool_name;    // Tool to invoke
  nlohmann::json default_args;  // Default arguments
  std::string response_template;  // Response text
  int priority = 0;         // Higher = checked first
};

// Offline fallback result
struct FallbackResult {
  bool matched = false;       // Whether a rule matched
  std::string tool_name;      // Tool to call (empty if direct response)
  nlohmann::json args;        // Tool arguments
  std::string direct_response;  // Direct text response (no tool needed)
};

// OfflineFallback: Rule-based tool matching for
// when all LLM backends are unavailable.
//
// Ensures basic device operations continue even
// without internet connectivity. This is critical
// for consumer appliances where "turn off the oven"
// must always work regardless of cloud status.
//
// Rules are loaded from offline_fallback.json and
// matched using simple keyword presence detection.
class OfflineFallback {
 public:
  OfflineFallback() = default;
  ~OfflineFallback() = default;

  // Load fallback rules from JSON config
  [[nodiscard]] bool LoadConfig(
      const std::string& config_path);

  // Try to match a user prompt to a fallback rule.
  // Returns FallbackResult with matched=true if
  // a rule was found.
  [[nodiscard]] FallbackResult Match(
      const std::string& prompt) const;

  // Get number of loaded rules
  [[nodiscard]] size_t GetRuleCount() const {
    return rules_.size();
  }

  // Get status as JSON (for dashboard)
  [[nodiscard]] nlohmann::json GetStatusJson() const;

 private:
  // Normalize text for keyword matching
  // (lowercase, trim whitespace)
  [[nodiscard]] static std::string Normalize(
      const std::string& text);

  // Check if prompt contains a keyword
  [[nodiscard]] static bool ContainsKeyword(
      const std::string& normalized_prompt,
      const std::string& keyword);

  std::vector<FallbackRule> rules_;
  bool config_loaded_ = false;
};

}  // namespace tizenclaw

#endif  // OFFLINE_FALLBACK_HH_
