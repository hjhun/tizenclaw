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
#include "system_context_provider.hh"

#include <chrono>
#include <ctime>
#include <iomanip>
#include <sstream>

#include "../../common/logging.hh"

namespace tizenclaw {

SystemContextProvider::SystemContextProvider() {
  // Initialize with defaults
  device_state_ = {
      {"model", "unknown"},
      {"display", "unknown"},
      {"wifi", "unknown"},
      {"bluetooth", "unknown"},
      {"usb", "unknown"},
      {"location", "unknown"},
      {"language", "unknown"},
      {"silent_mode", "unknown"}};
  runtime_state_ = {
      {"network", "unknown"},
      {"battery", {{"level", -1}, {"charging", false}}},
      {"memory_usage", "unknown"},
      {"power_mode", "normal"}};
  app_state_ = nlohmann::json::object();
}

SystemContextProvider::~SystemContextProvider() {
  Stop();
}

void SystemContextProvider::Start() {
  if (started_) return;

  subscription_id_ =
      EventBus::GetInstance().SubscribeAll(
          [this](const SystemEvent& event) {
            OnEvent(event);
          });

  started_ = true;
  LOG(INFO) << "SystemContextProvider started";
}

void SystemContextProvider::Stop() {
  if (!started_) return;

  if (subscription_id_ >= 0) {
    EventBus::GetInstance().Unsubscribe(
        subscription_id_);
    subscription_id_ = -1;
  }

  started_ = false;
  LOG(INFO) << "SystemContextProvider stopped";
}

void SystemContextProvider::OnEvent(
    const SystemEvent& event) {
  switch (event.type) {
    case EventType::kDisplayChanged:
    case EventType::kAppLifecycle:
    case EventType::kBluetoothChanged:
    case EventType::kUsbChanged:
    case EventType::kLocationChanged:
    case EventType::kSystemSetting:
      UpdateDeviceState(event);
      break;
    case EventType::kNetworkChanged:
    case EventType::kBatteryChanged:
    case EventType::kMemoryWarning:
      UpdateRuntimeState(event);
      break;
    case EventType::kRecentApp:
      UpdateAppState(event);
      break;
    default:
      break;
  }

  // All events go to recent list
  AddRecentEvent(event);
}

void SystemContextProvider::UpdateDeviceState(
    const SystemEvent& event) {
  std::lock_guard<std::mutex> lock(state_mutex_);

  if (event.type == EventType::kDisplayChanged) {
    if (event.data.contains("value")) {
      device_state_["display"] =
          event.data["value"];
    } else if (event.data.contains("state")) {
      device_state_["display"] =
          event.data["state"];
    }
  } else if (event.type ==
             EventType::kBluetoothChanged) {
    if (event.data.contains("value"))
      device_state_["bluetooth"] =
          event.data["value"];
  } else if (event.type ==
             EventType::kUsbChanged) {
    if (event.data.contains("value"))
      device_state_["usb"] =
          event.data["value"];
  } else if (event.type ==
             EventType::kLocationChanged) {
    if (event.data.contains("value"))
      device_state_["location"] =
          event.data["value"];
  } else if (event.type ==
             EventType::kSystemSetting) {
    if (event.name == "system.language" &&
        event.data.contains("value"))
      device_state_["language"] =
          event.data["value"];
    if (event.name == "system.silent_mode" &&
        event.data.contains("value"))
      device_state_["silent_mode"] =
          event.data["value"];
  } else if (event.type ==
             EventType::kAppLifecycle) {
    if (event.data.contains("app_id") &&
        event.data.contains("state")) {
      app_state_["foreground_app"] =
          event.data["app_id"];
      app_state_["foreground_state"] =
          event.data["state"];
    }
  }
}

void SystemContextProvider::UpdateAppState(
    const SystemEvent& event) {
  std::lock_guard<std::mutex> lock(state_mutex_);

  if (event.data.contains("recent_apps"))
    app_state_["recent_apps"] =
        event.data["recent_apps"];
}

void SystemContextProvider::UpdateRuntimeState(
    const SystemEvent& event) {
  std::lock_guard<std::mutex> lock(state_mutex_);

  if (event.type == EventType::kNetworkChanged) {
    if (event.data.contains("status")) {
      runtime_state_["network"] =
          event.data["status"].get<std::string>();
    }
    if (event.data.contains("type")) {
      runtime_state_["network_type"] =
          event.data["type"].get<std::string>();
    }
  } else if (event.type == EventType::kBatteryChanged) {
    if (event.data.contains("level")) {
      runtime_state_["battery"]["level"] =
          event.data["level"].get<int>();
    }
    if (event.data.contains("charging")) {
      runtime_state_["battery"]["charging"] =
          event.data["charging"].get<bool>();
    }
  } else if (event.type == EventType::kMemoryWarning) {
    if (event.data.contains("level")) {
      runtime_state_["memory_warning"] =
          event.data["level"].get<std::string>();
    }
  }
}

void SystemContextProvider::AddRecentEvent(
    const SystemEvent& event) {
  // Convert timestamp to HH:MM:SS
  auto tp = std::chrono::system_clock::time_point(
      std::chrono::milliseconds(event.timestamp));
  auto tt = std::chrono::system_clock::to_time_t(tp);
  std::tm tm{};
  localtime_r(&tt, &tm);

  std::ostringstream ts;
  ts << std::setfill('0') << std::setw(2) << tm.tm_hour
     << ":" << std::setw(2) << tm.tm_min
     << ":" << std::setw(2) << tm.tm_sec;

  // Build detail string from event data
  std::string detail;
  if (!event.data.empty()) {
    try {
      detail = event.data.dump();
      // Truncate long details
      if (detail.size() > 100) {
        detail = detail.substr(0, 97) + "...";
      }
    } catch (...) {
      detail = "{}";
    }
  }

  RecentEvent re;
  re.time = ts.str();
  re.source = event.source;
  re.event_name = event.name;
  re.detail = detail;

  std::lock_guard<std::mutex> lock(state_mutex_);
  recent_events_.push_back(std::move(re));
  if (recent_events_.size() > kMaxRecentEvents) {
    recent_events_.erase(recent_events_.begin());
  }
}

nlohmann::json SystemContextProvider::GetContextJson() const {
  nlohmann::json ctx;

  // Collect internal state under state_mutex_
  {
    std::lock_guard<std::mutex> lock(state_mutex_);
    ctx["device"] = device_state_;
    ctx["runtime"] = runtime_state_;
    if (!app_state_.empty())
      ctx["apps"] = app_state_;

    // Recent events
    auto events = nlohmann::json::array();
    for (const auto& re : recent_events_) {
      events.push_back({
          {"time", re.time},
          {"source", re.source},
          {"event", re.event_name},
          {"detail", re.detail}});
    }
    ctx["recent_events"] = events;

    if (!perception_insight_.empty())
      ctx["perception"] = perception_insight_;
  }

  // Query EventBus OUTSIDE state_mutex_ to avoid
  // nested lock ordering issues (Issue #5).
  auto sources =
      EventBus::GetInstance().ListEventSources();
  auto plugins = nlohmann::json::array();
  for (const auto& s : sources) {
    if (s.plugin_id != "builtin") {
      plugins.push_back(s.name);
    }
  }
  ctx["active_plugins"] = plugins;

  return ctx;
}

void SystemContextProvider::SetPerceptionInsight(
    const nlohmann::json& insight) {
  std::lock_guard<std::mutex> lock(state_mutex_);
  perception_insight_ = insight;
}

std::string SystemContextProvider::GetContextString() const {
  auto ctx = GetContextJson();
  if (ctx.empty()) return "";

  try {
    return ctx.dump(2);  // Pretty-print with 2-space indent
  } catch (...) {
    return "{}";
  }
}

}  // namespace tizenclaw
