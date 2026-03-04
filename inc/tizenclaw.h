#ifndef __TIZENCLAW_H__
#define __TIZENCLAW_H__

#include <dlog.h>
#include <tizen_core.h>
#include "agent_core.h"

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

    int argc_;
    char** argv_;
    tizen_core_task_h task_ = nullptr;
    AgentCore* agent_ = nullptr;
};

#endif // __TIZENCLAW_H__
