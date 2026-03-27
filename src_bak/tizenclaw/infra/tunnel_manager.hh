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
#ifndef TUNNEL_MANAGER_HH
#define TUNNEL_MANAGER_HH

#include <atomic>
#include <nlohmann/json.hpp>
#include <string>
#include <thread>

namespace tizenclaw {

class TunnelManager {
 public:
  TunnelManager(const std::string& config_file);
  ~TunnelManager();

  bool LoadConfig();
  bool StartTunnel(int local_port);
  void StopTunnel();
  std::string GetPublicUrl() const;
  bool IsRunning() const { return running_; }

 private:
  void MonitorTunnel();
  std::string FetchNgrokUrl() const;

  std::string config_file_;
  std::string provider_ = "none";
  std::string auth_token_;
  std::string custom_domain_;

  int local_port_ = 9090;
  std::string public_url_;

  std::atomic<bool> running_{false};
  pid_t tunnel_pid_ = -1;
  std::thread monitor_thread_;
};

}  // namespace tizenclaw

#endif  // TUNNEL_MANAGER_HH
