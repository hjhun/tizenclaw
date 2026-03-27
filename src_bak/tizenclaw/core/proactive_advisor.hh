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
#ifndef PROACTIVE_ADVISOR_HH
#define PROACTIVE_ADVISOR_HH

#include <atomic>
#include <json.hpp>
#include <map>
#include <mutex>
#include <string>
#include <thread>

#include "../channel/channel_registry.hh"
#include "context_fusion_engine.hh"
#include "event_bus.hh"

namespace tizenclaw {

class AgentCore;

// Advisory action type
enum class AdvisoryAction {
  kSuppress,   // No action (normal situation)
  kInject,     // Inject into system context only
  kNotify,     // Send notification via channels
  kEvaluate    // Forward to LLM for evaluation
};

// Advisory produced by ProactiveAdvisor
struct Advisory {
  AdvisoryAction action;
  std::string message;       // Human-readable message
  nlohmann::json context;    // Context for LLM
  std::string channel;       // Target channel (or "all")
};

// Evaluates SituationAssessment and decides
// whether/how to proactively inform the user.
// Publishes synthetic events to EventBus for
// AutonomousTrigger integration.
class ProactiveAdvisor {
 public:
  ProactiveAdvisor(AgentCore* agent,
                   ChannelRegistry* channels);
  ~ProactiveAdvisor();


  // Evaluate a situation and produce an advisory
  [[nodiscard]] Advisory Evaluate(
      const SituationAssessment& assessment);

  // Execute the advisory (notify/evaluate/etc.)
  void Execute(const Advisory& advisory);

  // Get last assessment (for injection)
  [[nodiscard]] nlohmann::json
  GetLastInsight() const;

 private:
  // Check cooldown for situation type
  [[nodiscard]] bool IsCoolingDown(
      const std::string& situation_key) const;
  void RecordAction(
      const std::string& situation_key);

  // Build notification message
  [[nodiscard]] std::string BuildNotification(
      const SituationAssessment& assessment) const;

  // Publish synthetic event to EventBus
  void PublishSituationEvent(
      const SituationAssessment& assessment);

  AgentCore* agent_;
  ChannelRegistry* channels_;

  // Last assessment for context injection
  mutable std::mutex state_mutex_;
  nlohmann::json last_insight_;
  SituationLevel last_level_ =
      SituationLevel::kNormal;

  // Cooldown tracking: key -> last_trigger_time
  mutable std::mutex cooldown_mutex_;
  std::map<std::string, int64_t> cooldowns_;

  // Cooldown periods (minutes)
  static constexpr int kAdvisoryCooldownMin = 30;
  static constexpr int kWarningCooldownMin = 15;
  static constexpr int kCriticalCooldownMin = 5;

  // Notification channel preference
  std::string notification_channel_ = "all";

  // LLM evaluation thread (joinable, replaces
  // detached thread to prevent UAF)
  std::thread eval_thread_;
  std::atomic<bool> eval_running_{false};
  void JoinEvalThread();
};

}  // namespace tizenclaw

#endif  // PROACTIVE_ADVISOR_HH
