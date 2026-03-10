#ifndef TIZENCLAW_INFRA_TUNNEL_MANAGER_H_
#define TIZENCLAW_INFRA_TUNNEL_MANAGER_H_

#include <string>
#include <atomic>
#include <thread>
#include <nlohmann/json.hpp>

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

#endif  // TIZENCLAW_INFRA_TUNNEL_MANAGER_H_
