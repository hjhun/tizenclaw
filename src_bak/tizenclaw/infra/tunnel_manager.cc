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
#include "tunnel_manager.hh"

#include <fcntl.h>
#include <spawn.h>
#include <sys/wait.h>
#include <unistd.h>

#include <chrono>
#include <csignal>
#include <fstream>
#include <vector>

#include "../../common/logging.hh"
#include "http_client.hh"

extern char** environ;

namespace tizenclaw {

TunnelManager::TunnelManager(const std::string& config_file)
    : config_file_(config_file) {
  LoadConfig();
}

TunnelManager::~TunnelManager() { StopTunnel(); }

bool TunnelManager::LoadConfig() {
  std::ifstream f(config_file_);
  if (!f.is_open()) {
    LOG(WARNING) << "Tunnel config not found: " << config_file_;
    return false;
  }

  try {
    nlohmann::json j;
    f >> j;
    provider_ = j.value("provider", "none");
    auth_token_ = j.value("auth_token", "");
    custom_domain_ = j.value("custom_domain", "");
    LOG(INFO) << "Loaded tunnel config. Provider: " << provider_;
    return true;
  } catch (const std::exception& e) {
    LOG(ERROR) << "Failed to parse tunnel config: " << e.what();
    return false;
  }
}

bool TunnelManager::StartTunnel(int local_port) {
  if (running_) {
    return true;
  }

  local_port_ = local_port;

  if (provider_ != "ngrok") {
    if (provider_ != "none") {
      LOG(WARNING) << "Tunnel provider '" << provider_
                   << "' is not supported yet.";
    }
    return false;
  }

  if (auth_token_.empty()) {
    LOG(WARNING) << "ngrok auth token is empty. Tunnel may not start or have "
                    "rate limits.";
  }

  // Build the ngrok command
  // ngrok http <port> --authtoken <token> --log=stdout
  std::vector<std::string> args = {"ngrok", "http",
                                   std::to_string(local_port_)};

  if (!auth_token_.empty()) {
    args.push_back("--authtoken");
    args.push_back(auth_token_);
  }

  if (!custom_domain_.empty()) {
    args.push_back("--domain");
    args.push_back(custom_domain_);
  }

  args.push_back("--log=stdout");
  args.push_back("--log-format=default");

  std::vector<char*> c_args;
  for (auto& arg : args) {
    c_args.push_back(const_cast<char*>(arg.c_str()));
  }
  c_args.push_back(nullptr);

  pid_t pid;
  posix_spawn_file_actions_t file_actions;
  posix_spawn_file_actions_init(&file_actions);

  // Redirect stdout/stderr to /dev/null to avoid spamming the daemon log
  posix_spawn_file_actions_addopen(&file_actions, STDOUT_FILENO, "/dev/null",
                                   O_WRONLY, 0);
  posix_spawn_file_actions_addopen(&file_actions, STDERR_FILENO, "/dev/null",
                                   O_WRONLY, 0);

  LOG(INFO) << "Starting ngrok tunnel for port " << local_port_;

  if (posix_spawnp(&pid, "ngrok", &file_actions, nullptr, c_args.data(),
                   environ) != 0) {
    LOG(ERROR) << "posix_spawnp failed to spawn ngrok daemon. Is it in PATH?";
    posix_spawn_file_actions_destroy(&file_actions);
    return false;
  }

  posix_spawn_file_actions_destroy(&file_actions);

  tunnel_pid_ = pid;
  running_ = true;

  // Start a monitor thread to fetch the public URL once it's up
  if (monitor_thread_.joinable()) {
    monitor_thread_.join();
  }

  monitor_thread_ = std::thread(&TunnelManager::MonitorTunnel, this);

  return true;
}

void TunnelManager::StopTunnel() {
  if (!running_ || tunnel_pid_ <= 0) {
    return;
  }

  LOG(INFO) << "Stopping ngrok tunnel (PID: " << tunnel_pid_ << ")";

  // Terminate the process gently
  kill(tunnel_pid_, SIGTERM);

  // Wait shortly
  std::this_thread::sleep_for(std::chrono::milliseconds(200));

  // Check if it's still alive, if so forcefully kill it
  int status;
  if (waitpid(tunnel_pid_, &status, WNOHANG) == 0) {
    kill(tunnel_pid_, SIGKILL);
    waitpid(tunnel_pid_, &status, 0);
  }

  tunnel_pid_ = -1;
  running_ = false;
  public_url_ = "";

  if (monitor_thread_.joinable()) {
    monitor_thread_.join();
  }
}

std::string TunnelManager::GetPublicUrl() const { return public_url_; }

void TunnelManager::MonitorTunnel() {
  int retries = 0;
  std::string api_url = "http://127.0.0.1:4040/api/tunnels";

  while (running_ && retries < 15) {  // Try for 15 seconds
    std::this_thread::sleep_for(std::chrono::seconds(1));

    // Check if process died
    int status;
    if (tunnel_pid_ > 0 && waitpid(tunnel_pid_, &status, WNOHANG) > 0) {
      LOG(ERROR) << "ngrok tunnel process died prematurely.";
      running_ = false;
      tunnel_pid_ = -1;
      return;
    }

    HttpResponse resp = HttpClient::Get(api_url);
    if (resp.success && resp.status_code == 200 && !resp.body.empty()) {
      try {
        auto j = nlohmann::json::parse(resp.body);
        if (j.contains("tunnels") && j["tunnels"].is_array() &&
            j["tunnels"].size() > 0) {
          public_url_ = j["tunnels"][0].value("public_url", "");
          if (!public_url_.empty()) {
            LOG(INFO) << "========================================";
            LOG(INFO) << "Secure Tunnel Established!";
            LOG(INFO) << "Public URL: " << public_url_;
            LOG(INFO) << "Routing to: localhost:" << local_port_;
            LOG(INFO) << "========================================";
            return;  // Success
          }
        }
      } catch (...) {
        // Parse error, keep retrying
      }
    }
    retries++;
  }

  if (running_) {
    LOG(WARNING) << "ngrok tunnel started but could not retrieve public URL "
                    "from local API after 15s.";
  }
}

}  // namespace tizenclaw
