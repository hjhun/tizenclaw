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
#ifndef WEB_DASHBOARD_HH
#define WEB_DASHBOARD_HH

#include <libsoup/soup.h>

#include <atomic>
#include <mutex>
#include <set>
#include <string>
#include <thread>
#include <vector>

#include "../infra/health_monitor.hh"
#include "../infra/ota_updater.hh"
#include "../infra/tunnel_manager.hh"
#include "a2a_handler.hh"
#include "channel.hh"

namespace tizenclaw {

class AgentCore;
class TaskScheduler;

// Web UI Dashboard channel.
// Serves a lightweight HTML+JS dashboard via
// libsoup HTTP server for monitoring and
// interacting with TizenClaw.
class WebDashboard : public Channel {
 public:
  WebDashboard(AgentCore* agent, TaskScheduler* scheduler);
  ~WebDashboard();

  // Channel interface
  std::string GetName() const override { return "web_dashboard"; }
  bool Start() override;
  void Stop() override;
  bool IsRunning() const override { return running_; }

 private:
  // Load dashboard config
  bool LoadConfig();

  // libsoup request handler
  static void HandleRequest(SoupServer* server, SoupMessage* msg,
                            const char* path, GHashTable* query,
                            SoupClientContext* client,
                            gpointer user_data);

  // Route API requests
  void HandleApi(SoupMessage* msg, const std::string& path,
                 GHashTable* query) const;

  // Serve static files (HTML/CSS/JS)
  void ServeStaticFile(SoupMessage* msg, const std::string& path) const;

  // API endpoint handlers
  void ApiSessions(SoupMessage* msg) const;
  void ApiSessionDetail(SoupMessage* msg,
                        const std::string& id) const;
  void ApiSessionDates(SoupMessage* msg) const;
  void ApiTasks(SoupMessage* msg) const;
  void ApiTaskDetail(SoupMessage* msg,
                     const std::string& file) const;
  void ApiTaskDates(SoupMessage* msg) const;
  void ApiLogs(SoupMessage* msg,
               const std::string& date) const;
  void ApiLogDates(SoupMessage* msg) const;
  void ApiChat(SoupMessage* msg) const;
  void ApiStatus(SoupMessage* msg) const;

  // A2A endpoints
  void ApiAgentCard(SoupMessage* msg) const;
  void ApiA2A(SoupMessage* msg);

  // Health metrics endpoint
  void ApiMetrics(SoupMessage* msg) const;

  // OTA update endpoints
  void ApiOtaCheck(SoupMessage* msg) const;
  void ApiOtaUpdate(SoupMessage* msg);
  void ApiOtaRollback(SoupMessage* msg);

  // Auth endpoints
  void ApiAuthLogin(SoupMessage* msg);
  void ApiAuthChangePassword(SoupMessage* msg);
  bool ValidateToken(SoupMessage* msg) const;
  std::string HashPassword(const std::string& pw) const;
  std::string GenerateToken() const;
  void LoadAdminPassword();
  void SaveAdminPassword();

  // Config endpoints
  void ApiConfigList(SoupMessage* msg) const;
  void ApiConfigGet(SoupMessage* msg, const std::string& name) const;
  void ApiConfigSet(SoupMessage* msg, const std::string& name);
  bool IsAllowedConfig(const std::string& name) const;
  std::string ConfigFilePath(const std::string& name) const;
  std::string SampleFilePath(const std::string& name) const;

  // Dynamic web app endpoints
  void ApiAppsList(SoupMessage* msg) const;
  void ApiAppDetail(SoupMessage* msg,
                    const std::string& app_id) const;
  void ApiAppDelete(SoupMessage* msg,
                    const std::string& app_id);
  void ServeAppFile(SoupMessage* msg,
                    const std::string& path) const;

  AgentCore* agent_;
  TaskScheduler* scheduler_;
  SoupServer* server_ = nullptr;
  std::thread server_thread_;
  GMainLoop* loop_ = nullptr;
  std::atomic<bool> running_{false};

  // Configuration
  int port_ = 9090;
  std::string web_root_;
  std::string config_dir_;

  // Auth state
  std::string admin_password_hash_;
  std::string admin_pw_file_;
  mutable std::mutex tokens_mutex_;
  std::set<std::string> active_tokens_;

  // Allowed config names
  static const std::vector<std::string> kAllowedConfigs;

  // A2A handler
  std::unique_ptr<A2AHandler> a2a_handler_;

  // Health monitor
  std::unique_ptr<HealthMonitor> health_monitor_;

  // OTA updater
  std::unique_ptr<OtaUpdater> ota_updater_;

  // Tunnel manager
  std::unique_ptr<TunnelManager> tunnel_manager_;
};

}  // namespace tizenclaw

#endif  // WEB_DASHBOARD_HH
