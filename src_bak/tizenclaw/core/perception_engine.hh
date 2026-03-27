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
#ifndef PERCEPTION_ENGINE_HH
#define PERCEPTION_ENGINE_HH

#include <atomic>
#include <condition_variable>
#include <json.hpp>
#include <memory>
#include <mutex>
#include <string>
#include <thread>

#include "../channel/channel_registry.hh"
#include "context_fusion_engine.hh"
#include "device_profiler.hh"
#include "event_bus.hh"
#include "proactive_advisor.hh"
#include "system_context_provider.hh"

namespace tizenclaw {

class AgentCore;

// Orchestrates the perception pipeline:
//   EventBus → DeviceProfiler → ContextFusion
//            → ProactiveAdvisor → Channels/LLM
//
// Hybrid model:
//   1. Event-driven: immediately analyzes on
//      significant events (battery critical,
//      network lost, memory warning, etc.)
//   2. Periodic: runs trend analysis every
//      kAnalysisIntervalSec for gradual changes
class PerceptionEngine {
 public:
  PerceptionEngine(AgentCore* agent,
                   SystemContextProvider* context,
                   ChannelRegistry* channels);
  ~PerceptionEngine();

  // Start event subscription and analysis loop
  void Start();
  void Stop();

  // Get latest perception insight (for external
  // queries)
  [[nodiscard]] nlohmann::json GetInsight() const;

  // Get full perception status for monitoring
  [[nodiscard]] nlohmann::json GetStatus() const;

  // Check if running
  [[nodiscard]] bool IsRunning() const {
    return running_.load();
  }

 private:
  // EventBus callback — records event and
  // wakes analysis thread if significant
  void OnEvent(const SystemEvent& event);

  // Classify whether an event should trigger
  // immediate analysis
  [[nodiscard]] static bool IsSignificantEvent(
      const SystemEvent& event);

  // Analysis loop (waits on CV, wakes on event
  // or timeout)
  void AnalysisLoop();

  // Run one analysis tick
  void RunAnalysisTick();

  AgentCore* agent_;
  SystemContextProvider* context_;

  // Sub-components (owned)
  std::unique_ptr<DeviceProfiler> profiler_;
  std::unique_ptr<ContextFusionEngine> fusion_;
  std::unique_ptr<ProactiveAdvisor> advisor_;

  // Analysis thread
  std::thread analysis_thread_;
  std::atomic<bool> running_{false};

  // Event-driven wake mechanism
  std::mutex wake_mutex_;
  std::condition_variable wake_cv_;
  std::atomic<bool> wake_requested_{false};

  // EventBus subscription
  int subscription_id_ = -1;

  // Periodic analysis interval (seconds)
  static constexpr int kAnalysisIntervalSec = 30;

  // Minimum events before first analysis
  static constexpr size_t kMinEventsForAnalysis = 2;

  // Debounce: minimum ms between event-driven
  // analysis ticks to avoid flooding
  static constexpr int kEventDrivenDebounceMs =
      3000;
  std::atomic<int64_t> last_event_driven_tick_{0};
};

}  // namespace tizenclaw

#endif  // PERCEPTION_ENGINE_HH
