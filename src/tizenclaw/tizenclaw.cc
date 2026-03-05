#include "tizenclaw.hh"

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

    // TODO: Initialize LXC Container Engine
    // TODO: Start MCP Server connection
    
    ipc_running_ = true;
    ipc_thread_ = std::thread(&TizenClawDaemon::IpcServerLoop, this);

    // Start Native Telegram Client
    telegram_client_ = new TelegramClient(agent_);
    if (!telegram_client_->Start()) {
        LOG(WARNING) << "Telegram client not started (config may be missing)";
    }
}

void TizenClawDaemon::OnDestroy() {
    LOG(INFO) << "TizenClaw Daemon OnDestroy";

    // Stop Native Telegram Client
    if (telegram_client_) {
        telegram_client_->Stop();
        delete telegram_client_;
        telegram_client_ = nullptr;
    }

    ipc_running_ = false;
    if (ipc_socket_ != -1) {
        shutdown(ipc_socket_, SHUT_RDWR);
        close(ipc_socket_);
        ipc_socket_ = -1;
    }
    if (ipc_thread_.joinable()) {
        ipc_thread_.join();
    }

    if (agent_) {
        agent_->Shutdown();
        delete agent_;
        agent_ = nullptr;
    }
    
    // TODO: Cleanup LXC processes and MCP sockets here
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

    // Abstract namespace socket: "\0tizenclaw.ipc"
    const char kSocketName[] = "tizenclaw.ipc";
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

    LOG(INFO) << "IPC Server listening on \\0tizenclaw.ipc (addr_len=" << addr_len << ")";

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
            close(client_sock);
            continue;
        }

        LOG(INFO) << "Authorized IPC from pid=" << cred.pid << " uid=" << cred.uid;

        // Spawn detached thread to handle this client
        std::thread([this, client_sock]() {
            HandleIpcClient(client_sock);
        }).detach();
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
            std::string prompt = req.value("text", "");
            bool stream_requested = req.value("stream", false);

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

int main(int argc, char *argv[]) {
    using namespace tizenclaw;

    // Add file-based logging (reliable inside chroot where dlog is unavailable)
    tizenclaw::utils::LogCore::GetCore().AddLogBackend(
        std::make_shared<tizenclaw::utils::FileLogBackend>(
            "/tmp/tizenclaw.log", 1024 * 1024, 3));

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
