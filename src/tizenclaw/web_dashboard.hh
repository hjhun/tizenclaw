#ifndef __WEB_DASHBOARD_H__
#define __WEB_DASHBOARD_H__

#include <string>
#include <thread>
#include <atomic>
#include <set>
#include <vector>
#include <mutex>
#include <libsoup/soup.h>

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
  WebDashboard(AgentCore* agent,
               TaskScheduler* scheduler);
  ~WebDashboard();

  // Channel interface
  std::string GetName() const override {
    return "web_dashboard";
  }
  bool Start() override;
  void Stop() override;
  bool IsRunning() const override {
    return running_;
  }

private:
  // Load dashboard config
  bool LoadConfig();

  // libsoup request handler
  static void HandleRequest(
      SoupServer* server,
      SoupMessage* msg,
      const char* path,
      GHashTable* query,
      SoupClientContext* client,
      gpointer user_data);

  // Route API requests
  void HandleApi(
      SoupMessage* msg,
      const std::string& path) const;

  // Serve static files (HTML/CSS/JS)
  void ServeStaticFile(
      SoupMessage* msg,
      const std::string& path) const;

  // API endpoint handlers
  void ApiSessions(SoupMessage* msg) const;
  void ApiTasks(SoupMessage* msg) const;
  void ApiLogs(SoupMessage* msg) const;
  void ApiChat(SoupMessage* msg) const;
  void ApiStatus(SoupMessage* msg) const;

  // Auth endpoints
  void ApiAuthLogin(SoupMessage* msg);
  void ApiAuthChangePassword(SoupMessage* msg);
  bool ValidateToken(
      SoupMessage* msg) const;
  std::string HashPassword(
      const std::string& pw) const;
  std::string GenerateToken() const;
  void LoadAdminPassword();
  void SaveAdminPassword();

  // Config endpoints
  void ApiConfigList(SoupMessage* msg) const;
  void ApiConfigGet(
      SoupMessage* msg,
      const std::string& name) const;
  void ApiConfigSet(
      SoupMessage* msg,
      const std::string& name);
  bool IsAllowedConfig(
      const std::string& name) const;
  std::string ConfigFilePath(
      const std::string& name) const;
  std::string SampleFilePath(
      const std::string& name) const;

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
  static const std::vector<std::string>
      kAllowedConfigs;
};

}  // namespace tizenclaw

#endif  // __WEB_DASHBOARD_H__
