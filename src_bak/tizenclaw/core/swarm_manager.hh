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
#ifndef SWARM_MANAGER_HH_
#define SWARM_MANAGER_HH_

#include <atomic>
#include <json.hpp>
#include <map>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

#include "event_bus.hh"
#include "safety_guard.hh"

namespace tizenclaw {

// Represents a discovered TizenClaw peer in the home network
struct SwarmPeer {
  std::string ip_address;
  std::string device_type;   // "tv", "refrigerator", "oven"
  int64_t last_seen_ms;      // epoch ms for heartbeat tracking
  std::vector<std::string> capabilities;
};

// SwarmManager enables Multi-Device Orchestration (Phase 3).
// It discovers other TizenClaw agents on the local network via
// UDP broadcast and establishes a Cross-Device Event Bus.
// Local events are broadcasted to the swarm, and remote events
// are injected into the local EventBus.
class SwarmManager {
 public:
  explicit SwarmManager(const SafetyGuard& safety_guard);
  ~SwarmManager();

  // Start the UDP listener, broadcaster, and local event subscription
  bool Start();

  // Stop the swarm networking
  void Stop();

  // Get active peers
  [[nodiscard]] std::vector<SwarmPeer> GetPeers() const;

  // Get status as JSON for WebDashboard
  [[nodiscard]] nlohmann::json GetStatusJson() const;

 private:
  // Internal thread loops
  void ListenerLoop();
  void HeartbeatLoop();

  // Local EventBus callback
  void OnLocalEvent(const SystemEvent& event);

  // Networking helpers
  void BroadcastJson(const nlohmann::json& payload);
  void HandleIncomingPacket(const std::string& data, const std::string& sender_ip);

  // Peer management
  void UpdatePeer(const std::string& ip, const nlohmann::json& peer_info);
  void RemoveStalePeers();

  const SafetyGuard& safety_guard_;
  
  std::map<std::string, SwarmPeer> peers_;
  mutable std::mutex peers_mutex_;

  std::thread listener_thread_;
  std::thread heartbeat_thread_;
  std::atomic<bool> running_{false};

  int sock_fd_ = -1;
  int event_sub_id_ = -1;
  std::string local_ip_; // Resolved local IP to ignore self-broadcasts

  static constexpr int kSwarmPort = 39888;
  static constexpr int kHeartbeatIntervalMs = 5000;
  static constexpr int kPeerTimeoutMs = 15000;
};

}  // namespace tizenclaw

#endif  // SWARM_MANAGER_HH_
