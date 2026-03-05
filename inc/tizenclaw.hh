#ifndef __TIZENCLAW_H__
#define __TIZENCLAW_H__

#include <dlog.h>
#include <tizen_core.h>
#include <json.hpp>
#include <thread>
#include <atomic>
#include "agent_core.hh"
#include "telegram_bridge.hh"

#ifdef  LOG_TAG
#undef  LOG_TAG
#endif
#define LOG_TAG "TizenClaw"

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
    bool IsAllowedUid(uid_t uid) const;

    int argc_;
    char** argv_;
    tizen_core_task_h task_ = nullptr;
    AgentCore* agent_ = nullptr;
    
    std::thread ipc_thread_;
    std::atomic<bool> ipc_running_{false};
    int ipc_socket_ = -1;
    TelegramBridge* telegram_bridge_ = nullptr;

    // Allowed UIDs for IPC connections
    // 0=root, 301=app_fw, 200=system, 5001=developer
    static constexpr uid_t kAllowedUids[] = {
        0, 200, 301, 5001
    };
};

#endif // __TIZENCLAW_H__
