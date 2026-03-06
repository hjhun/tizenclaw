#include "tizenclaw.hh"
#include "key_store.hh"
#include "audit_logger.hh"

#include <iostream>
#include <string>
#include <csignal>
#include <exception>
#include <sys/socket.h>
#include <arpa/inet.h>
#include <sys/un.h>
#include <unistd.h>
#include <vector>
#include <arpa/inet.h>
#include <algorithm>

namespace tizenclaw {

TizenClawDaemon* g_daemon = nullptr;

void signal_handler(int sig) {
    LOG(INFO) << "Caught signal " << sig;
    if (g_daemon) {
        g_daemon->Quit();
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
    OnDestroy();
    return ret;
}

void TizenClawDaemon::Quit() {
    LOG(INFO) << "TizenClaw Daemon Quit";
    if (task_) {
        tizen_core_task_quit(task_);
    }
}

void TizenClawDaemon::OnCreate() {
    LOG(INFO) << "TizenClaw Daemon OnCreate";
    agent_ = new AgentCore();
    if (!agent_->Initialize()) {
        LOG(ERROR) << "Failed to initialize AgentCore";
    }

    // Register channels
    channel_registry_.Register(
        std::make_unique<McpServer>(agent_));
    channel_registry_.Register(
        std::make_unique<TelegramClient>(agent_));
    channel_registry_.Register(
        std::make_unique<WebhookChannel>(agent_));
    channel_registry_.Register(
        std::make_unique<SlackChannel>(agent_));
    channel_registry_.Register(
        std::make_unique<DiscordChannel>(agent_));
    channel_registry_.StartAll();

    // Initialize Task Scheduler
    scheduler_ = new TaskScheduler();
    agent_->SetScheduler(scheduler_);
    scheduler_->Start(agent_);
    
    ipc_running_ = true;
    ipc_thread_ = std::thread(
        &TizenClawDaemon::IpcServerLoop, this);

    // Start Skill Watcher (inotify)
    skill_watcher_.Start(
        "/opt/usr/share/tizenclaw/skills",
        [this]() {
          if (agent_) {
            agent_->ReloadSkills();
          }
        });
}

void TizenClawDaemon::OnDestroy() {
    LOG(INFO) << "TizenClaw Daemon OnDestroy";

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
        std::lock_guard<std::mutex> lock(
            threads_mutex_);
        for (auto& t : client_threads_) {
            if (t.joinable()) {
                t.join();
            }
        }
        client_threads_.clear();
    }

    if (agent_) {
        agent_->SetScheduler(nullptr);
        agent_->Shutdown();
        delete agent_;
        agent_ = nullptr;
    }

    // Stop Task Scheduler
    if (scheduler_) {
        scheduler_->Stop();
        delete scheduler_;
        scheduler_ = nullptr;
    }
}

void TizenClawDaemon::IpcServerLoop() {
    LOG(INFO) << "IPC Server thread starting...";

    int sock = socket(AF_UNIX, SOCK_STREAM, 0);
    if (sock < 0) {
        LOG(ERROR) << "Failed to create IPC socket: " << strerror(errno);
        return;
    }
    ipc_socket_ = sock;

    struct sockaddr_un addr;
    std::memset(&addr, 0, sizeof(addr));
    addr.sun_family = AF_UNIX;

    // Abstract namespace socket: "\0tizenclaw.sock"
    const char kSocketName[] = "tizenclaw.sock";
    constexpr size_t kNameLen =
        1 + sizeof(kSocketName) - 1;
    std::memcpy(addr.sun_path + 1, kSocketName,
                sizeof(kSocketName) - 1);

    socklen_t addr_len =
        offsetof(struct sockaddr_un, sun_path) +
        kNameLen;

    if (bind(ipc_socket_,
             (struct sockaddr*)&addr,
             addr_len) < 0) {
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

    LOG(INFO) << "IPC Server listening on \\0tizenclaw.sock (addr_len=" << addr_len << ")";

    while (ipc_running_) {
        int client_sock =
            accept(ipc_socket_, nullptr, nullptr);
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
        if (getsockopt(client_sock, SOL_SOCKET,
                       SO_PEERCRED, &cred,
                       &cred_len) < 0) {
            LOG(ERROR) << "Failed to get peer cred: " << strerror(errno);
            close(client_sock);
            continue;
        }

        if (!IsAllowedUid(cred.uid)) {
            LOG(WARNING) << "Rejected IPC from uid=" << cred.uid << " pid=" << cred.pid;
            AuditLogger::Instance().Log(
                AuditLogger::MakeEvent(
                    AuditEventType::kIpcAuth,
                    "",
                    {{"uid",
                      static_cast<int>(
                          cred.uid)},
                     {"pid",
                      static_cast<int>(
                          cred.pid)},
                     {"allowed", false}}));
            close(client_sock);
            continue;
        }

        LOG(INFO) << "Authorized IPC from pid=" << cred.pid << " uid=" << cred.uid;
        AuditLogger::Instance().Log(
            AuditLogger::MakeEvent(
                AuditEventType::kIpcAuth,
                "",
                {{"uid",
                  static_cast<int>(cred.uid)},
                 {"pid",
                  static_cast<int>(cred.pid)},
                 {"allowed", true}}));

        // Check concurrent client limit
        if (active_clients_.load() >= kMaxConcurrentClients) {
            LOG(WARNING) << "Max concurrent clients reached (" << kMaxConcurrentClients << "), rejecting";
            nlohmann::json busy = {
                {"type", "response"},
                {"status", "error"},
                {"text", "Server busy, try again later"}
            };
            std::string busy_str = busy.dump();
            uint32_t busy_len = htonl(busy_str.size());
            ::write(client_sock, &busy_len, 4);
            ::write(client_sock, busy_str.data(), busy_str.size());
            close(client_sock);
            continue;
        }

        // Spawn tracked thread to handle this client
        {
            std::lock_guard<std::mutex> lock(threads_mutex_);
            // Clean up finished threads
            client_threads_.erase(
                std::remove_if(client_threads_.begin(),
                               client_threads_.end(),
                               [](std::thread& t) {
                                   if (t.joinable()) {
                                       // Can't check if done without extra state,
                                       // so try_join is not available in C++17.
                                       // We'll use detach-after-decrement approach instead.
                                       return false;
                                   }
                                   return true;
                               }),
                client_threads_.end());

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
            if (len > 10 * 1024 * 1024) { // 10MB limit
                LOG(ERROR) << "IPC Payload too large: " << len;
                break;
            }
            
            std::vector<char> buffer(len);
            ssize_t body_read = ::recv(client_sock, buffer.data(), len, MSG_WAITALL);
            if (body_read != len) {
                LOG(ERROR) << "Incomplete IPC payload read";
                break;
            }
            raw_msg.assign(buffer.data(), len);
        } else if (hdr_read > 0) {
            // Fallback: Legacy EOF-based protocol
            // We read 1-3 bytes into net_len by accident, append it
            raw_msg.append(reinterpret_cast<char*>(&net_len), hdr_read);
            
            std::vector<char> buffer(4096);
            ssize_t bytes_read;
            while ((bytes_read = ::read(client_sock, buffer.data(), buffer.size())) > 0) {
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

            std::string session_id = req.value("session_id", "default");
            std::string command = req.value("command", "");

            // Handle get_usage command
            if (command == "get_usage") {
                std::string usage_type =
                    req.value("type", "daily");
                auto& store =
                    agent_->GetSessionStore();

                if (usage_type == "session") {
                    std::string sid =
                        req.value("session_id",
                                  "default");
                    auto s = store.LoadTokenUsage(
                        sid);
                    response_json = {
                        {"type", "usage"},
                        {"usage_type", "session"},
                        {"session_id", sid},
                        {"prompt_tokens",
                         s.total_prompt_tokens},
                        {"completion_tokens",
                         s.total_completion_tokens},
                        {"entries",
                         (int)s.entries.size()},
                        {"status", "ok"}};
                } else if (usage_type == "monthly") {
                    std::string month =
                        req.value("month", "");
                    auto s =
                        store.LoadMonthlyUsage(
                            month);
                    response_json = {
                        {"type", "usage"},
                        {"usage_type", "monthly"},
                        {"month", month},
                        {"prompt_tokens",
                         s.total_prompt_tokens},
                        {"completion_tokens",
                         s.total_completion_tokens},
                        {"total_requests",
                         s.total_requests},
                        {"status", "ok"}};
                } else {
                    // Default: daily
                    std::string date =
                        req.value("date", "");
                    auto s =
                        store.LoadDailyUsage(date);
                    response_json = {
                        {"type", "usage"},
                        {"usage_type", "daily"},
                        {"date", date},
                        {"prompt_tokens",
                         s.total_prompt_tokens},
                        {"completion_tokens",
                         s.total_completion_tokens},
                        {"total_requests",
                         s.total_requests},
                        {"status", "ok"}};
                }
            } else {
            // Normal prompt processing
            std::string prompt =
                req.value("text", "");
            bool stream_requested =
                req.value("stream", false);

            if (prompt.empty()) {
                response_json = {
                    {"type", "response"},
                    {"session_id", session_id},
                    {"status", "error"},
                    {"text", "Empty prompt"}
                };
            } else {
                std::function<void(const std::string&)> on_chunk = nullptr;
                if (stream_requested) {
                    on_chunk = [client_sock, session_id](const std::string& chunk) {
                        nlohmann::json chunk_json = {
                            {"type", "stream_chunk"},
                            {"session_id", session_id},
                            {"text", chunk}
                        };
                        std::string chunk_str = chunk_json.dump();
                        uint32_t chunk_len_net = htonl(chunk_str.size());
                        
                        if (::write(client_sock, &chunk_len_net, 4) == 4) {
                            ssize_t total = 0;
                            ssize_t len = static_cast<ssize_t>(chunk_str.size());
                            while (total < len) {
                                ssize_t written = ::write(client_sock, chunk_str.data() + total, len - total);
                                if (written <= 0) break;
                                total += written;
                            }
                        }
                    };
                }

                std::string result = agent_->ProcessPrompt(session_id, prompt, on_chunk);
                response_json = {
                    {"type", stream_requested ? "stream_end" : "response"},
                    {"session_id", session_id},
                    {"status", "ok"},
                    {"text", result}
                };
            }
            } // end else (normal prompt)
        } catch (const nlohmann::json::exception& e) {
            LOG(WARNING) << "Non-JSON IPC msg, treating as plain text";
            std::string result = agent_->ProcessPrompt("default", raw_msg);
            response_json = {
                {"type", "response"},
                {"session_id", "default"},
                {"status", "ok"},
                {"text", result}
            };
        } catch (const std::exception& e) {
            LOG(ERROR) << "IPC processing error: " << e.what();
            response_json = {
                {"type", "response"},
                {"session_id", "default"},
                {"status", "error"},
                {"text", std::string("Internal error: ") + e.what()}
            };
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
        ssize_t len = static_cast<ssize_t>(resp_str.size());
        while (total < len) {
            ssize_t written = ::write(client_sock, resp_str.data() + total, len - total);
            if (written <= 0) {
                LOG(WARNING) << "Failed to write IPC response: " << strerror(errno);
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

bool TizenClawDaemon::IsAllowedUid(
    uid_t uid) const {
  for (auto allowed : kAllowedUids) {
    if (uid == allowed) return true;
  }
  return false;
}

constexpr uid_t TizenClawDaemon::kAllowedUids[];

} // namespace tizenclaw

#include "../common/file_log_backend.hh"
#include "mcp_server.hh"

int main(int argc, char *argv[]) {
    using namespace tizenclaw;

    // Add file-based logging (reliable inside chroot where dlog is unavailable)
    tizenclaw::utils::LogCore::GetCore().AddLogBackend(
        std::make_shared<tizenclaw::utils::FileLogBackend>(
            "/tmp/tizenclaw.log", 1024 * 1024, 3));

    // --mcp-stdio mode: run MCP Server on stdio
    // without daemon event loop
    if (argc > 1 &&
        std::string(argv[1]) == "--mcp-stdio") {
        LOG(INFO) << "Starting MCP stdio mode...";
        AgentCore agent;
        if (!agent.Initialize()) {
            LOG(ERROR) << "Failed to initialize "
                       << "AgentCore for MCP";
            return -1;
        }
        McpServer mcp(&agent);
        mcp.RunStdio();
        agent.Shutdown();
        return 0;
    }

    // --encrypt-keys mode: encrypt plaintext API
    // keys in llm_config.json in-place
    if (argc > 1 &&
        std::string(argv[1]) ==
            "--encrypt-keys") {
        std::string config_path =
            "/opt/usr/share/tizenclaw/config/"
            "llm_config.json";
        if (argc > 2) config_path = argv[2];
        LOG(INFO) << "Encrypting keys in: "
                  << config_path;
        bool ok =
            KeyStore::EncryptConfig(config_path);
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
