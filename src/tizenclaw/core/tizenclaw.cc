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
#include "tizenclaw.hh"

#include <arpa/inet.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

#include <algorithm>
#include <csignal>
#include <exception>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <nlohmann/json.hpp>
#include <ranges>
#include <string>
#include <vector>

#include "../infra/key_store.hh"
#include "../llm/plugin_manager.hh"
#include "../storage/audit_logger.hh"

namespace tizenclaw {

TizenClawDaemon* g_daemon = nullptr;

void signal_handler(int sig) {
  LOG(INFO) << "Caught signal " << sig;
  if (g_daemon) {
    g_daemon->Quit();
  }
}

static bool IsChannelEnabled(const std::string& config_file,
                             const std::vector<std::string>& required_keys) {
  std::string path = "/opt/usr/share/tizenclaw/config/" + config_file;
  std::ifstream f(path);
  if (!f.is_open()) return false;
  try {
    nlohmann::json j;
    f >> j;

    if (config_file == "webhook_config.json") {
      return j.contains("routes") && j["routes"].is_array() &&
             !j["routes"].empty();
    }

    for (const auto& key : required_keys) {
      if (!j.contains(key)) return false;
      if (j[key].is_string()) {
        std::string val = j[key].get<std::string>();
        if (val.empty() || val.find("YOUR_") != std::string::npos) {
          return false;
        }
      }
    }
    return true;
  } catch (...) {
    return false;
  }
}

TizenClawDaemon::TizenClawDaemon(int argc, char** argv)
    : argc_(argc), argv_(argv) {
  tizen_core_init();
  tizen_core_task_create("main", false, &task_);
}

TizenClawDaemon::~TizenClawDaemon() {
  if (task_) {
    tizen_core_task_destroy(task_);
    task_ = nullptr;
  }
  tizen_core_shutdown();
}

int TizenClawDaemon::Run() {
  LOG(INFO) << "TizenClaw Daemon Run";
  OnCreate();

  // Set up signal handling
  std::signal(SIGINT, signal_handler);
  std::signal(SIGTERM, signal_handler);

  int ret = tizen_core_task_run(task_);
  if (ret != 0) {
    // Fallback: tizen_core event loop failed
    // (e.g., no D-Bus in chroot environment).
    // Keep running since IPC and channels
    // operate in their own threads.
    LOG(WARNING) << "tizen_core_task_run " << "returned " << ret
                 << ", using fallback loop";
    while (ipc_running_) {
      std::this_thread::sleep_for(std::chrono::seconds(1));
    }
  }
  OnDestroy();
  return 0;
}

void TizenClawDaemon::Quit() {
  LOG(INFO) << "TizenClaw Daemon Quit";
  if (task_) {
    tizen_core_task_quit(task_);
  }
}

void TizenClawDaemon::OnCreate() {
  LOG(INFO) << "TizenClaw Daemon OnCreate";

  // Initialize Plugin Manager before AgentCore
  // so AgentCore can find installed plugins during backend creation
  PluginManager::GetInstance().Initialize();

  agent_ = std::make_unique<AgentCore>();
  if (!agent_->Initialize()) {
    LOG(ERROR) << "Failed to initialize AgentCore";
  }

  // Start EventBus and SystemEventCollector
  EventBus::GetInstance().Start();
  std::string events_dir =
      std::string(APP_DATA_DIR) + "/tools/events";
  EventBus::GetInstance().LoadPlugins(events_dir);
  event_collector_ = std::make_unique<SystemEventCollector>();
  event_collector_->Start();

  // Initialize AutonomousTrigger
  auto_trigger_ = std::make_unique<AutonomousTrigger>(
      agent_.get(), agent_->GetSystemContext());
  std::string trigger_config =
      std::string(APP_DATA_DIR)
      + "/config/autonomous_trigger.json";
  auto_trigger_->LoadRules(trigger_config);
  auto_trigger_->Start();

  // Initialize Task Scheduler
  scheduler_ = std::make_unique<TaskScheduler>();
  agent_->SetScheduler(scheduler_.get());
  scheduler_->Start(agent_.get());

  // Register channels
  auto* a = agent_.get();
  channel_registry_.Register(std::make_unique<McpServer>(a));

  if (IsChannelEnabled("telegram_config.json", {"bot_token"})) {
    LOG(INFO) << "Telegram configured, registering channel";
    channel_registry_.Register(std::make_unique<TelegramClient>(a));
  }
  if (IsChannelEnabled("webhook_config.json", {})) {
    LOG(INFO) << "Webhook configured, registering channel";
    channel_registry_.Register(std::make_unique<WebhookChannel>(a));
  }
  if (IsChannelEnabled("slack_config.json", {"app_token", "bot_token"})) {
    LOG(INFO) << "Slack configured, registering channel";
    channel_registry_.Register(std::make_unique<SlackChannel>(a));
  }
  if (IsChannelEnabled("discord_config.json", {"bot_token"})) {
    LOG(INFO) << "Discord configured, registering channel";
    channel_registry_.Register(std::make_unique<DiscordChannel>(a));
  }

  channel_registry_.Register(
      std::make_unique<WebDashboard>(a, scheduler_.get()));
  channel_registry_.Register(std::make_unique<VoiceChannel>(a));
  channel_registry_.StartAll();

  ipc_running_ = true;
  ipc_thread_ = std::thread(&TizenClawDaemon::IpcServerLoop, this);

  // Start Skill Watcher (inotify)
  skill_watcher_.Start("/opt/usr/share/tizenclaw/tools/skills", [this]() {
    if (agent_) {
      agent_->ReloadSkills();
    }
  });

  // Initialize Skill Repository
  skill_repo_ = std::make_unique<SkillRepository>();
  std::string skill_repo_config =
      std::string(APP_DATA_DIR)
      + "/config/skill_repo.json";
  skill_repo_->Initialize(skill_repo_config);
  LOG(INFO) << "SkillRepository initialized"
            << (skill_repo_->IsEnabled()
                    ? " (enabled)" : " (disabled)");

  // Initialize Fleet Agent
  fleet_agent_ = std::make_unique<FleetAgent>();
  std::string fleet_config =
      std::string(APP_DATA_DIR)
      + "/config/fleet_config.json";
  fleet_agent_->Initialize(fleet_config);
  if (fleet_agent_->IsEnabled())
    fleet_agent_->Start();
  LOG(INFO) << "FleetAgent initialized"
            << (fleet_agent_->IsEnabled()
                    ? " (enabled)" : " (disabled)");
}

void TizenClawDaemon::OnDestroy() {
  LOG(INFO) << "TizenClaw Daemon OnDestroy";

  // Stop Plugin Manager
  PluginManager::GetInstance().Shutdown();

  // Stop Fleet Agent
  if (fleet_agent_) fleet_agent_->Stop();

  // Stop AutonomousTrigger, EventBus and Collector
  if (auto_trigger_) auto_trigger_->Stop();
  if (event_collector_) event_collector_->Stop();
  EventBus::GetInstance().Stop();

  // Stop Skill Watcher
  skill_watcher_.Stop();

  // Stop all channels
  channel_registry_.StopAll();

  ipc_running_ = false;
  if (ipc_socket_ != -1) {
    shutdown(ipc_socket_, SHUT_RDWR);
    close(ipc_socket_);
    ipc_socket_ = -1;
  }
  if (ipc_thread_.joinable()) {
    ipc_thread_.join();
  }

  // Wait for all active client threads
  {
    std::lock_guard<std::mutex> lock(threads_mutex_);
    for (auto& t : client_threads_) {
      if (t.joinable()) {
        t.join();
      }
    }
    client_threads_.clear();
  }

  // Stop Task Scheduler (before AgentCore)
  if (scheduler_) {
    scheduler_->Stop();
  }

  if (agent_) {
    agent_->SetScheduler(nullptr);
    agent_->Shutdown();
  }

  // unique_ptr releases in reverse order
  scheduler_.reset();
  agent_.reset();
}

void TizenClawDaemon::IpcServerLoop() {
  LOG(INFO) << "IPC Server thread starting...";

  int sock = socket(AF_UNIX, SOCK_STREAM, 0);
  if (sock < 0) {
    LOG(ERROR) << "Failed to create IPC socket: " << strerror(errno);
    return;
  }
  ipc_socket_ = sock;

  struct sockaddr_un addr = {};
  addr.sun_family = AF_UNIX;

  // Abstract namespace socket: "\0tizenclaw.sock"
  const char kSocketName[] = "tizenclaw.sock";
  constexpr size_t kNameLen = 1 + sizeof(kSocketName) - 1;
  // Copy into sun_path at offset 1 (abstract namespace).
  // Use a loop instead of memcpy(sun_path+1,...) to avoid
  // GCC -Wstringop-overread false positive on ARM32.
  for (size_t i = 0; i < sizeof(kSocketName) - 1; ++i)
    addr.sun_path[1 + i] = kSocketName[i];

  socklen_t addr_len = offsetof(struct sockaddr_un, sun_path) + kNameLen;

  if (bind(ipc_socket_, (struct sockaddr*)&addr, addr_len) < 0) {
    LOG(ERROR) << "Failed to bind IPC socket: " << strerror(errno);
    close(ipc_socket_);
    ipc_socket_ = -1;
    return;
  }

  if (listen(ipc_socket_, 5) < 0) {
    LOG(ERROR) << "Failed to listen IPC socket: " << strerror(errno);
    close(ipc_socket_);
    ipc_socket_ = -1;
    return;
  }

  LOG(INFO) << "IPC Server listening on "
            << "\\0tizenclaw.sock (addr_len=" << addr_len << ")";

  while (ipc_running_) {
    int client_sock = accept(ipc_socket_, nullptr, nullptr);
    if (client_sock < 0) {
      if (ipc_running_) {
        LOG(WARNING) << "accept() failed: " << strerror(errno);
      }
      continue;
    }

    LOG(INFO) << "IPC client connected";

    // --- Peer credential verification ---
    struct ucred cred;
    socklen_t cred_len = sizeof(cred);
    if (getsockopt(client_sock, SOL_SOCKET, SO_PEERCRED, &cred, &cred_len) <
        0) {
      LOG(ERROR) << "Failed to get peer cred: " << strerror(errno);
      close(client_sock);
      continue;
    }

    if (!IsAllowedUid(cred.uid)) {
      LOG(WARNING) << "Rejected IPC from uid=" << cred.uid
                   << " pid=" << cred.pid;
      AuditLogger::Instance().Log(
          AuditLogger::MakeEvent(AuditEventType::kIpcAuth, "",
                                 {{"uid", static_cast<int>(cred.uid)},
                                  {"pid", static_cast<int>(cred.pid)},
                                  {"allowed", false}}));
      close(client_sock);
      continue;
    }

    LOG(INFO) << "Authorized IPC from pid=" << cred.pid << " uid=" << cred.uid;
    AuditLogger::Instance().Log(
        AuditLogger::MakeEvent(AuditEventType::kIpcAuth, "",
                               {{"uid", static_cast<int>(cred.uid)},
                                {"pid", static_cast<int>(cred.pid)},
                                {"allowed", true}}));

    // Check concurrent client limit
    if (active_clients_.load() >= kMaxConcurrentClients) {
      LOG(WARNING) << "Max concurrent clients " << "reached ("
                   << kMaxConcurrentClients << "), rejecting";
      nlohmann::json busy = {{"type", "response"},
                             {"status", "error"},
                             {"text", "Server busy, try again later"}};
      std::string busy_str = busy.dump();
      uint32_t busy_len = htonl(busy_str.size());
      if (::write(client_sock, &busy_len, 4) == 4) {
        ssize_t wr = ::write(client_sock, busy_str.data(), busy_str.size());
        (void)wr;
      }
      close(client_sock);
      continue;
    }

    // Spawn tracked thread to handle this client
    {
      std::lock_guard<std::mutex> lock(threads_mutex_);
      // Clean up finished threads
      std::erase_if(client_threads_,
                    [](std::thread& t) { return !t.joinable(); });

      client_threads_.emplace_back([this, client_sock]() {
        active_clients_.fetch_add(1);
        HandleIpcClient(client_sock);
        active_clients_.fetch_sub(1);
      });
      client_threads_.back().detach();
    }
  }
}

void TizenClawDaemon::HandleIpcClient(int client_sock) {
  while (true) {
    // Read 4-byte length prefix
    uint32_t net_len = 0;
    ssize_t hdr_read = ::recv(client_sock, &net_len, 4, MSG_WAITALL);

    std::string raw_msg;

    if (hdr_read == 4) {
      // New protocol: Length prefixed
      uint32_t len = ntohl(net_len);
      if (len > 10 * 1024 * 1024) {  // 10MB limit
        LOG(ERROR) << "IPC Payload too large: " << len;
        break;
      }

      std::vector<char> buffer(len);
      ssize_t body_read = ::recv(client_sock, buffer.data(), len, MSG_WAITALL);
      if (body_read != static_cast<ssize_t>(len)) {
        LOG(ERROR) << "Incomplete IPC payload read";
        break;
      }
      raw_msg.assign(buffer.data(), len);
    } else if (hdr_read > 0) {
      // Fallback: Legacy EOF-based protocol
      // We read 1-3 bytes into net_len by accident, append it.
      // Copy via char array to avoid GCC -Wstringop-overread
      // on reinterpret_cast<char*>(&net_len) for ARM32.
      {
        char hdr_bytes[4];
        std::memcpy(hdr_bytes, &net_len, sizeof(net_len));
        raw_msg.append(hdr_bytes, hdr_read);
      }

      std::vector<char> buffer(4096);
      ssize_t bytes_read;
      while ((bytes_read = ::read(client_sock, buffer.data(), buffer.size())) >
             0) {
        raw_msg.append(buffer.data(), bytes_read);
      }
    } else {
      // Client disconnected (0) or error (-1)
      break;
    }

    if (raw_msg.empty() || !agent_) {
      break;
    }

    LOG(INFO) << "Received IPC msg (" << raw_msg.size() << " bytes)";

    // Parse JSON and process
    nlohmann::json response_json;
    try {
      auto req = nlohmann::json::parse(raw_msg);

      // JSON-RPC 2.0 Check
      if (req.value("jsonrpc", "") != "2.0" || !req.contains("method")) {
        response_json = {
            {"jsonrpc", "2.0"},
            {"error", {{"code", -32600}, {"message", "Invalid Request"}}},
            {"id", req.value("id", nlohmann::json(nullptr))}};
      } else {
        std::string method = req.value("method", "");
        nlohmann::json params = req.value("params", nlohmann::json::object());
        nlohmann::json req_id = req.value("id", nlohmann::json(nullptr));

        // Handle get_usage method
        if (method == "get_usage") {
          std::string usage_type = params.value("type", "daily");
          auto& store = agent_->GetSessionStore();

          if (usage_type == "session") {
            std::string sid = params.value("session_id", "default");
            auto s = store.LoadTokenUsage(sid);
            response_json = {{"jsonrpc", "2.0"},
                             {"id", req_id},
                             {"result",
                              {{"usage_type", "session"},
                               {"session_id", sid},
                               {"prompt_tokens", s.total_prompt_tokens},
                               {"completion_tokens", s.total_completion_tokens},
                               {"entries", (int)s.entries.size()}}}};
          } else if (usage_type == "monthly") {
            std::string month = params.value("month", "");
            auto s = store.LoadMonthlyUsage(month);
            response_json = {{"jsonrpc", "2.0"},
                             {"id", req_id},
                             {"result",
                              {{"usage_type", "monthly"},
                               {"month", month},
                               {"prompt_tokens", s.total_prompt_tokens},
                               {"completion_tokens", s.total_completion_tokens},
                               {"total_requests", s.total_requests}}}};
          } else {
            // Default: daily
            std::string date = params.value("date", "");
            auto s = store.LoadDailyUsage(date);
            response_json = {{"jsonrpc", "2.0"},
                             {"id", req_id},
                             {"result",
                              {{"usage_type", "daily"},
                               {"date", date},
                               {"prompt_tokens", s.total_prompt_tokens},
                               {"completion_tokens", s.total_completion_tokens},
                               {"total_requests", s.total_requests}}}};
          }
        } else if (method == "prompt") {
          std::string session_id = params.value("session_id", "default");
          std::string prompt = params.value("text", "");
          bool stream_requested = params.value("stream", false);

          if (prompt.empty()) {
            response_json = {
                {"jsonrpc", "2.0"},
                {"error", {{"code", -32602}, {"message", "Empty prompt"}}},
                {"id", req_id}};
          } else {
            std::function<void(const std::string&)> on_chunk = nullptr;
            if (stream_requested) {
              on_chunk = [client_sock](const std::string& chunk) {
                nlohmann::json cj = {{"jsonrpc", "2.0"},
                                     {"method", "stream_chunk"},
                                     {"params", {{"text", chunk}}}};
                std::string cs = cj.dump();
                uint32_t cl = htonl(cs.size());
                if (::write(client_sock, &cl, 4) == 4) {
                  ssize_t t = 0;
                  auto sz = static_cast<ssize_t>(cs.size());
                  while (t < sz) {
                    auto w = ::write(client_sock, cs.data() + t, sz - t);
                    if (w <= 0) break;
                    t += w;
                  }
                }
              };
            }

            std::string result =
                agent_->ProcessPrompt(session_id, prompt, on_chunk);
            response_json = {{"jsonrpc", "2.0"},
                             {"id", req_id},
                             {"result", {{"text", result}}}};
          }
        } else {
          response_json = {
              {"jsonrpc", "2.0"},
              {"error", {{"code", -32601}, {"message", "Method not found"}}},
              {"id", req_id}};
        }
      }  // end else valid json-rpc
    } catch (const nlohmann::json::exception& e) {
      LOG(WARNING) << "Non-JSON IPC msg, treating as plain text";
      std::string result = agent_->ProcessPrompt("default", raw_msg);
      response_json = {{"jsonrpc", "2.0"},
                       {"id", nlohmann::json(nullptr)},
                       {"result", {{"text", result}}}};
    } catch (const std::exception& e) {
      LOG(ERROR) << "IPC processing error: " << e.what();
      response_json = {
          {"jsonrpc", "2.0"},
          {"id", nlohmann::json(nullptr)},
          {"error",
           {{"code", -32000},
            {"message", std::string("Internal error: ") + e.what()}}}};
    }

    // Write response back to client (with 4-byte length prefix)
    std::string resp_str = response_json.dump();
    uint32_t resp_len_net = htonl(resp_str.size());

    // Write header
    if (::write(client_sock, &resp_len_net, 4) != 4) {
      LOG(WARNING) << "Failed to write IPC header";
      break;
    }

    // Write payload
    ssize_t total = 0;
    auto len = static_cast<ssize_t>(resp_str.size());
    while (total < len) {
      ssize_t written =
          ::write(client_sock, resp_str.data() + total, len - total);
      if (written <= 0) {
        LOG(WARNING) << "Failed to write IPC "
                     << "response: " << strerror(errno);
        break;
      }
      total += written;
    }

    // In legacy mode, we must break after one message
    if (hdr_read != 4) {
      break;
    }
  }

  close(client_sock);
  LOG(INFO) << "IPC client disconnected";
}

bool TizenClawDaemon::IsAllowedUid(uid_t uid) const {
  return std::ranges::any_of(kAllowedUids,
                             [uid](uid_t allowed) { return uid == allowed; });
}

constexpr uid_t TizenClawDaemon::kAllowedUids[];

}  // namespace tizenclaw

