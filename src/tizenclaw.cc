#include "tizenclaw.hh"

#include <iostream>
#include <string>
#include <csignal>
#include <exception>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>
#include <cstring>
#include <vector>

TizenClawDaemon* g_daemon = nullptr;

void signal_handler(int sig) {
    dlog_print(DLOG_INFO, LOG_TAG, "Caught signal %d", sig);
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
    dlog_print(DLOG_INFO, LOG_TAG, "TizenClaw Daemon Run");
    OnCreate();
    
    // Set up signal handling
    std::signal(SIGINT, signal_handler);
    std::signal(SIGTERM, signal_handler);

    int ret = tizen_core_task_run(task_);
    OnDestroy();
    return ret;
}

void TizenClawDaemon::Quit() {
    dlog_print(DLOG_INFO, LOG_TAG, "TizenClaw Daemon Quit");
    if (task_) {
        tizen_core_task_quit(task_);
    }
}

void TizenClawDaemon::OnCreate() {
    dlog_print(DLOG_INFO, LOG_TAG, "TizenClaw Daemon OnCreate");
    agent_ = new AgentCore();
    if (!agent_->Initialize()) {
        dlog_print(DLOG_ERROR, LOG_TAG, "Failed to initialize AgentCore");
    }

    // TODO: Initialize LXC Container Engine
    // TODO: Start MCP Server connection
    
    ipc_running_ = true;
    ipc_thread_ = std::thread(&TizenClawDaemon::IpcServerLoop, this);

    // Start Telegram bridge (non-fatal if config is missing)
    telegram_bridge_ = new TelegramBridge();
    if (!telegram_bridge_->Start()) {
        dlog_print(DLOG_WARN, LOG_TAG,
                   "Telegram bridge not started "
                   "(config may be missing)");
    }
}

