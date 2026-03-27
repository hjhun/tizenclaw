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
#ifndef FLEET_AGENT_HH_
#define FLEET_AGENT_HH_

#include <json.hpp>
#include <atomic>
#include <string>
#include <thread>

namespace tizenclaw {

// Fleet management agent for enterprise device
// management. Handles device registration,
// periodic heartbeat, and remote command execution.
// Disabled by default via fleet_config.json.
class FleetAgent {
 public:
  FleetAgent() = default;
  ~FleetAgent();

  // Initialize from config file
  [[nodiscard]] bool Initialize(
      const std::string& config_path);

  // Start heartbeat thread
  void Start();

  // Stop heartbeat thread
  void Stop();

  // Check if fleet management is enabled
  [[nodiscard]] bool IsEnabled() const {
    return enabled_;
  }

  // Get device registration info
  [[nodiscard]] nlohmann::json
  GetDeviceInfo() const;

  // Get last heartbeat status
  [[nodiscard]] nlohmann::json
  GetHeartbeatStatus() const;

 private:
  // Register device with fleet server
  [[nodiscard]] bool RegisterDevice();

  // Heartbeat loop (runs in separate thread)
  void HeartbeatLoop();

  // Collect device metrics
  [[nodiscard]] nlohmann::json
  CollectMetrics() const;

  bool enabled_ = false;
  std::string fleet_url_;
  std::string device_name_;
  std::string device_group_;
  int heartbeat_interval_sec_ = 60;

  std::atomic<bool> running_{false};
  std::thread heartbeat_thread_;
  std::atomic<int64_t> last_heartbeat_time_{0};
};

}  // namespace tizenclaw

#endif  // FLEET_AGENT_HH_
