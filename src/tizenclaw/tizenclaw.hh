#ifndef __TIZENCLAW_H__
#define __TIZENCLAW_H__

#include <tizen_core.h>
#include <json.hpp>
#include <thread>
#include <atomic>
#include <vector>
#include <mutex>
#include "agent_core.hh"
#include "telegram_client.hh"
#include "mcp_server.hh"
#include "task_scheduler.hh"
#include "channel_registry.hh"
#include "skill_watcher.hh"
#include "webhook_channel.hh"
#include "slack_channel.hh"
#include "discord_channel.hh"
#include "../common/logging.hh"

namespace tizenclaw {


class TizenClawDaemon {
public:
    TizenClawDaemon(int argc, char** argv);
    ~TizenClawDaemon();

    int Run();
    void Quit();

private:
    void OnCreate();
    void OnDestroy();
    void IpcServerLoop();
    void HandleIpcClient(int client_sock);
    bool IsAllowedUid(uid_t uid) const;

    int argc_;
    char** argv_;
    tizen_core_task_h task_ = nullptr;
    AgentCore* agent_ = nullptr;
    
    std::thread ipc_thread_;
    int ipc_socket_;
    bool ipc_running_;
    TaskScheduler* scheduler_ = nullptr;
    ChannelRegistry channel_registry_;
    SkillWatcher skill_watcher_;

    // Concurrency control
    std::atomic<int> active_clients_{0};
    static constexpr int kMaxConcurrentClients = 4;
    std::vector<std::thread> client_threads_;
    std::mutex threads_mutex_;

    // Allowed UIDs for IPC connections
    // 0=root, 301=app_fw, 200=system, 5001=developer
    static constexpr uid_t kAllowedUids[] = {
        0, 200, 301, 5001
    };
};

} // namespace tizenclaw

#endif // __TIZENCLAW_H__
