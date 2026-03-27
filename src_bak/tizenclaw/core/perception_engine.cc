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
#include "perception_engine.hh"
#include <chrono>
#include <cstdio>
#include <fstream>
#include <mutex>

#include "../../common/logging.hh"

namespace tizenclaw {

PerceptionEngine::PerceptionEngine(
    AgentCore* agent,
    SystemContextProvider* context,
    ChannelRegistry* channels)
    : agent_(agent),
      context_(context),
      profiler_(std::make_unique<DeviceProfiler>()),
      fusion_(
          std::make_unique<ContextFusionEngine>()),
      advisor_(std::make_unique<ProactiveAdvisor>(
          agent, channels)) {}

PerceptionEngine::~PerceptionEngine() {
  Stop();
}

void PerceptionEngine::Start() {
  if (running_.load()) return;

  // Subscribe to all EventBus events
  subscription_id_ =
      EventBus::GetInstance().SubscribeAll(
          [this](const SystemEvent& event) {
            OnEvent(event);
          });

  // Start analysis loop
  running_.store(true);
  analysis_thread_ = std::thread(
      &PerceptionEngine::AnalysisLoop, this);

  LOG(INFO) << "PerceptionEngine started "
            << "(hybrid: event-driven + "
            << kAnalysisIntervalSec << "s tick)";
}

void PerceptionEngine::Stop() {
  if (!running_.load()) return;

  running_.store(false);

  // Wake analysis thread for clean shutdown
  {
    std::lock_guard<std::mutex> lock(wake_mutex_);
    wake_requested_.store(true);
  }
  wake_cv_.notify_one();

  if (analysis_thread_.joinable()) {
    analysis_thread_.join();
  }

  if (subscription_id_ >= 0) {
    EventBus::GetInstance().Unsubscribe(
        subscription_id_);
    subscription_id_ = -1;
  }

  LOG(INFO) << "PerceptionEngine stopped";
}

void PerceptionEngine::OnEvent(
    const SystemEvent& event) {
  // Skip our own synthetic events to avoid loop
  if (event.source == "perception") return;

  // Feed every event to DeviceProfiler
  profiler_->RecordEvent(event);

  // If the event is significant, wake the
  // analysis thread immediately (with debounce)
  if (IsSignificantEvent(event)) {
    auto now = std::chrono::duration_cast<
                   std::chrono::milliseconds>(
                   std::chrono::system_clock::now()
                       .time_since_epoch())
                   .count();
    auto last = last_event_driven_tick_.load();
    if ((now - last) >= kEventDrivenDebounceMs) {
      // Update debounce timestamp at point of
      // wake to prevent multiple rapid wakes
      last_event_driven_tick_.store(now);
      LOG(INFO) << "PerceptionEngine: significant "
                << "event '" << event.name
                << "' — triggering analysis";
      {
        std::lock_guard<std::mutex> lock(
            wake_mutex_);
        wake_requested_.store(true);
      }
      wake_cv_.notify_one();
    }
  }
}

bool PerceptionEngine::IsSignificantEvent(
    const SystemEvent& event) {
  // Battery critical (< 15%) or charging change
  if (event.type == EventType::kBatteryChanged) {
    if (event.data.contains("level")) {
      int level = event.data["level"].get<int>();
      if (level < 15) return true;
    }
    if (event.data.contains("charging")) {
      return true;  // Charging state change
    }
  }

  // Network disconnected
  if (event.type == EventType::kNetworkChanged) {
    if (event.name == "network.disconnected") {
      return true;
    }
  }

  // Memory warning
  if (event.type == EventType::kMemoryWarning) {
    return true;
  }

  // Display off (device going to sleep)
  if (event.type == EventType::kDisplayChanged) {
    if (event.data.contains("state")) {
      std::string state =
          event.data["state"].get<std::string>();
      if (state == "off") return true;
    }
  }

  return false;
}

void PerceptionEngine::AnalysisLoop() {
  LOG(INFO) << "PerceptionEngine analysis "
            << "loop started (hybrid mode)";

  while (running_.load()) {
    // Wait on CV with periodic timeout
    {
      std::unique_lock<std::mutex> lock(
          wake_mutex_);
      wake_cv_.wait_for(
          lock,
          std::chrono::seconds(
              kAnalysisIntervalSec),
          [this]() {
            return wake_requested_.load() ||
                   !running_.load();
          });
      wake_requested_.store(false);
    }

    if (!running_.load()) break;

    RunAnalysisTick();

    // Update debounce timestamp
    last_event_driven_tick_.store(
        std::chrono::duration_cast<
            std::chrono::milliseconds>(
            std::chrono::system_clock::now()
                .time_since_epoch())
            .count());
  }

  LOG(INFO) << "PerceptionEngine analysis "
            << "loop exited";
}

void PerceptionEngine::RunAnalysisTick() {
  nlohmann::json insight;

  // Step 2: Conditional profile and situation analysis
  if (profiler_->GetEventCount() >= kMinEventsForAnalysis) {
    auto profile = profiler_->Analyze();
    nlohmann::json device_state;
    if (context_) {
      auto ctx = context_->GetContextJson();
      if (ctx.contains("device")) {
        device_state = ctx["device"];
      }
    }
    auto assessment = fusion_->Fuse(profile, device_state);
    
    insight["situation"] = ContextFusionEngine::ToJson(assessment);
    insight["trends"] = {
        {"battery_drain_rate", profile.battery_drain_rate},
        {"battery_health", profile.battery_health},
        {"memory_trend", profile.memory_trend},
        {"network_stability",
         profile.network_drop_count == 0 ? "stable" : (profile.network_drop_count < 3 ? "degraded" : "unstable")},
        {"memory_warning_count", profile.memory_warning_count}
    };
    if (!profile.top_apps.empty()) insight["top_apps"] = profile.top_apps;
    if (!profile.foreground_app.empty()) insight["foreground_app"] = profile.foreground_app;
    if (!profile.anomalies.empty()) insight["anomalies"] = profile.anomalies;

    auto advisory = advisor_->Evaluate(assessment);
    advisor_->Execute(advisory);

    if (assessment.level != SituationLevel::kNormal) {
      LOG(INFO) << "PerceptionEngine: " << ContextFusionEngine::LevelToString(assessment.level)
                << " (risk=" << static_cast<int>(assessment.risk_score * 100)
                << "%, factors=" << assessment.factors.size() << ")";
    }
  }

  // Step 3: Inject built insight into SystemContextProvider
  if (context_) {
    context_->SetPerceptionInsight(insight);
  }
}

nlohmann::json PerceptionEngine::GetInsight()
    const {
  if (advisor_) {
    return advisor_->GetLastInsight();
  }
  return {};
}

nlohmann::json PerceptionEngine::GetStatus()
    const {
  nlohmann::json status;

  // Engine state
  status["engine"] = {
      {"running", running_.load()},
      {"mode", "hybrid_event_driven"},
      {"analysis_interval_sec",
       kAnalysisIntervalSec},
      {"event_debounce_ms",
       kEventDrivenDebounceMs},
      {"event_count",
       profiler_ ? profiler_->GetEventCount()
                 : 0}};

  // Current profile snapshot
  if (profiler_) {
    auto profile = profiler_->Analyze();
    status["profile"] = {
        {"battery_level", profile.battery_level},
        {"battery_drain_rate",
         std::round(profile.battery_drain_rate *
                    100) /
             100.0},
        {"battery_health", profile.battery_health},
        {"charging", profile.charging},
        {"memory_trend", profile.memory_trend},
        {"memory_warning_count",
         profile.memory_warning_count},
        {"network_status",
         profile.network_status},
        {"network_drop_count",
         profile.network_drop_count},
        {"foreground_app",
         profile.foreground_app},
        {"top_apps", profile.top_apps}};

    if (!profile.anomalies.empty()) {
      status["anomalies"] = profile.anomalies;
    }

    // Current situation assessment
    if (fusion_) {
      nlohmann::json device_state;
      if (context_) {
        auto ctx = context_->GetContextJson();
        if (ctx.contains("device")) {
          device_state = ctx["device"];
        }
      }
      auto assessment =
          fusion_->Fuse(profile, device_state);
      status["situation"] =
          ContextFusionEngine::ToJson(assessment);
    }
  }

  // Last insight from ProactiveAdvisor
  if (advisor_) {
    auto insight = advisor_->GetLastInsight();
    if (!insight.empty()) {
      status["last_insight"] = insight;
    }
  }

  return status;
}

}  // namespace tizenclaw
