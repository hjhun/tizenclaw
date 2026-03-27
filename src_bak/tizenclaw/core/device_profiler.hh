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
#ifndef DEVICE_PROFILER_HH
#define DEVICE_PROFILER_HH

#include <deque>
#include <json.hpp>
#include <mutex>
#include <string>
#include <vector>

#include "event_bus.hh"

namespace tizenclaw {

// Snapshot of analyzed device profile trends
struct ProfileSnapshot {
  double battery_drain_rate = 0.0;  // %/min
  std::string battery_health = "unknown";
  int battery_level = -1;
  bool charging = false;

  int memory_warning_count = 0;  // in analysis window
  std::string memory_trend = "unknown";

  int network_drop_count = 0;  // in analysis window
  std::string network_status = "unknown";

  std::vector<std::string> top_apps;
  std::string foreground_app;

  nlohmann::json anomalies = nlohmann::json::array();
};

// Records time-series events in a ring buffer
// and computes trend analysis / anomaly detection.
class DeviceProfiler {
 public:
  DeviceProfiler();
  ~DeviceProfiler() = default;

  // Record a raw system event
  void RecordEvent(const SystemEvent& event);

  // Analyze recent events and produce a snapshot
  [[nodiscard]] ProfileSnapshot Analyze() const;

  // Get event count in buffer
  [[nodiscard]] size_t GetEventCount() const;

 private:
  // Timestamped event record
  struct EventRecord {
    int64_t timestamp;
    EventType type;
    std::string name;
    nlohmann::json data;
  };

  // Battery sample for drain rate calculation
  struct BatterySample {
    int64_t timestamp;
    int level;
    bool charging;  // Track per-sample charging state
  };

  // Compute battery drain rate (%/min)
  [[nodiscard]] double ComputeDrainRate() const;

  // Classify battery health based on drain rate
  [[nodiscard]] static std::string ClassifyBatteryHealth(
      double drain_rate, int level, bool charging);

  // Detect anomalies in recent events
  [[nodiscard]] nlohmann::json DetectAnomalies() const;

  mutable std::mutex mutex_;

  // Ring buffer of recent events
  std::deque<EventRecord> events_;
  static constexpr size_t kMaxEvents = 200;

  // Battery samples for drain rate
  std::deque<BatterySample> battery_samples_;
  static constexpr size_t kMaxBatterySamples = 30;

  // Latest state tracking
  int latest_battery_level_ = -1;
  bool latest_charging_ = false;
  std::string latest_network_status_ = "unknown";
  std::string latest_foreground_app_;
  std::vector<std::string> recent_apps_;

  // Analysis window (minutes)
  static constexpr int kAnalysisWindowMin = 30;
};

}  // namespace tizenclaw

#endif  // DEVICE_PROFILER_HH