void TizenClawDaemon::OnDestroy() {
    dlog_print(DLOG_INFO, LOG_TAG, "TizenClaw Daemon OnDestroy");

    // Stop Telegram bridge first
    if (telegram_bridge_) {
        telegram_bridge_->Stop();
        delete telegram_bridge_;
        telegram_bridge_ = nullptr;
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
    dlog_print(DLOG_INFO, LOG_TAG,
               "IPC Server thread starting...");

    int sock = socket(AF_UNIX, SOCK_STREAM, 0);
    if (sock < 0) {
        dlog_print(DLOG_ERROR, LOG_TAG,
                   "Failed to create IPC socket: %s",
                   strerror(errno));
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
        dlog_print(DLOG_ERROR, LOG_TAG,
                   "Failed to bind IPC socket: %s",
                   strerror(errno));
        close(ipc_socket_);
        ipc_socket_ = -1;
        return;
    }

    if (listen(ipc_socket_, 5) < 0) {
        dlog_print(DLOG_ERROR, LOG_TAG,
                   "Failed to listen IPC socket: %s",
                   strerror(errno));
        close(ipc_socket_);
        ipc_socket_ = -1;
        return;
    }

    dlog_print(DLOG_INFO, LOG_TAG,
               "IPC Server listening on "
               "\\0tizenclaw.ipc (addr_len=%d)",
               addr_len);

    while (ipc_running_) {
        int client_sock =
            accept(ipc_socket_, nullptr, nullptr);
        if (client_sock < 0) {
            if (ipc_running_) {
                dlog_print(DLOG_WARN, LOG_TAG,
                           "accept() failed: %s",
                           strerror(errno));
            }
            continue;
        }

        dlog_print(DLOG_INFO, LOG_TAG,
                   "IPC client connected");

        // --- Peer credential verification ---
        struct ucred cred;
        socklen_t cred_len = sizeof(cred);
        if (getsockopt(client_sock, SOL_SOCKET,
                       SO_PEERCRED, &cred,
                       &cred_len) < 0) {
            dlog_print(DLOG_ERROR, LOG_TAG,
                       "Failed to get peer cred: %s",
                       strerror(errno));
            close(client_sock);
            continue;
        }

        if (!IsAllowedUid(cred.uid)) {
            dlog_print(DLOG_WARN, LOG_TAG,
                       "Rejected IPC from "
                       "uid=%d pid=%d",
                       cred.uid, cred.pid);
            close(client_sock);
            continue;
        }

        dlog_print(DLOG_INFO, LOG_TAG,
                   "Authorized IPC from "
                   "pid=%d uid=%d",
                   cred.pid, cred.uid);

        // Read all data until client signals EOF
        std::vector<char> buffer(4096);
        std::string raw_msg;
        ssize_t bytes_read;
        while ((bytes_read = ::read(
                    client_sock, buffer.data(),
                    buffer.size())) > 0) {
            raw_msg.append(buffer.data(),
                           bytes_read);
        }

        if (raw_msg.empty() || !agent_) {
            close(client_sock);
            continue;
        }

        dlog_print(DLOG_INFO, LOG_TAG,
                   "Received IPC msg (%zu bytes)",
                   raw_msg.size());

        // Parse JSON and process
        nlohmann::json response_json;
        try {
            auto req =
                nlohmann::json::parse(raw_msg);

            std::string session_id =
                req.value("session_id", "default");
            std::string prompt =
                req.value("text", "");

            if (prompt.empty()) {
                response_json = {
                    {"type", "response"},
                    {"session_id", session_id},
                    {"status", "error"},
                    {"text", "Empty prompt"}
                };
            } else {
                std::string result =
                    agent_->ProcessPrompt(
                        session_id, prompt);
                response_json = {
                    {"type", "response"},
                    {"session_id", session_id},
                    {"status", "ok"},
                    {"text", result}
                };
            }
        } catch (const nlohmann::json::exception& e) {
            // Fallback: treat as plain text prompt
            dlog_print(DLOG_WARN, LOG_TAG,
                       "Non-JSON IPC msg, "
                       "treating as plain text");
            std::string result =
                agent_->ProcessPrompt(
                    "default", raw_msg);
            response_json = {
                {"type", "response"},
                {"session_id", "default"},
                {"status", "ok"},
                {"text", result}
            };
        } catch (const std::exception& e) {
            dlog_print(DLOG_ERROR, LOG_TAG,
                       "IPC processing error: %s",
                       e.what());
            response_json = {
                {"type", "response"},
                {"session_id", "default"},
                {"status", "error"},
                {"text", std::string("Internal "
                         "error: ") + e.what()}
            };
        }

        // Write response back to client
        std::string resp_str =
            response_json.dump();
        ssize_t total = 0;
        ssize_t len =
            static_cast<ssize_t>(resp_str.size());
        while (total < len) {
            ssize_t written = ::write(
                client_sock,
                resp_str.data() + total,
                len - total);
            if (written <= 0) {
                dlog_print(DLOG_WARN, LOG_TAG,
                           "Failed to write IPC "
                           "response: %s",
                           strerror(errno));
                break;
            }
            total += written;
        }

        close(client_sock);
        dlog_print(DLOG_INFO, LOG_TAG,
                   "IPC response sent (%zd bytes)",
                   total);
    }

    dlog_print(DLOG_INFO, LOG_TAG,
               "IPC Server thread exiting...");
}

bool TizenClawDaemon::IsAllowedUid(
    uid_t uid) const {
  for (auto allowed : kAllowedUids) {
    if (uid == allowed) return true;
  }
  return false;
}

constexpr uid_t TizenClawDaemon::kAllowedUids[];

int main(int argc, char *argv[]) {
    dlog_print(DLOG_INFO, LOG_TAG, "TizenClaw Service starting...");
    try {
        TizenClawDaemon daemon(argc, argv);
        g_daemon = &daemon;
        return daemon.Run();
    } catch (const std::exception& e) {
        dlog_print(DLOG_ERROR, LOG_TAG, "Exception: %s", e.what());
        return -1;
    } catch (...) {
        dlog_print(DLOG_ERROR, LOG_TAG, "Unknown exception");
        return -1;
    }
}