#include "../../common/file_log_backend.hh"
#include "../channel/mcp_server.hh"

int main(int argc, char* argv[]) {
  using namespace tizenclaw;

  // Add file-based logging for debugging
  // Path is inside the container rootfs (always writable).
  try {
    const std::string log_dir = "/opt/usr/share/tizenclaw/logs";
    if (!std::filesystem::exists(log_dir))
      std::filesystem::create_directories(log_dir);
    tizenclaw::utils::LogCore::GetCore().AddLogBackend(
        std::make_shared<tizenclaw::utils::FileLogBackend>(
            log_dir + "/tizenclaw.log", 1024 * 1024, 3));
  } catch (const std::exception& e) {
    LOG(ERROR) << "Failed to initialize file log: " << e.what();
  }

  // --mcp-stdio mode: run MCP Server on stdio
  // without daemon event loop
  if (argc > 1 && std::string(argv[1]) == "--mcp-stdio") {
    LOG(INFO) << "Starting MCP stdio mode...";
    PluginManager::GetInstance().Initialize();
    AgentCore agent;
    if (!agent.Initialize()) {
      LOG(ERROR) << "Failed to initialize " << "AgentCore for MCP";
      return -1;
    }
    McpServer mcp(&agent);
    mcp.RunStdio();
    agent.Shutdown();
    PluginManager::GetInstance().Shutdown();
    return 0;
  }

  // --encrypt-keys mode: encrypt plaintext API
  // keys in llm_config.json in-place
  if (argc > 1 && std::string(argv[1]) == "--encrypt-keys") {
    std::string config_path =
        "/opt/usr/share/tizenclaw/config/"
        "llm_config.json";
    if (argc > 2) config_path = argv[2];
    LOG(INFO) << "Encrypting keys in: " << config_path;
    bool ok = KeyStore::EncryptConfig(config_path);
    return ok ? 0 : 1;
  }

  LOG(INFO) << "TizenClaw Service starting...";
  try {
    TizenClawDaemon daemon(argc, argv);
    g_daemon = &daemon;
    return daemon.Run();
  } catch (const std::exception& e) {
    LOG(ERROR) << "Exception: " << e.what();
    return -1;
  } catch (...) {
    LOG(ERROR) << "Unknown exception";
    return -1;
  }
}
