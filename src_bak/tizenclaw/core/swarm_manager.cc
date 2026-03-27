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

#include "swarm_manager.hh"

#include <arpa/inet.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <sys/socket.h>
#include <unistd.h>
#include <chrono>
#include <cstring>

#include "../../common/logging.hh"

namespace tizenclaw {

namespace {

int64_t GetCurrentMs() {
  return std::chrono::duration_cast<std::chrono::milliseconds>(
             std::chrono::system_clock::now().time_since_epoch())
      .count();
}

std::string ResolveLocalIp() {
  int sock = socket(AF_INET, SOCK_DGRAM, 0);
  if (sock < 0) return "127.0.0.1";

  struct sockaddr_in serv;
  memset(&serv, 0, sizeof(serv));
  serv.sin_family = AF_INET;
  serv.sin_addr.s_addr = inet_addr("8.8.8.8"); // External IP to determine routing interface
  serv.sin_port = htons(53);

  std::string result = "127.0.0.1";
  if (connect(sock, (const struct sockaddr*)&serv, sizeof(serv)) == 0) {
    struct sockaddr_in name;
    socklen_t namelen = sizeof(name);
    if (getsockname(sock, (struct sockaddr*)&name, &namelen) == 0) {
      char buffer[INET_ADDRSTRLEN];
      inet_ntop(AF_INET, &name.sin_addr, buffer, sizeof(buffer));
      result = buffer;
    }
  }
  close(sock);
  return result;
}

} // namespace

SwarmManager::SwarmManager(const SafetyGuard& safety_guard)
    : safety_guard_(safety_guard) {}

SwarmManager::~SwarmManager() {
  Stop();
}

bool SwarmManager::Start() {
  if (running_) return true;

  local_ip_ = ResolveLocalIp();
  LOG(INFO) << "SwarmManager: Local IP resolved to " << local_ip_;

  sock_fd_ = socket(AF_INET, SOCK_DGRAM, 0);
  if (sock_fd_ < 0) {
    LOG(ERROR) << "SwarmManager: Failed to create UDP socket.";
    return false;
  }

  int opt = 1;
  if (setsockopt(sock_fd_, SOL_SOCKET, SO_REUSEADDR, &opt, sizeof(opt)) < 0) {
    LOG(WARNING) << "SwarmManager: setsockopt SO_REUSEADDR failed";
  }

#ifdef SO_REUSEPORT
  if (setsockopt(sock_fd_, SOL_SOCKET, SO_REUSEPORT, &opt, sizeof(opt)) < 0) {
    LOG(WARNING) << "SwarmManager: setsockopt SO_REUSEPORT failed";
  }
#endif

  int broadcast = 1;
  if (setsockopt(sock_fd_, SOL_SOCKET, SO_BROADCAST, &broadcast, sizeof(broadcast)) < 0) {
    LOG(ERROR) << "SwarmManager: setsockopt SO_BROADCAST failed";
    close(sock_fd_);
    return false;
  }

  struct sockaddr_in bind_addr;
  memset(&bind_addr, 0, sizeof(bind_addr));
  bind_addr.sin_family = AF_INET;
  bind_addr.sin_addr.s_addr = htonl(INADDR_ANY);
  bind_addr.sin_port = htons(kSwarmPort);

  if (bind(sock_fd_, (struct sockaddr*)&bind_addr, sizeof(bind_addr)) < 0) {
    LOG(ERROR) << "SwarmManager: Failed to bind UDP socket on port " << kSwarmPort;
    close(sock_fd_);
    return false;
  }

  running_ = true;

  // Subscribe to local EventBus
  event_sub_id_ = EventBus::GetInstance().SubscribeAll(
      [this](const SystemEvent& evt) { OnLocalEvent(evt); });

  // Start background threads
  listener_thread_ = std::thread(&SwarmManager::ListenerLoop, this);
  heartbeat_thread_ = std::thread(&SwarmManager::HeartbeatLoop, this);

  LOG(INFO) << "SwarmManager: UDP Orchestration Network started on port " << kSwarmPort;
  return true;
}

void SwarmManager::Stop() {
  if (!running_) return;
  running_ = false;

  if (event_sub_id_ > 0) {
    EventBus::GetInstance().Unsubscribe(event_sub_id_);
    event_sub_id_ = -1;
  }

  if (sock_fd_ >= 0) {
    // Send leaving heartbeat
    nlohmann::json hj;
    hj["swarm_type"] = "heartbeat";
    hj["status"] = "leaving";
    hj["device_type"] = safety_guard_.GetDeviceProfile().device_type;
    BroadcastJson(hj);

    close(sock_fd_);
    sock_fd_ = -1;
  }

  if (listener_thread_.joinable()) listener_thread_.join();
  if (heartbeat_thread_.joinable()) heartbeat_thread_.join();

  LOG(INFO) << "SwarmManager: Stopped.";
}

void SwarmManager::ListenerLoop() {
  char buffer[8192];
  struct sockaddr_in sender_addr;
  socklen_t sender_len = sizeof(sender_addr);

  // Set timeout so loop can exit check
  struct timeval tv;
  tv.tv_sec = 1;
  tv.tv_usec = 0;
  setsockopt(sock_fd_, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv));

  while (running_) {
    int n = recvfrom(sock_fd_, buffer, sizeof(buffer) - 1, 0,
                     (struct sockaddr*)&sender_addr, &sender_len);
    if (n > 0) {
      buffer[n] = '\0';
      char ip_str[INET_ADDRSTRLEN];
      inet_ntop(AF_INET, &sender_addr.sin_addr, ip_str, INET_ADDRSTRLEN);
      std::string sender_ip = ip_str;

      // Ignore packets from ourselves
      if (sender_ip != local_ip_) {
        HandleIncomingPacket(std::string(buffer, n), sender_ip);
      }
    }
  }
}

