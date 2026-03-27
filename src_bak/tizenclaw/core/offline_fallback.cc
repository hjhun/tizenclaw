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
#include "offline_fallback.hh"

#include <algorithm>
#include <fstream>

#include "../../common/logging.hh"

namespace tizenclaw {

bool OfflineFallback::LoadConfig(
    const std::string& config_path) {
  std::ifstream f(config_path);
  if (!f.is_open()) {
    LOG(INFO) << "OfflineFallback: no config at "
              << config_path
              << " (offline fallback disabled)";
    return false;
  }

  try {
    nlohmann::json j;
    f >> j;

    if (!j.contains("rules") ||
        !j["rules"].is_array()) {
      LOG(WARNING) << "OfflineFallback: config "
                   << "missing 'rules' array";
      return false;
    }

    rules_.clear();
    for (const auto& rule_j : j["rules"]) {
      FallbackRule rule;
      rule.name = rule_j.value("name", "");
      rule.tool_name = rule_j.value("tool", "");
      rule.response_template =
          rule_j.value("response", "");
      rule.priority = rule_j.value("priority", 0);
      rule.default_args =
          rule_j.value("args", nlohmann::json::object());

      if (rule_j.contains("keywords") &&
          rule_j["keywords"].is_array()) {
        for (const auto& kw : rule_j["keywords"]) {
          if (kw.is_string()) {
            rule.keywords.push_back(
                Normalize(kw.get<std::string>()));
          }
        }
      }

      if (!rule.keywords.empty() &&
          (!rule.tool_name.empty() ||
           !rule.response_template.empty())) {
        rules_.push_back(std::move(rule));
      }
    }

    // Sort by priority (higher first)
    std::sort(rules_.begin(), rules_.end(),
              [](const FallbackRule& a,
                 const FallbackRule& b) {
                return a.priority > b.priority;
              });

    config_loaded_ = true;
    LOG(INFO) << "OfflineFallback: loaded "
              << rules_.size() << " rules";
    return true;
  } catch (const std::exception& e) {
    LOG(ERROR) << "OfflineFallback: config parse "
               << "error: " << e.what();
    return false;
  }
}

FallbackResult OfflineFallback::Match(
    const std::string& prompt) const {
  FallbackResult result;

  if (rules_.empty()) return result;

  std::string normalized = Normalize(prompt);

  for (const auto& rule : rules_) {
    bool matched = false;
    for (const auto& kw : rule.keywords) {
      if (ContainsKeyword(normalized, kw)) {
        matched = true;
        break;
      }
    }

    if (matched) {
      result.matched = true;
      result.tool_name = rule.tool_name;
      result.args = rule.default_args;
      result.direct_response =
          rule.response_template;
      return result;
    }
  }

  return result;
}

nlohmann::json OfflineFallback::GetStatusJson() const {
  nlohmann::json status;
  status["config_loaded"] = config_loaded_;
  status["rule_count"] =
      static_cast<int>(rules_.size());

  nlohmann::json rule_names =
      nlohmann::json::array();
  for (const auto& r : rules_) {
    rule_names.push_back(r.name);
  }
  status["rules"] = rule_names;

  return status;
}

std::string OfflineFallback::Normalize(
    const std::string& text) {
  std::string result;
  result.reserve(text.size());
  for (char c : text) {
    result += static_cast<char>(
        std::tolower(static_cast<unsigned char>(c)));
  }
  return result;
}

bool OfflineFallback::ContainsKeyword(
    const std::string& normalized_prompt,
    const std::string& keyword) {
  return normalized_prompt.find(keyword) !=
         std::string::npos;
}

}  // namespace tizenclaw
