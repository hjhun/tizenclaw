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
#ifndef CAPABILITY_REGISTRY_HH_
#define CAPABILITY_REGISTRY_HH_

#include <json.hpp>
#include <map>
#include <mutex>
#include <string>
#include <utility>
#include <vector>

namespace tizenclaw {

// Side effect classification for Planning
// Agent decision-making
enum class SideEffect {
  kNone,          // Read-only (get_battery_info)
  kReversible,    // Can be undone (control_volume)
  kIrreversible,  // Cannot be undone (send_notification)
  kUnknown,       // Legacy tools without contract
};

// Function contract defining tool behavior
// guarantees
struct FunctionContract {
  SideEffect side_effect = SideEffect::kUnknown;
  int max_retries = 0;
  int retry_delay_ms = 1000;
  bool idempotent = false;
  std::vector<std::string> required_permissions;
  int estimated_duration_ms = 5000;
  std::string execution_env;  // "container"|"host"|"action_framework"
};

// Source of the capability registration
enum class CapabilitySource {
  kSkill,       // Python container skills (Standard)
  kCustomSkill, // Runtime-generated custom skills
  kAction,      // Tizen Action Framework
  kBuiltin,     // Built-in C++ tools (Embedded)
  kSystemCli,   // System CLI tools (/usr/bin)
  kCli,         // CLI tool plugins (TPK)
  kRpk,         // RPK plugin skills
};

// Full capability descriptor for a tool
struct Capability {
  std::string name;
  std::string description;
  std::string category;
  CapabilitySource source = CapabilitySource::kSkill;
  FunctionContract contract;
};

// Central registry of all tool capabilities.
// Thread-safe singleton for use across
// AgentCore, ToolPolicy, and ToolIndexer.
class CapabilityRegistry {
 public:
  static CapabilityRegistry& GetInstance();

  // Register a capability
  void Register(const std::string& name,
                const Capability& cap);

  // Unregister a capability
  void Unregister(const std::string& name);

  // Clear all registered capabilities
  void Clear();

  // Get capability by name (nullptr if not found)
  [[nodiscard]] const Capability* Get(
      const std::string& name) const;

  // Query capabilities by category
  [[nodiscard]] std::vector<Capability> QueryByCategory(
      const std::string& category) const;

  // Query capabilities by side effect type
  [[nodiscard]] std::vector<Capability> QueryBySideEffect(
      SideEffect effect) const;

  // Query capabilities requiring a permission
  [[nodiscard]] std::vector<Capability>
  QueryByPermission(
      const std::string& permission) const;

  // Get all registered capability names
  [[nodiscard]] std::vector<std::string>
  GetAllNames() const;

  // Get count of registered capabilities
  [[nodiscard]] size_t Size() const;

  // Detect overlapping tools: same category,
  // different source priority. Returns pairs of
  // (lower_priority_name, higher_priority_name).
  [[nodiscard]] std::vector<std::pair<
      std::string, std::string>>
  DetectOverlaps() const;

  // Generate JSON summary for LLM system prompt
  [[nodiscard]] nlohmann::json
  GetCapabilitySummary() const;

  // Parse FunctionContract from manifest JSON
  static FunctionContract ParseContract(
      const nlohmann::json& j);

  // Convert SideEffect to string
  static std::string SideEffectToString(
      SideEffect effect);

  // Parse SideEffect from string
  static SideEffect ParseSideEffect(
      const std::string& str);

 private:
  CapabilityRegistry() = default;
  ~CapabilityRegistry() = default;
  CapabilityRegistry(const CapabilityRegistry&) = delete;
  CapabilityRegistry& operator=(
      const CapabilityRegistry&) = delete;

  std::map<std::string, Capability> capabilities_;
  mutable std::mutex mutex_;
};

}  // namespace tizenclaw

#endif  // CAPABILITY_REGISTRY_HH_