void SwarmManager::HeartbeatLoop() {
  while (running_) {
    const auto& profile = safety_guard_.GetDeviceProfile();

    nlohmann::json hj;
    hj["swarm_type"] = "heartbeat";
    hj["status"] = "active";
    hj["device_type"] = profile.device_type;
    hj["capabilities"] = profile.capabilities;

    BroadcastJson(hj);

    RemoveStalePeers();

    int slept = 0;
    while (running_ && slept < kHeartbeatIntervalMs) {
      std::this_thread::sleep_for(std::chrono::milliseconds(100));
      slept += 100;
    }
  }
}

void SwarmManager::OnLocalEvent(const SystemEvent& event) {
  // Prevent broadcasting events we received from peers
  if (event.plugin_id == "remote_peer") return;

  // Format into swarm payload
  nlohmann::json j;
  j["swarm_type"] = "event";
  j["event"] = {
      {"type_id", static_cast<int>(event.type)},
      {"source", event.source},
      {"name", event.name},
      {"data", event.data},
      {"timestamp", event.timestamp},
      // Originate standardizing with local device type context
      {"device_type", safety_guard_.GetDeviceProfile().device_type}};

  BroadcastJson(j);
}

void SwarmManager::BroadcastJson(const nlohmann::json& payload) {
  if (sock_fd_ < 0) return;

  std::string dump = payload.dump();
  struct sockaddr_in dest;
  memset(&dest, 0, sizeof(dest));
  dest.sin_family = AF_INET;
  dest.sin_addr.s_addr = inet_addr("255.255.255.255");
  dest.sin_port = htons(kSwarmPort);

  sendto(sock_fd_, dump.c_str(), dump.length(), 0,
         (struct sockaddr*)&dest, sizeof(dest));
}

void SwarmManager::HandleIncomingPacket(
    const std::string& data, const std::string& sender_ip) {
  try {
    auto j = nlohmann::json::parse(data);
    std::string swarm_type = j.value("swarm_type", "");

    if (swarm_type == "heartbeat") {
      UpdatePeer(sender_ip, j);
    } else if (swarm_type == "event") {
      if (j.contains("event")) {
        auto ej = j["event"];
        SystemEvent e;
        e.type = static_cast<EventType>(ej.value("type_id", 0));
        e.source = ej.value("source", "unknown");
        e.name = ej.value("name", "unknown");
        if (ej.contains("data")) e.data = ej["data"];
        e.timestamp = ej.value("timestamp", GetCurrentMs());
        
        // Mark as remote peer so we don't re-broadcast
        e.plugin_id = "remote_peer";

        // Inject original context into the event name/data if not already there
        std::string remote_device = ej.value("device_type", "unknown");
        e.source = remote_device + "::" + e.source;

        EventBus::GetInstance().Publish(std::move(e));
        LOG(INFO) << "SwarmManager: Forwarded remote event > " << e.name << " from " << sender_ip;
      }
    }
  } catch (const nlohmann::json::parse_error&) {
    LOG(WARNING) << "SwarmManager: Invalid JSON received from " << sender_ip;
  }
}

void SwarmManager::UpdatePeer(
    const std::string& ip, const nlohmann::json& peer_info) {
  std::lock_guard<std::mutex> lock(peers_mutex_);
  
  std::string status = peer_info.value("status", "active");
  if (status == "leaving") {
    peers_.erase(ip);
    LOG(INFO) << "SwarmManager: Peer left > " << ip;
    return;
  }

  bool is_new = false;
  auto it = peers_.find(ip);
  if (it == peers_.end()) {
    is_new = true;
    SwarmPeer p;
    p.ip_address = ip;
    peers_[ip] = std::move(p);
    it = peers_.find(ip);
  }

  it->second.last_seen_ms = GetCurrentMs();
  if (peer_info.contains("device_type")) {
    it->second.device_type = peer_info["device_type"].get<std::string>();
  }
  if (peer_info.contains("capabilities") && peer_info["capabilities"].is_array()) {
    it->second.capabilities.clear();
    for (auto& c : peer_info["capabilities"]) {
      it->second.capabilities.push_back(c.get<std::string>());
    }
  }

  if (is_new) {
    LOG(INFO) << "SwarmManager: New peer discovered > " 
              << it->second.device_type << " at " << ip;
  }
}

void SwarmManager::RemoveStalePeers() {
  int64_t now = GetCurrentMs();
  int64_t cutoff = now - kPeerTimeoutMs;

  std::lock_guard<std::mutex> lock(peers_mutex_);
  for (auto it = peers_.begin(); it != peers_.end();) {
    if (it->second.last_seen_ms < cutoff) {
      LOG(INFO) << "SwarmManager: Peer timed out > " 
                << it->second.device_type << " at " << it->first;
      it = peers_.erase(it);
    } else {
      ++it;
    }
  }
}

std::vector<SwarmPeer> SwarmManager::GetPeers() const {
  std::vector<SwarmPeer> result;
  std::lock_guard<std::mutex> lock(peers_mutex_);
  for (const auto& [ip, peer] : peers_) {
    result.push_back(peer);
  }
  return result;
}

nlohmann::json SwarmManager::GetStatusJson() const {
  nlohmann::json j;
  j["active_peers"] = nlohmann::json::array();
  
  std::lock_guard<std::mutex> lock(peers_mutex_);
  for (const auto& [ip, peer] : peers_) {
    nlohmann::json pj;
    pj["ip"] = peer.ip_address;
    pj["device_type"] = peer.device_type;
    pj["capabilities"] = peer.capabilities;
    j["active_peers"].push_back(std::move(pj));
  }
  return j;
}

}  // namespace tizenclaw
