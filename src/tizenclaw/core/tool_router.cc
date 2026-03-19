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
#include "tool_router.hh"

#include "../../common/logging.hh"

namespace tizenclaw {

std::string ToolRouter::Resolve(
    const std::string& name) const {
  std::lock_guard<std::mutex> lock(mutex_);

  // 1. Manual aliases take highest precedence
  auto alias_it = aliases_.find(name);
  if (alias_it != aliases_.end()) {
    LOG(INFO) << "ToolRouter: Redirected '"
              << name << "' -> '"
              << alias_it->second
              << "' (alias)";
    return alias_it->second;
  }

  // 2. Auto-detected overlaps
  auto overlap_it = overlaps_.find(name);
  if (overlap_it != overlaps_.end()) {
    LOG(INFO) << "ToolRouter: Redirected '"
              << name << "' -> '"
              << overlap_it->second
              << "' (overlap)";
    return overlap_it->second;
  }

  // 3. No redirect — use original
  return name;
}

void ToolRouter::LoadAliases(
    const nlohmann::json& aliases) {
  std::lock_guard<std::mutex> lock(mutex_);
  aliases_.clear();

  if (!aliases.is_object()) return;

  for (auto& [key, val] : aliases.items()) {
    if (!val.is_string()) continue;
    std::string target = val.get<std::string>();
    if (target.empty()) continue;

    // Prevent self-referencing aliases
    if (key == target) {
      LOG(WARNING) << "ToolRouter: "
                   << "Ignoring self-alias '"
                   << key << "'";
      continue;
    }

    aliases_[key] = target;
    LOG(INFO) << "ToolRouter: Alias '"
              << key << "' -> '" << target << "'";
  }
}

void ToolRouter::RegisterOverlap(
    const std::string& lower,
    const std::string& higher) {
  std::lock_guard<std::mutex> lock(mutex_);

  if (lower == higher) return;

  // Don't override manual aliases
  if (aliases_.count(lower) > 0) {
    LOG(INFO) << "ToolRouter: Overlap '"
              << lower << "' -> '" << higher
              << "' skipped (alias exists)";
    return;
  }

  overlaps_[lower] = higher;
  LOG(INFO) << "ToolRouter: Overlap '"
            << lower << "' -> '" << higher << "'";
}

bool ToolRouter::HasRedirect(
    const std::string& name) const {
  std::lock_guard<std::mutex> lock(mutex_);
  return aliases_.count(name) > 0 ||
         overlaps_.count(name) > 0;
}

std::map<std::string, std::string>
ToolRouter::GetAllRedirects() const {
  std::lock_guard<std::mutex> lock(mutex_);
  std::map<std::string, std::string> result;

  // Overlaps first, then aliases override
  for (const auto& [k, v] : overlaps_)
    result[k] = v;
  for (const auto& [k, v] : aliases_)
    result[k] = v;

  return result;
}

int ToolRouter::SourcePriority(
    CapabilitySource src) {
  switch (src) {
    case CapabilitySource::kAction:
      return 0;
    case CapabilitySource::kBuiltin:
      return 1;
    case CapabilitySource::kSystemCli:
      return 2;
    case CapabilitySource::kSkill:
      return 3;
    case CapabilitySource::kCli:
      return 4;
    case CapabilitySource::kRpk:
      return 5;
  }
  return 99;
}

void ToolRouter::Clear() {
  std::lock_guard<std::mutex> lock(mutex_);
  aliases_.clear();
  overlaps_.clear();
}

}  // namespace tizenclaw
