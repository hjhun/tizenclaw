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
#include "fleet_agent.hh"

#include <chrono>
#include <fstream>

#include "../../common/logging.hh"

namespace tizenclaw {

FleetAgent::~FleetAgent() { Stop(); }

bool FleetAgent::Initialize(
    const std::string& config_path) {
  std::ifstream f(config_path);
  if (!f.is_open()) {
    LOG(INFO) << "FleetAgent config not found, "
              << "disabled";
    return true;  // Non-fatal
  }

  try {
    nlohmann::json config;
    f >> config;
    enabled_ = config.value("enabled", false);
    fleet_url_ =
        config.value("fleet_server_url", "");
    device_name_ =
        config.value("device_name", "TizenClaw Device");
    device_group_ =
        config.value("device_group", "default");
    heartbeat_interval_sec_ =
        config.value("heartbeat_interval_sec", 60);

    if (enabled_) {
      LOG(INFO) << "FleetAgent enabled: "
                << fleet_url_;
    } else {
      LOG(INFO) << "FleetAgent disabled";
    }
  } catch (const std::exception& e) {
    LOG(WARNING) << "FleetAgent config error: "
                 << e.what();
  }

  return true;
}

void FleetAgent::Start() {
  if (!enabled_ || running_.load()) return;

  running_ = true;

  if (!RegisterDevice()) {
    LOG(WARNING) << "FleetAgent: initial "
                 << "registration failed";
  }

  heartbeat_thread_ =
      std::thread(&FleetAgent::HeartbeatLoop, this);
  LOG(INFO) << "FleetAgent heartbeat started "
            << "(interval: "
            << heartbeat_interval_sec_ << "s)";
}

void FleetAgent::Stop() {
  if (!running_.load()) return;
  running_ = false;
  if (heartbeat_thread_.joinable())
    heartbeat_thread_.join();
  LOG(INFO) << "FleetAgent stopped";
}

nlohmann::json FleetAgent::GetDeviceInfo() const {
  return {
      {"device_name", device_name_},
      {"device_group", device_group_},
      {"fleet_url", fleet_url_},
      {"enabled", enabled_},
      {"heartbeat_interval_sec",
       heartbeat_interval_sec_}};
}

nlohmann::json
FleetAgent::GetHeartbeatStatus() const {
  return {{"running", running_.load()},
          {"last_heartbeat_time",
           last_heartbeat_time_.load()}};
}

bool FleetAgent::RegisterDevice() {
  if (fleet_url_.empty()) {
    LOG(WARNING) << "FleetAgent: no fleet URL "
                 << "configured";
    return false;
  }

  // Stub: would POST device info to fleet server
  LOG(INFO) << "FleetAgent: device registration "
            << "(stub) to " << fleet_url_;
  return true;
}

void FleetAgent::HeartbeatLoop() {
  LOG(INFO) << "FleetAgent: heartbeat loop started";

  while (running_.load()) {
    auto metrics = CollectMetrics();

    // Stub: would POST metrics to fleet server
    auto now =
        std::chrono::duration_cast<
            std::chrono::seconds>(
            std::chrono::system_clock::now()
                .time_since_epoch())
            .count();
    last_heartbeat_time_.store(now);

    LOG(DEBUG) << "FleetAgent: heartbeat sent "
               << "(stub)";

    // Sleep for heartbeat interval
    for (int i = 0;
         i < heartbeat_interval_sec_ &&
         running_.load();
         ++i) {
      std::this_thread::sleep_for(
          std::chrono::seconds(1));
    }
  }
}

nlohmann::json
FleetAgent::CollectMetrics() const {
  // Collect basic system metrics
  return {
      {"device_name", device_name_},
      {"daemon_status", "running"},
      {"daemon_version", "1.0.0"}};
}

}  // namespace tizenclaw
