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
#include "capability_registry.hh"

#include <algorithm>

#include "../../common/logging.hh"
#include "tool_router.hh"

namespace tizenclaw {

CapabilityRegistry& CapabilityRegistry::GetInstance() {
  static CapabilityRegistry instance;
  return instance;
}

void CapabilityRegistry::Register(
    const std::string& name, const Capability& cap) {
  std::lock_guard<std::mutex> lock(mutex_);
  capabilities_[name] = cap;
}

void CapabilityRegistry::Unregister(
    const std::string& name) {
  std::lock_guard<std::mutex> lock(mutex_);
  capabilities_.erase(name);
}

void CapabilityRegistry::Clear() {
  std::lock_guard<std::mutex> lock(mutex_);
  capabilities_.clear();
}

const Capability* CapabilityRegistry::Get(
    const std::string& name) const {
  std::lock_guard<std::mutex> lock(mutex_);
  auto it = capabilities_.find(name);
  if (it == capabilities_.end()) return nullptr;
  return &it->second;
}

std::vector<Capability>
CapabilityRegistry::QueryByCategory(
    const std::string& category) const {
  std::lock_guard<std::mutex> lock(mutex_);
  std::vector<Capability> result;
  for (const auto& [name, cap] : capabilities_) {
    if (cap.category == category)
      result.push_back(cap);
  }
  return result;
}

std::vector<Capability>
CapabilityRegistry::QueryBySideEffect(
    SideEffect effect) const {
  std::lock_guard<std::mutex> lock(mutex_);
  std::vector<Capability> result;
  for (const auto& [name, cap] : capabilities_) {
    if (cap.contract.side_effect == effect)
      result.push_back(cap);
  }
  return result;
}

std::vector<Capability>
CapabilityRegistry::QueryByPermission(
    const std::string& permission) const {
  std::lock_guard<std::mutex> lock(mutex_);
  std::vector<Capability> result;
  for (const auto& [name, cap] : capabilities_) {
    const auto& perms = cap.contract.required_permissions;
    if (std::find(perms.begin(), perms.end(),
                  permission) != perms.end()) {
      result.push_back(cap);
    }
  }
  return result;
}

std::vector<std::string>
CapabilityRegistry::GetAllNames() const {
  std::lock_guard<std::mutex> lock(mutex_);
  std::vector<std::string> names;
  names.reserve(capabilities_.size());
  for (const auto& [name, cap] : capabilities_)
    names.push_back(name);
  return names;
}

size_t CapabilityRegistry::Size() const {
  std::lock_guard<std::mutex> lock(mutex_);
  return capabilities_.size();
}

std::vector<std::pair<std::string, std::string>>
CapabilityRegistry::DetectOverlaps() const {
  std::lock_guard<std::mutex> lock(mutex_);

  // Group capabilities by category
  std::map<std::string,
           std::vector<
               std::pair<std::string,
                         CapabilitySource>>>
      by_category;

  for (const auto& [name, cap] : capabilities_) {
    std::string cat =
        cap.category.empty() ? "general"
                             : cap.category;
    by_category[cat].emplace_back(
        name, cap.source);
  }

  std::vector<
      std::pair<std::string, std::string>>
      overlaps;

  for (const auto& [cat, tools] : by_category) {
    if (tools.size() < 2) continue;

    // Find the highest-priority tool in category
    const std::string* best_name = nullptr;
    int best_prio = 99;
    for (const auto& [tname, src] : tools) {
      int prio = ToolRouter::SourcePriority(src);
      if (prio < best_prio) {
        best_prio = prio;
        best_name = &tname;
      }
    }

    if (!best_name) continue;

    // All lower-priority tools redirect to best
    for (const auto& [tname, src] : tools) {
      int prio = ToolRouter::SourcePriority(src);
      if (prio > best_prio) {
        overlaps.emplace_back(tname, *best_name);
        LOG(WARNING)
            << "CapabilityRegistry: Overlap "
            << "detected in '" << cat
            << "': '" << tname << "' ("
            << prio << ") -> '" << *best_name
            << "' (" << best_prio << ")";
      }
    }
  }

  return overlaps;
}

nlohmann::json
CapabilityRegistry::GetCapabilitySummary() const {
  std::lock_guard<std::mutex> lock(mutex_);

  // Group by category
  std::map<std::string, std::vector<std::string>>
      categories;
  // Count side effects
  int none_count = 0, reversible_count = 0;
  int irreversible_count = 0, unknown_count = 0;

  for (const auto& [name, cap] : capabilities_) {
    std::string cat =
        cap.category.empty() ? "general" : cap.category;
    std::string entry = name;

    // Add side-effect and duration info
    std::string se =
        SideEffectToString(cap.contract.side_effect);
    int dur_sec =
        cap.contract.estimated_duration_ms / 1000;
    entry += " (" + se + ", " +
             std::to_string(dur_sec) + "s";
    if (!cap.contract.required_permissions.empty()) {
      entry += ", requires:";
      for (const auto& p :
           cap.contract.required_permissions) {
        entry += " " + p;
      }
    }
    entry += ")";
    categories[cat].push_back(entry);

    switch (cap.contract.side_effect) {
      case SideEffect::kNone:
        none_count++;
        break;
      case SideEffect::kReversible:
        reversible_count++;
        break;
      case SideEffect::kIrreversible:
        irreversible_count++;
        break;
      case SideEffect::kUnknown:
        unknown_count++;
        break;
    }
  }

  nlohmann::json summary;
  summary["categories"] = categories;
  summary["total_capabilities"] = capabilities_.size();
  summary["side_effect_summary"] = {
      {"none", none_count},
      {"reversible", reversible_count},
      {"irreversible", irreversible_count},
      {"unknown", unknown_count}};
  return summary;
}

FunctionContract CapabilityRegistry::ParseContract(
    const nlohmann::json& j) {
  FunctionContract contract;
  if (!j.is_object()) return contract;

  if (j.contains("side_effect")) {
    contract.side_effect =
        ParseSideEffect(j["side_effect"]);
  }
  contract.max_retries = j.value("max_retries", 0);
  contract.retry_delay_ms =
      j.value("retry_delay_ms", 1000);
  contract.idempotent = j.value("idempotent", false);
  contract.estimated_duration_ms =
      j.value("estimated_duration_ms", 5000);
  contract.execution_env =
      j.value("execution_env", "container");

  if (j.contains("required_permissions") &&
      j["required_permissions"].is_array()) {
    for (const auto& p : j["required_permissions"])
      contract.required_permissions.push_back(
          p.get<std::string>());
  }

  return contract;
}

std::string CapabilityRegistry::SideEffectToString(
    SideEffect effect) {
  switch (effect) {
    case SideEffect::kNone:
      return "read-only";
    case SideEffect::kReversible:
      return "reversible";
    case SideEffect::kIrreversible:
      return "irreversible";
    case SideEffect::kUnknown:
      return "unknown";
  }
  return "unknown";
}

SideEffect CapabilityRegistry::ParseSideEffect(
    const std::string& str) {
  if (str == "none" || str == "read-only")
    return SideEffect::kNone;
  if (str == "reversible")
    return SideEffect::kReversible;
  if (str == "irreversible")
    return SideEffect::kIrreversible;
  return SideEffect::kUnknown;
}

}  // namespace tizenclaw
