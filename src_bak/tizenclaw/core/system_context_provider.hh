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
#ifndef SYSTEM_CONTEXT_PROVIDER_HH
#define SYSTEM_CONTEXT_PROVIDER_HH

#include <json.hpp>
#include <mutex>
#include <string>
#include <vector>

#include "event_bus.hh"

namespace tizenclaw {

// Subscribes to EventBus and maintains a
// normalized JSON state for LLM system prompt
// injection via {{SYSTEM_CONTEXT}} placeholder.
class SystemContextProvider {
 public:
  SystemContextProvider();
  ~SystemContextProvider();

  // Start subscribing to EventBus
  void Start();
  void Stop();

  // Get current system context as formatted
  // string for system prompt injection
  [[nodiscard]] std::string GetContextString() const;

  // Get current context as JSON
  [[nodiscard]] nlohmann::json GetContextJson() const;

  // Set perception insight from PerceptionEngine
  void SetPerceptionInsight(
      const nlohmann::json& insight);

 private:
  // EventBus callback
  void OnEvent(const SystemEvent& event);

  // Update state based on event type
  void UpdateDeviceState(const SystemEvent& event);
  void UpdateRuntimeState(const SystemEvent& event);
  void UpdateAppState(const SystemEvent& event);
  void AddRecentEvent(const SystemEvent& event);

  // Current state (normalized, interpreted)
  mutable std::mutex state_mutex_;
  nlohmann::json device_state_;     // display, BT, WiFi, model
  nlohmann::json runtime_state_;    // network, memory, power, battery
  nlohmann::json app_state_;        // recent app events
  nlohmann::json perception_insight_;  // from PerceptionEngine

  // Recent events (ring buffer, max 10)
  struct RecentEvent {
    std::string time;
    std::string source;
    std::string event_name;
    std::string detail;
  };
  std::vector<RecentEvent> recent_events_;
  static constexpr size_t kMaxRecentEvents = 10;

  // Subscription IDs
  int subscription_id_ = -1;
  bool started_ = false;
};

}  // namespace tizenclaw

#endif  // SYSTEM_CONTEXT_PROVIDER_HH
