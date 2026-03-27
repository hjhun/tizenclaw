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
#include "device_profiler.hh"

#include <algorithm>
#include <chrono>
#include <map>
#include <set>

#include "../../common/logging.hh"

namespace tizenclaw {

namespace {

int64_t NowMs() {
  return std::chrono::duration_cast<
             std::chrono::milliseconds>(
             std::chrono::system_clock::now()
                 .time_since_epoch())
      .count();
}

}  // namespace

DeviceProfiler::DeviceProfiler() = default;

void DeviceProfiler::RecordEvent(
    const SystemEvent& event) {
  std::lock_guard<std::mutex> lock(mutex_);

  // Store in ring buffer
  EventRecord record;
  record.timestamp = event.timestamp > 0
                         ? event.timestamp
                         : NowMs();
  record.type = event.type;
  record.name = event.name;
  record.data = event.data;

  events_.push_back(std::move(record));
  if (events_.size() > kMaxEvents) {
    events_.pop_front();
  }

  // Update latest state tracking
  if (event.type == EventType::kBatteryChanged) {
    if (event.data.contains("level")) {
      latest_battery_level_ =
          event.data["level"].get<int>();

      BatterySample sample;
      sample.timestamp = record.timestamp;
      sample.level = latest_battery_level_;
      sample.charging = latest_charging_;
      battery_samples_.push_back(sample);
      if (battery_samples_.size() >
          kMaxBatterySamples) {
        battery_samples_.pop_front();
      }
    }
    if (event.data.contains("charging")) {
      latest_charging_ =
          event.data["charging"].get<bool>();
    }
  } else if (event.type ==
             EventType::kNetworkChanged) {
    if (event.data.contains("status")) {
      latest_network_status_ =
          event.data["status"].get<std::string>();
    }
  } else if (event.type ==
             EventType::kAppLifecycle) {
    if (event.data.contains("app_id") &&
        event.data.contains("state")) {
      std::string app_id =
          event.data["app_id"].get<std::string>();
      std::string state =
          event.data["state"].get<std::string>();
      if (state == "resumed" ||
          state == "launched") {
        latest_foreground_app_ = app_id;
        // Track in recent apps (max 10 unique)
        auto it = std::find(recent_apps_.begin(),
                            recent_apps_.end(),
                            app_id);
        if (it != recent_apps_.end()) {
          recent_apps_.erase(it);
        }
        recent_apps_.insert(
            recent_apps_.begin(), app_id);
        if (recent_apps_.size() > 10) {
          recent_apps_.pop_back();
        }
      }
    }
  } else if (event.type ==
             EventType::kRecentApp) {
    if (event.data.contains("recent_apps") &&
        event.data["recent_apps"].is_array()) {
      recent_apps_.clear();
      for (const auto& app :
           event.data["recent_apps"]) {
        if (app.is_string()) {
          recent_apps_.push_back(
              app.get<std::string>());
        }
      }
    }
  }
}

ProfileSnapshot DeviceProfiler::Analyze() const {
  std::lock_guard<std::mutex> lock(mutex_);

  ProfileSnapshot snap;
  snap.battery_level = latest_battery_level_;
  snap.charging = latest_charging_;
  snap.network_status = latest_network_status_;
  snap.foreground_app = latest_foreground_app_;

  // Battery drain rate
  snap.battery_drain_rate = ComputeDrainRate();
  snap.battery_health = ClassifyBatteryHealth(
      snap.battery_drain_rate,
      snap.battery_level, snap.charging);

  // Analyze events in the analysis window
  int64_t cutoff =
      NowMs() - (kAnalysisWindowMin * 60 * 1000LL);
  int mem_warnings = 0;
  int net_drops = 0;
  std::map<std::string, int> app_freq;

  for (const auto& ev : events_) {
    if (ev.timestamp < cutoff) continue;

    if (ev.type == EventType::kMemoryWarning) {
      mem_warnings++;
    } else if (ev.type ==
               EventType::kNetworkChanged) {
      if (ev.name == "network.disconnected") {
        net_drops++;
      }
    } else if (ev.type ==
               EventType::kAppLifecycle) {
      if (ev.data.contains("app_id")) {
        std::string app =
            ev.data["app_id"].get<std::string>();
        app_freq[app]++;
      }
    }
  }

  snap.memory_warning_count = mem_warnings;
  snap.network_drop_count = net_drops;

  // Classify memory trend
  if (mem_warnings >= 3) {
    snap.memory_trend = "critical";
  } else if (mem_warnings >= 1) {
    snap.memory_trend = "rising";
  } else {
    snap.memory_trend = "stable";
  }

  // Top apps by frequency
  std::vector<std::pair<std::string, int>>
      app_list(app_freq.begin(), app_freq.end());
  std::sort(app_list.begin(), app_list.end(),
            [](const auto& a, const auto& b) {
              return a.second > b.second;
            });
  for (size_t i = 0;
       i < std::min(app_list.size(), (size_t)5);
       i++) {
    snap.top_apps.push_back(app_list[i].first);
  }

  // If no current top_apps from events,
  // use recent_apps tracking
  if (snap.top_apps.empty() &&
      !recent_apps_.empty()) {
    snap.top_apps = recent_apps_;
    if (snap.top_apps.size() > 5) {
      snap.top_apps.resize(5);
    }
  }

  // Anomaly detection
  snap.anomalies = DetectAnomalies();

  return snap;
}

size_t DeviceProfiler::GetEventCount() const {
  std::lock_guard<std::mutex> lock(mutex_);
  return events_.size();
}

double DeviceProfiler::ComputeDrainRate() const {
  // Need at least 2 samples while not charging
  if (battery_samples_.size() < 2) return 0.0;

  // Filter to only non-charging samples
  std::vector<BatterySample> discharge_samples;
  for (const auto& s : battery_samples_) {
    if (!s.charging)
      discharge_samples.push_back(s);
  }

  if (discharge_samples.size() < 2) return 0.0;

  // Use first and last discharge samples
  const auto& first = discharge_samples.front();
  const auto& last = discharge_samples.back();

  int64_t dt_ms = last.timestamp - first.timestamp;
  if (dt_ms <= 0) return 0.0;

  double dt_min = dt_ms / 60000.0;
  if (dt_min < 1.0) return 0.0;

  int delta_level = first.level - last.level;

  // Only compute drain if battery is decreasing
  if (delta_level <= 0) return 0.0;

  return delta_level / dt_min;
}

std::string DeviceProfiler::ClassifyBatteryHealth(
    double drain_rate, int level, bool charging) {
  if (charging) return "charging";
  if (level < 0) return "unknown";

  if (level < 5) return "critical";
  if (level < 15 || drain_rate > 2.0) {
    return "degrading";
  }
  return "good";
}

nlohmann::json
DeviceProfiler::DetectAnomalies() const {
  auto anomalies = nlohmann::json::array();

  int64_t cutoff =
      NowMs() - (10 * 60 * 1000LL);  // 10 min

  // Count rapid battery drain
  // (> 5% in 10 minutes)
  if (battery_samples_.size() >= 2) {
    auto recent_begin = battery_samples_.end();
    for (auto it = battery_samples_.begin();
         it != battery_samples_.end(); ++it) {
      if (it->timestamp >= cutoff) {
        recent_begin = it;
        break;
      }
    }
    if (recent_begin != battery_samples_.end() &&
        recent_begin != battery_samples_.begin()) {
      auto prev = std::prev(recent_begin);
      int drain = prev->level -
                  battery_samples_.back().level;
      if (drain > 5 && !latest_charging_) {
        anomalies.push_back({
            {"type", "rapid_battery_drain"},
            {"detail", "Battery dropped " +
                           std::to_string(drain) +
                           "% in ~10 minutes"},
            {"severity", "warning"}});
      }
    }
  }

  // Count memory warnings in short window
  int short_mem_warnings = 0;
  for (const auto& ev : events_) {
    if (ev.timestamp < cutoff) continue;
    if (ev.type == EventType::kMemoryWarning) {
      short_mem_warnings++;
    }
  }
  if (short_mem_warnings >= 2) {
    anomalies.push_back({
        {"type", "memory_pressure_spike"},
        {"detail",
         std::to_string(short_mem_warnings) +
             " memory warnings in 10 minutes"},
        {"severity", "warning"}});
  }

  // Frequent network drops
  int short_net_drops = 0;
  for (const auto& ev : events_) {
    if (ev.timestamp < cutoff) continue;
    if (ev.type == EventType::kNetworkChanged &&
        ev.name == "network.disconnected") {
      short_net_drops++;
    }
  }
  if (short_net_drops >= 3) {
    anomalies.push_back({
        {"type", "network_instability"},
        {"detail",
         std::to_string(short_net_drops) +
             " network drops in 10 minutes"},
        {"severity", "warning"}});
  }

  return anomalies;
}

}  // namespace tizenclaw
