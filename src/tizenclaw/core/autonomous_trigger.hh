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
#ifndef AUTONOMOUS_TRIGGER_HH
#define AUTONOMOUS_TRIGGER_HH

#include <json.hpp>
#include <map>
#include <mutex>
#include <string>
#include <vector>

#include "../channel/channel_registry.hh"
#include "event_bus.hh"
#include "system_context_provider.hh"

namespace tizenclaw {

class AgentCore;

// Rule definition for autonomous event matching
struct EventRule {
  std::string name;
  std::string event_type;        // e.g. "battery.level_changed"
  nlohmann::json condition;      // {"level": {"$lt": 15}}
  int cooldown_minutes = 10;
  std::string action;            // "evaluate" | "direct"
  std::string direct_prompt;     // prompt for action="direct"
};

// Two-phase autonomous trigger system:
// Phase 1: Rule-based filter (fast, no cost)
// Phase 2: LLM-based evaluation (smart, costly)
class AutonomousTrigger {
 public:
  AutonomousTrigger(AgentCore* agent,
                    SystemContextProvider* context,
                    ChannelRegistry* channels = nullptr);
  ~AutonomousTrigger();

  // Load trigger rules from JSON config
  [[nodiscard]] bool LoadRules(
      const std::string& config_path);

  // Start/Stop subscribing to EventBus
  void Start();
  void Stop();

  // List loaded rules
  [[nodiscard]] nlohmann::json ListRules() const;

  // Check if enabled
  [[nodiscard]] bool IsEnabled() const {
    return enabled_;
  }

 private:
  // EventBus callback
  void OnEvent(const SystemEvent& event);

  // Phase 1: Rule matching
  [[nodiscard]] bool MatchRule(
      const EventRule& rule,
      const SystemEvent& event) const;

  // Evaluate condition operators
  [[nodiscard]] bool EvalCondition(
      const nlohmann::json& condition,
      const nlohmann::json& data) const;

  // Check cooldown
  [[nodiscard]] bool IsCoolingDown(
      const std::string& rule_name) const;
  void RecordTrigger(const std::string& rule_name);

  // Phase 2: LLM evaluation
  void EvaluateWithLlm(const EventRule& rule,
                       const SystemEvent& event);

  // Execute autonomous action
  void ExecuteAction(const std::string& action,
                     const std::string& prompt,
                     const std::string& reason);

  // Send notification to channels
  void Notify(const std::string& message);

  // Rate limiting
  [[nodiscard]] bool CheckRateLimit();

  AgentCore* agent_;
  SystemContextProvider* context_;
  ChannelRegistry* channels_;
  std::vector<EventRule> rules_;
  mutable std::mutex rules_mutex_;

  // Cooldown tracking
  std::map<std::string, int64_t> last_trigger_;
  mutable std::mutex cooldown_mutex_;

  // Rate limiting
  int64_t hour_start_ = 0;
  int eval_count_ = 0;
  int max_evals_per_hour_ = 10;
  std::mutex rate_mutex_;

  // Configuration
  bool enabled_ = false;
  std::string eval_session_ = "autonomous";
  std::string notification_channel_ = "telegram";

  int subscription_id_ = -1;
  bool started_ = false;
};

}  // namespace tizenclaw

#endif  // AUTONOMOUS_TRIGGER_HH
